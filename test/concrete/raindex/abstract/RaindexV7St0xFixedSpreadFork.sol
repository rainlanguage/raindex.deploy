// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test, console2} from "forge-std-1.16.1/src/Test.sol";
import {IERC20} from "@openzeppelin-contracts-5.6.1/token/ERC20/IERC20.sol";
import {IERC4626} from "@openzeppelin-contracts-5.6.1/interfaces/IERC4626.sol";
import {
    IRaindexV6,
    QuoteV2,
    OrderV4,
    SignedContextV1,
    TakeOrderConfigV4,
    TakeOrdersConfigV5,
    TaskV2,
    Float
} from "raindex-interface-0.1.3/src/interface/IRaindexV6.sol";
import {LibOrder} from "../../../../src/lib/LibOrder.sol";
import {LibDecimalFloat} from "rain-math-float-0.1.1/src/lib/LibDecimalFloat.sol";
import {AlwaysValid1271} from "./AlwaysValid1271.sol";

/// @title RaindexV7St0xFixedSpreadFork
/// @notice Shared Base/Robinhood fork harness for live st0x-fixed-spread-v7
/// SPYM sell and buy orders. Approach A: keep live `ORDER_BYTES`, etch the
/// real oracle signer to EIP-1271 always-valid, build/mutate 10-slot v7 frames.
///
/// Happy path: quote+take derives
/// `underlying * convertToAssets(1 share)` (sell) or
/// `underlying * convertToShares(1 asset)` (buy; oracle underlying is inverted).
/// NAV step between quote and take re-prices (does not revert).
/// Guard matrix: one `expectRevert` per rainlang `ensure` string.
abstract contract RaindexV7St0xFixedSpreadFork is Test {
    using LibOrder for OrderV4;
    using LibDecimalFloat for Float;

    address internal constant ORACLE_SIGNER = 0xdB665B5ef8Bd04Fd977bB3A64790eaa52749ACcc;

    /// Session / expiry window used by the synthetic frame (unix).
    uint256 internal constant ORACLE_PUBLISH_TIME = 0x6a8c5c5b;
    uint256 internal constant SESSION_START = 0x6a8c4758;
    uint256 internal constant SESSION_END = 0x6a8ca2c0;
    uint256 internal constant FRAME_EXPIRY = 0x6a8c5c6f;

    /// IntOrAString V3 encodings: ASCII chars then `0xe0 | length`.
    bytes32 internal constant SESSION_RTH = bytes32(uint256(0x727468e3)); // "rth"
    bytes32 internal constant SESSION_CLOSED = bytes32(uint256(0x636c6f736564e6)); // "closed"

    uint256 internal constant DEPOSIT_SHARES = 1e15;
    /// Enough USDC for buy-order output deposits (6-decimal raw).
    uint256 internal constant DEPOSIT_USDC = 1_000_000e6;
    uint256 internal constant ONE = 1e18;

    // -------- chain-specific fixtures (implemented by concrete forks) --------

    function _rpcEnvKey() internal pure virtual returns (string memory);

    function _rpcFallback() internal pure virtual returns (string memory);

    function _raindex() internal pure virtual returns (address);

    function _usdc() internal pure virtual returns (address);

    function _wtVault() internal pure virtual returns (address);

    function _expectedOrderHash() internal pure virtual returns (bytes32);

    function _orderBytes() internal pure virtual returns (bytes memory);

    function _expectedChainId() internal pure virtual returns (uint256);

    function _usdcDecimals() internal pure virtual returns (uint8);

    /// Pinned tip for deterministic CI when the RPC is archive-capable.
    /// Return `0` to fork at provider head (required when CI only has a
    /// non-archive endpoint, e.g. public Robinhood).
    function _forkBlockNumber() internal pure virtual returns (uint256);

    /// `true` for buy-share orders (vault is input, USDC is output).
    function _isBuy() internal pure virtual returns (bool) {
        return false;
    }

    function setUp() public {
        string memory rpc = vm.envOr(_rpcEnvKey(), _rpcFallback());
        uint256 forkBlock = _forkBlockNumber();
        if (forkBlock == 0) {
            vm.createSelectFork(rpc);
        } else {
            vm.createSelectFork(rpc, forkBlock);
        }
        vm.warp(ORACLE_PUBLISH_TIME);

        // Real signer may carry EIP-7702 code; replace with always-valid 1271.
        AlwaysValid1271 impl = new AlwaysValid1271();
        vm.etch(ORACLE_SIGNER, address(impl).code);
    }

    // ------------------------------ happy path ------------------------------

    function testQuoteAndTakeDerivesFromLiveConvert() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());

        _depositOrderOutput(order);

        (bool success,, Float ioRatio) = _quote(order, signedContext);
        assertTrue(success, "quote should succeed");

        Float expected = _expectedIoRatio();
        _logFloat("expected io", expected);
        _logFloat("actual   io", ioRatio);
        assertTrue(ioRatio.eq(expected), "io-ratio must equal underlying * live vault convert");

        _fundTakerAndApprove();
        (Float totalIn, Float totalOut) = IRaindexV6(_raindex()).takeOrders4(_takeConfig(order, signedContext));
        if (_isBuy()) {
            assertFalse(totalIn.isZero(), "taker should receive USDC");
            assertFalse(totalOut.isZero(), "taker should pay shares");
        } else {
            assertFalse(totalIn.isZero(), "taker should receive shares");
            assertFalse(totalOut.isZero(), "taker should pay USDC");
        }
    }

    function testTakeStillSucceedsWhenNavStepsAfterQuote() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());

        _depositOrderOutput(order);
        (bool success,,) = _quote(order, signedContext);
        assertTrue(success, "quote should succeed before NAV step");

        uint256 navBefore =
            _isBuy() ? IERC4626(_wtVault()).convertToShares(ONE) : IERC4626(_wtVault()).convertToAssets(ONE);
        _donateUnderlyingToVault();
        uint256 navAfter =
            _isBuy() ? IERC4626(_wtVault()).convertToShares(ONE) : IERC4626(_wtVault()).convertToAssets(ONE);
        assertTrue(navAfter != navBefore, "donate must change live vault convert(1e18)");
        console2.log("navBefore", navBefore);
        console2.log("navAfter", navAfter);

        (, Float ioRatioAfter) = _requoteRatio(order, signedContext);
        Float expectedAfter = _expectedIoRatio();
        assertTrue(ioRatioAfter.eq(expectedAfter), "post-step io must track live NAV");

        _fundTakerAndApprove();
        (Float totalIn, Float totalOut) = IRaindexV6(_raindex()).takeOrders4(_takeConfig(order, signedContext));
        assertFalse(totalIn.isZero(), "fill still executes after NAV step");
        assertFalse(totalOut.isZero(), "fill still executes after NAV step");

        Float settlementRatio = totalOut.div(totalIn);
        assertTrue(settlementRatio.gte(expectedAfter), "settlement must not undercut re-priced io");
        Float tolerance = expectedAfter.mul(LibDecimalFloat.packLossless(1, -4));
        assertTrue(
            settlementRatio.lte(expectedAfter.add(tolerance)), "settlement must track re-priced io, not a stale ratio"
        );
    }

    // --------------------------- signed-context guards ---------------------------

    function testRevertUntrustedOracleSigner() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());
        // Signature must still clear LibContext: etch a second always-valid
        // 1271 at a non-bound address so the rainlang signer ensure fires.
        address badSigner = address(0xB0B);
        vm.etch(badSigner, address(new AlwaysValid1271()).code);
        signedContext[0].signer = badSigner;
        _expectEnsureRevert(order, signedContext, "Untrusted oracle signer");
    }

    function testRevertOracleSchemaVersionMismatch() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());
        signedContext[0].context[0] = bytes32(uint256(6));
        _expectEnsureRevert(order, signedContext, "Oracle schema version mismatch");
    }

    function testRevertWrongChain() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());
        signedContext[0].context[9] = bytes32(uint256(1)); // ethereum mainnet
        _expectEnsureRevert(order, signedContext, "Wrong chain");
    }

    function testRevertOracleInputTokenMismatch() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());
        signedContext[0].context[6] = bytes32(uint256(uint160(address(0xdead))));
        _expectEnsureRevert(order, signedContext, "Oracle input token mismatch");
    }

    function testRevertOracleOutputTokenMismatch() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());
        signedContext[0].context[7] = bytes32(uint256(uint160(address(0xdead))));
        _expectEnsureRevert(order, signedContext, "Oracle output token mismatch");
    }

    function testRevertNotInActiveMarketSession() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());
        signedContext[0].context[3] = SESSION_CLOSED;
        _expectEnsureRevert(order, signedContext, "Not in active market session");
    }

    function testRevertBeforeSessionStart() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());
        vm.warp(SESSION_START - 1);
        _expectEnsureRevert(order, signedContext, "Before session start");
    }

    function testRevertAfterSessionEnd() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());
        // Keep expiry ahead of session end so this hits the session-end guard.
        signedContext[0].context[8] = bytes32(uint256(SESSION_END + 100));
        vm.warp(SESSION_END + 1);
        _expectEnsureRevert(order, signedContext, "After session end");
    }

    function testRevertPriceBeforeSessionStart() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());
        signedContext[0].context[2] = bytes32(uint256(SESSION_START - 1));
        _expectEnsureRevert(order, signedContext, "Price before session start");
    }

    function testRevertPastOracleExpiry() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(_underlyingPrice());
        signedContext[0].context[8] = bytes32(uint256(ORACLE_PUBLISH_TIME - 1));
        _expectEnsureRevert(order, signedContext, "Past oracle expiry");
    }

    function testRevertDerivedPriceIsZero() external {
        OrderV4 memory order = _order();
        SignedContextV1[] memory signedContext = _signedContext(LibDecimalFloat.packLossless(0, 0));
        _expectEnsureRevert(order, signedContext, "Derived price is zero");
    }

    function testRevertPriceBelowMinimumBound() external {
        OrderV4 memory order = _order();
        // Tiny positive underlying so derived vault price is below the live order's min-price.
        SignedContextV1[] memory signedContext = _signedContext(LibDecimalFloat.packLossless(1, -18));
        _expectEnsureRevert(order, signedContext, "Price below minimum bound");
    }

    function testRevertPriceAboveMaximumBound() external {
        OrderV4 memory order = _order();
        // Huge underlying so derived vault price exceeds the live order's max-price.
        SignedContextV1[] memory signedContext = _signedContext(LibDecimalFloat.packLossless(1_000_000_000, 0));
        _expectEnsureRevert(order, signedContext, "Price above maximum bound");
    }

    // ------------------------------ helpers ------------------------------

    /// Sell: quote-per-underlying (100). Buy: oracle signs the inverse
    /// orientation (0.01); both multiply by the matching vault convert.
    function _underlyingPrice() internal pure returns (Float) {
        if (_isBuy()) {
            return LibDecimalFloat.packLossless(1, -2);
        }
        return LibDecimalFloat.packLossless(100, 0);
    }

    function _expectedIoRatio() internal view returns (Float) {
        Float convert =
            _isBuy() ? _erc4626ConvertToSharesFloat(_wtVault(), ONE) : _erc4626ConvertToAssetsFloat(_wtVault(), ONE);
        return _underlyingPrice().mul(convert);
    }

    function _order() internal pure returns (OrderV4 memory order) {
        order = abi.decode(_orderBytes(), (OrderV4));
        require(order.hash() == _expectedOrderHash(), "order hash mismatch");
    }

    /// 10-slot v7 frame. Signature is ignored under etched EIP-1271.
    /// Slots 6/7 follow the order IO: sell is USDC→vault, buy is vault→USDC.
    function _signedContext(Float underlyingPrice) internal pure returns (SignedContextV1[] memory signedContext) {
        bytes32[] memory context = new bytes32[](10);
        context[0] = bytes32(uint256(7));
        context[1] = Float.unwrap(underlyingPrice);
        context[2] = bytes32(ORACLE_PUBLISH_TIME);
        context[3] = SESSION_RTH;
        context[4] = bytes32(SESSION_START);
        context[5] = bytes32(SESSION_END);
        if (_isBuy()) {
            context[6] = bytes32(uint256(uint160(_wtVault())));
            context[7] = bytes32(uint256(uint160(_usdc())));
        } else {
            context[6] = bytes32(uint256(uint160(_usdc())));
            context[7] = bytes32(uint256(uint160(_wtVault())));
        }
        context[8] = bytes32(FRAME_EXPIRY);
        context[9] = bytes32(_expectedChainId());

        signedContext = new SignedContextV1[](1);
        signedContext[0] = SignedContextV1({signer: ORACLE_SIGNER, context: context, signature: hex"00"});
    }

    function _quote(OrderV4 memory order, SignedContextV1[] memory signedContext)
        internal
        view
        returns (bool success, Float maxOutput, Float ioRatio)
    {
        return IRaindexV6(_raindex())
            .quote2(QuoteV2({order: order, inputIOIndex: 0, outputIOIndex: 0, signedContext: signedContext}));
    }

    function _requoteRatio(OrderV4 memory order, SignedContextV1[] memory signedContext)
        internal
        view
        returns (bool success, Float ioRatio)
    {
        (success,, ioRatio) = _quote(order, signedContext);
    }

    /// Deposit + take under `vm.expectRevert`. Quote2 may return
    /// `success=false` instead of bubbling ensure strings; take surfaces them.
    function _expectEnsureRevert(OrderV4 memory order, SignedContextV1[] memory signedContext, bytes memory message)
        internal
    {
        _depositOrderOutput(order);
        _fundTakerAndApprove();
        vm.expectRevert(message);
        IRaindexV6(_raindex()).takeOrders4(_takeConfig(order, signedContext));
    }

    function _erc4626ConvertToAssetsFloat(address vault, uint256 shares) internal view returns (Float) {
        return LibDecimalFloat.fromFixedDecimalLosslessPacked(IERC4626(vault).convertToAssets(shares), 18);
    }

    function _erc4626ConvertToSharesFloat(address vault, uint256 assets) internal view returns (Float) {
        return LibDecimalFloat.fromFixedDecimalLosslessPacked(IERC4626(vault).convertToShares(assets), 18);
    }

    function _logFloat(string memory label, Float f) internal pure {
        (uint256 fixed18,) = f.toFixedDecimalLossy(18);
        console2.log(label, fixed18);
    }

    function _takeConfig(OrderV4 memory order, SignedContextV1[] memory signedContext)
        internal
        pure
        returns (TakeOrdersConfigV5 memory)
    {
        TakeOrderConfigV4[] memory orders = new TakeOrderConfigV4[](1);
        orders[0] = TakeOrderConfigV4({order: order, inputIOIndex: 0, outputIOIndex: 0, signedContext: signedContext});
        return TakeOrdersConfigV5({
            minimumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIO: LibDecimalFloat.fromFixedDecimalLosslessPacked(1e12, 18),
            maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
            IOIsInput: true,
            orders: orders,
            data: ""
        });
    }

    function _depositOrderOutput(OrderV4 memory order) internal {
        if (_isBuy()) {
            _depositOutputUsdc(order, DEPOSIT_USDC);
        } else {
            _depositOutputShares(order, DEPOSIT_SHARES);
        }
    }

    function _depositOutputShares(OrderV4 memory order, uint256 shareAmount) internal {
        address owner = order.owner;
        address vault = _wtVault();
        bytes32 vaultId = order.validOutputs[0].vaultId;
        deal(vault, owner, shareAmount);
        vm.startPrank(owner);
        IERC20(vault).approve(_raindex(), shareAmount);
        IRaindexV6(_raindex())
            .deposit4(vault, vaultId, LibDecimalFloat.fromFixedDecimalLosslessPacked(shareAmount, 18), new TaskV2[](0));
        vm.stopPrank();
    }

    function _depositOutputUsdc(OrderV4 memory order, uint256 usdcAmount) internal {
        address owner = order.owner;
        address usdc = _usdc();
        bytes32 vaultId = order.validOutputs[0].vaultId;
        deal(usdc, owner, usdcAmount);
        vm.startPrank(owner);
        IERC20(usdc).approve(_raindex(), usdcAmount);
        IRaindexV6(_raindex())
            .deposit4(
                usdc,
                vaultId,
                LibDecimalFloat.fromFixedDecimalLosslessPacked(usdcAmount, _usdcDecimals()),
                new TaskV2[](0)
            );
        vm.stopPrank();
    }

    function _fundTakerAndApprove() internal {
        if (_isBuy()) {
            // Taker pays vault shares (order input) and receives USDC.
            deal(_wtVault(), address(this), DEPOSIT_SHARES);
            IERC20(_wtVault()).approve(_raindex(), type(uint256).max);
        } else {
            _fundUsdcAndApprove(1_000_000 * (10 ** uint256(_usdcDecimals())));
        }
    }

    function _fundUsdcAndApprove(uint256 amount) internal {
        deal(_usdc(), address(this), amount);
        IERC20(_usdc()).approve(_raindex(), type(uint256).max);
    }

    /// Dividend-style NAV step: push underlying into the vault without minting
    /// shares. Size is a small fraction of current assets so the derived price
    /// stays inside the live order's max-price band (a huge donate on a thin
    /// vault blows convertToAssets past the bound and falsely fails the test).
    function _donateUnderlyingToVault() internal {
        address vault = _wtVault();
        address asset = IERC4626(vault).asset();
        uint256 assetsBefore = IERC20(asset).balanceOf(vault);
        uint256 amount = assetsBefore / 1_000; // ~0.1%
        if (amount == 0) amount = 1;
        deal(asset, address(this), amount);
        IERC20(asset).transfer(vault, amount);
    }
}
