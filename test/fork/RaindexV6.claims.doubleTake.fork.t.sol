// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test, console2} from "forge-std-1.17.0/src/Test.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

/// @title RaindexV6ClaimsDoubleTakeForkTest
/// @notice Fork of the Base RaindexV6 at `0xb05D…e3cE` the block before
/// `0x9844…6476` took claims order
/// `0xaffd6e3aaad92a44017714b1db48909098f509404c9baeafd29bf4eda874d7c8`.
///
/// The order is the `claims.rain` strategy. `calculate-io` requires the leaf's
/// claimed flag to be unset. `handle-io` is what sets it. This deployment's
/// `takeOrders4` runs every `calculate-io` in the batch before any `handle-io`,
/// so a second copy of the same leaf in that batch still sees the flag unset.
///
/// The signed context is the one from the take in the next block,
/// `0xbdfb412dc546e960b0701c891beb0c88422749c7a5f534bc9666f1f841b0f887`:
/// leaf index 46, the claimed amount, and its depth-8 merkle proof.
contract RaindexV6ClaimsDoubleTakeForkTest is Test {
    uint256 internal constant FORK_BLOCK = 52_675_841;
    address internal constant RAINDEX = 0xb05D73E6BCc26AEB5b67Ff68C6E9C6151073e3cE;
    address internal constant TAKER = 0x98440184426887970B7c3141e3019B6f39406476;
    address internal constant OWNER = 0xA51fd23D6E2442805130eac0712F590691e91517;
    address internal constant INTERPRETER = 0x3bF9bd9DA4784f75c92317e61c68493Ecc9aaBDC;
    address internal constant STORE = 0x1Aa775533E28B1D843e1A589034984E3a62005DC;
    address internal constant SIGNER = 0x48C2A79B9604ADd957884bd51559693bCC9481B7;
    address internal constant WETH = 0x4200000000000000000000000000000000000006;
    address internal constant USDC = 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913;

    bytes32 internal constant ORDER_HASH = 0xaffd6e3aaad92a44017714b1db48909098f509404c9baeafd29bf4eda874d7c8;
    bytes32 internal constant NONCE = 0x7cd8721efc5b91488edf682ddf54d318cd783c50f3309f62e4fbf85c203fa5cb;
    bytes32 internal constant VAULT_ID = 0x75340ff2e67adee60702dc91d4fe0b8d5c23b8c444681acd2e7887e528edadb1;
    /// 5.970208333333340160, the leaf amount. `takeOrders4` pushes the lossy
    /// 6-decimal truncation, 5.970208 USDC.
    bytes32 internal constant CLAIM = 0xffffffee000000000000000000000000000000000000000052da70d8ff692000;
    /// Copies of this leaf submitted in one `takeOrders4`.
    uint256 internal constant CLAIM_COPIES = 3;
    /// Same exponent as `CLAIM`, coefficient scaled by `CLAIM_COPIES`, so every copy fits under `maximumIO`.
    bytes32 internal constant CLAIMS = bytes32(
        (uint256(CLAIM) & ~((uint256(1) << 224) - 1)) | ((uint256(CLAIM) & ((uint256(1) << 224) - 1)) * CLAIM_COPIES)
    );
    uint256 internal constant CLAIM_USDC = 5_970_208;
    /// USDC paid for `CLAIM_COPIES` leaves. The leaf exponent is -18 and USDC has 6 decimals, so this divides the
    /// scaled coefficient by 10^12. `takeOrders4` sums the floats and truncates once. `CLAIM_COPIES * CLAIM_USDC` is
    /// the sum of per-leaf truncations, which is 1 raw unit lower at 3 copies.
    uint256 internal constant CLAIM_COPIES_USDC =
        ((uint256(CLAIM) & ((uint256(1) << 224) - 1)) * CLAIM_COPIES) / 1e12;

    bytes internal constant BYTECODE =
        hex"000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000457870656374656420636c61696dee000000000000000000000000000000000000000000000000636c61696d6564e70000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000436c61696d6564e713595e07b47595143f3c8574b2a03918d8faac9de11cc20a4eb15187673efb4e000000000000000000000000000000000000000000000000000050726f6f66e500000000000000000000000022d83c031bc02737ad3eaf5aa3386de2bf7f7fbc00000000000000000000000000000000000000000000000000000000000000df0500000024005c0070009808040004031001060b100002001000010b010003001000010b01000400100000011000000d060002031001060b1000020110000100100000031004041f1200001e020000011000030110000200100001031000010c13000047020000040300010310010603100201031000060c1300000906010101100004011000000110000200100000031000010c130000461100001f1200001e0200000e0b010203100906031008060310070603100606031005060310040603100306031002060010000001100005021a000701100006001000011e020000";

    IRaindexV6Fork internal constant RAINDEX_FORK = IRaindexV6Fork(RAINDEX);

    function setUp() external {
        vm.createSelectFork("base", FORK_BLOCK);
    }

    /// The order bytes hash to the order the taker took.
    function testFork_orderHash() external pure {
        assertEq(keccak256(abi.encode(_order())), ORDER_HASH, "order hash");
    }

    /// One copy, with `maximumIO` equal to the leaf, pays 5.970208 USDC and
    /// nothing in return. That is the transfer in the next block.
    function testFork_historicalClaimPaysOnce() external {
        uint256 usdcBefore = IERC20(USDC).balanceOf(TAKER);
        uint256 wethBefore = IERC20(WETH).balanceOf(TAKER);

        vm.prank(TAKER);
        (bytes32 takerInput, bytes32 takerOutput) = RAINDEX_FORK.takeOrders4(_config(1, CLAIM));

        uint256 usdcAfter = IERC20(USDC).balanceOf(TAKER);
        console2.log("historical claim USDC received", usdcAfter - usdcBefore);
        assertEq(takerInput, CLAIM, "taker receives the leaf");
        assertEq(takerOutput, bytes32(0), "ratio 0 pays nothing");
        assertEq(usdcAfter - usdcBefore, CLAIM_USDC, "USDC received");
        assertEq(IERC20(WETH).balanceOf(TAKER), wethBefore, "WETH untouched");
    }

    /// The claimed flag is visible to a later transaction. A second take of the
    /// same leaf reverts in `calculate-io`.
    function testFork_laterTransactionSeesClaimedFlag() external {
        uint256 before = IERC20(USDC).balanceOf(TAKER);

        vm.prank(TAKER);
        RAINDEX_FORK.takeOrders4(_config(1, CLAIM));
        uint256 afterFirst = IERC20(USDC).balanceOf(TAKER);
        console2.log("later transaction USDC received after first take", afterFirst - before);

        vm.expectRevert();
        vm.prank(TAKER);
        RAINDEX_FORK.takeOrders4(_config(1, CLAIM));

        uint256 afterSecond = IERC20(USDC).balanceOf(TAKER);
        console2.log("later transaction USDC received after reverted take", afterSecond - before);
        assertEq(afterSecond, afterFirst, "second transaction pays nothing");
    }

    /// `CLAIM_COPIES` copies in one `takeOrders4` all calculate before any
    /// `handle-io` sets the flag, so the leaf pays once per copy. `maximumIO`
    /// is that many leaves; the historical take set it to one leaf, which
    /// stops the loop after the first.
    function testFork_sameLeafPaysTwiceInOneBatch() external {
        uint256 before = IERC20(USDC).balanceOf(TAKER);

        vm.prank(TAKER);
        (bytes32 takerInput, bytes32 takerOutput) = RAINDEX_FORK.takeOrders4(_config(CLAIM_COPIES, CLAIMS));

        uint256 usdcAfter = IERC20(USDC).balanceOf(TAKER);
        console2.log("double take USDC received", usdcAfter - before);
        assertEq(takerOutput, bytes32(0), "ratio 0 pays nothing");
        assertEq(usdcAfter - before, CLAIM_COPIES_USDC, "leaf paid once per copy");
        // The returned input float is the sum of the fills. Its exact
        // packing is the contract's, so the USDC balance is the assertion.
        assertTrue(takerInput != bytes32(0), "batch took a non-zero input");
    }

    function _config(uint256 copies, bytes32 maximumIO) internal pure returns (TakeOrdersConfigV5 memory config) {
        TakeOrderConfigV4[] memory orders = new TakeOrderConfigV4[](copies);
        for (uint256 i = 0; i < copies; i++) {
            orders[i] = TakeOrderConfigV4({
                order: _order(), signedContext: _signedContext(), inputIOIndex: 0, outputIOIndex: 0
            });
        }
        config = TakeOrdersConfigV5({
            minimumIO: bytes32(0),
            maximumIO: maximumIO,
            // Coefficient 1, exponent 0. The order's ratio is 0, which is not above this.
            maximumIORatio: bytes32(uint256(1)),
            IOIsInput: true,
            orders: orders,
            data: ""
        });
    }

    function _order() internal pure returns (OrderV4 memory) {
        IOV2[] memory inputs = new IOV2[](1);
        inputs[0] = IOV2({token: WETH, vaultId: VAULT_ID});
        IOV2[] memory outputs = new IOV2[](1);
        outputs[0] = IOV2({token: USDC, vaultId: VAULT_ID});
        return OrderV4({
            owner: OWNER,
            evaluable: EvaluableV4({interpreter: INTERPRETER, store: STORE, bytecode: BYTECODE}),
            validInputs: inputs,
            validOutputs: outputs,
            nonce: NONCE
        });
    }

    function _signedContext() internal pure returns (SignedContextV1[] memory signed) {
        bytes32[] memory context = new bytes32[](10);
        context[0] = bytes32(uint256(0x2e));
        context[1] = CLAIM;
        context[2] = 0xa72857e104d6edf45c955edd7a7976646df6d5212fb61fa583c6392ed85621a5;
        context[3] = 0x06ce56df476223b87d18d2a8fcfd60519edc8760f81d816d63150d4a5d192e41;
        context[4] = 0xbbbb063f8db2d8551361f810c35646ca9fb841b40c532f48103aec752db7a87a;
        context[5] = 0x777eb73adecec219741d2ef199811e18b01c5a0eaee9375e74fdd47fd13130df;
        context[6] = 0x19f43218f6ba810cb612b4f6dc9792101cedfb52e3c6d025cca5239e160a59ee;
        context[7] = 0x18bafbf03371d5d2dec1aab04d2e6d948cf3531bf70f33dbdadfca1ab33054d2;
        context[8] = 0x66058ac12680ec286b66d779cb5869ab831392446b5abda03057d74c958ffaac;
        context[9] = 0x4beaed292b7b3037c6111adba908f32980b69e4726e8085c129d027b3a547370;
        signed = new SignedContextV1[](1);
        signed[0] = SignedContextV1({
            signer: SIGNER,
            context: context,
            signature: hex"971d18d4b254805659250722e1e3b7f2b795c2fc27ee1146186fd39e2b98e5923145f5bb49bccaf9baa2843a175cdbd5ac4882b9446d7eb610d5ce8db799c02b1c"
        });
    }
}

interface IRaindexV6Fork {
    function takeOrders4(TakeOrdersConfigV5 calldata config)
        external
        returns (bytes32 totalTakerInput, bytes32 totalTakerOutput);
}

struct IOV2 {
    address token;
    bytes32 vaultId;
}

struct EvaluableV4 {
    address interpreter;
    address store;
    bytes bytecode;
}

struct OrderV4 {
    address owner;
    EvaluableV4 evaluable;
    IOV2[] validInputs;
    IOV2[] validOutputs;
    bytes32 nonce;
}

struct SignedContextV1 {
    address signer;
    bytes32[] context;
    bytes signature;
}

struct TakeOrderConfigV4 {
    OrderV4 order;
    uint256 inputIOIndex;
    uint256 outputIOIndex;
    SignedContextV1[] signedContext;
}

struct TakeOrdersConfigV5 {
    bytes32 minimumIO;
    bytes32 maximumIO;
    bytes32 maximumIORatio;
    bool IOIsInput;
    TakeOrderConfigV4[] orders;
    bytes data;
}
