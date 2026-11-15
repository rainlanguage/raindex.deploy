// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

import {GenericPoolRaindexV7FlashBorrower} from "../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";
import {
    IRaindexV7,
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    OrderV4,
    IOV2,
    EvaluableV4,
    SignedContextV2,
    TaskV3,
    Float
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.35/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";
import {LossyConversionFromFloat} from "rain-math-float-0.3.12/src/error/ErrDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";
import {RealisticFlashLendingMockRaindex} from "test/util/concrete/RealisticFlashLendingMockRaindex.sol";

contract RaindexV7FlashBorrowerFlashLoanAmountLosslessTest is Test {
    /// Build a full arb5 setup matching the realistic mock pattern but with a
    /// caller-supplied `minimumIO` (a `Float`) so the test can exercise the
    /// lossless-conversion guard in arb5's `flashLoanAmount` computation.
    function callArb4(Float minimumIO) internal {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        // 6-decimal output (USDT-like) so a sub-1e-6 `minimumIO` cannot be
        // represented exactly at the output token's decimals.
        MockToken inputToken = new MockToken("DAI", "DAI", 18);
        MockToken outputToken = new MockToken("USDT", "USDT", 6);

        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(new RealisticFlashLendingMockRaindex()).code);
        address raindex = LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS;
        MockExchange exchange = new MockExchange();

        outputToken.mint(raindex, 1000e6);
        inputToken.mint(address(exchange), 100e6);

        GenericPoolRaindexV7FlashBorrower arb = new GenericPoolRaindexV7FlashBorrower();

        arb.arb5(
            IRaindexV7(raindex),
            TakeOrdersConfigV6({
                minimumIO: minimumIO,
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: buildSingleOrder(address(inputToken), address(outputToken)),
                data: ""
            }),
            abi.encode(
                address(exchange),
                address(exchange),
                abi.encodeCall(MockExchange.swap, (IERC20(address(outputToken)), IERC20(address(inputToken)), 100e6))
            ),
            TaskV3({
                evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
                signedContext: new SignedContextV2[](0)
            })
        );
    }

    /// Build a single-order takeOrders config with the given input/output token.
    function buildSingleOrder(address inputToken, address outputToken)
        internal
        pure
        returns (TakeOrderConfigV5[] memory orders)
    {
        IOV2[] memory validInputs = new IOV2[](1);
        validInputs[0] = IOV2(inputToken, bytes32(0));
        IOV2[] memory validOutputs = new IOV2[](1);
        validOutputs[0] = IOV2(outputToken, bytes32(0));

        OrderV4 memory order = OrderV4({
            owner: address(0x1234),
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            validInputs: validInputs,
            validOutputs: validOutputs,
            nonce: bytes32(0)
        });

        orders = new TakeOrderConfigV5[](1);
        orders[0] = TakeOrderConfigV5(order, 0, 0, new SignedContextV2[](0));
    }

    /// arb5 computes the flash loan amount with `toFixedDecimalLossless`: a
    /// `minimumIO` that cannot be represented exactly at the output token's
    /// decimals MUST revert `LossyConversionFromFloat` rather than silently
    /// truncating to a smaller loan. minimumIO = 1e-7 at 6 output decimals:
    /// finalExponent = -7 + 6 = -1, so 1 / 10 = 0 with remainder => lossy.
    function testArb4FlashLoanAmountRevertsOnLossyMinimumIO() external {
        Float lossyMinimumIO = LibDecimalFloat.packLossless(1, -7);
        vm.expectRevert(abi.encodeWithSelector(LossyConversionFromFloat.selector, int256(1), int256(-7)));
        this.callArb4Wrapper(lossyMinimumIO);
    }

    /// Control: a `minimumIO` that IS exactly representable at the output
    /// decimals (100 whole tokens at 6 decimals => exactly 100e6 base units)
    /// does NOT trip the lossless guard and the full arb cycle completes. This
    /// pins that the lossy revert above is specific to non-representable input,
    /// not a blanket revert.
    function testArb4FlashLoanAmountAcceptsLosslessMinimumIO() external {
        Float losslessMinimumIO = LibDecimalFloat.packLossless(100, 0);
        // No revert expected; a representable minimumIO proceeds through the
        // full arb cycle (flashLoanAmount == 100e6, matching the exchange swap).
        callArb4(losslessMinimumIO);
    }

    /// External wrapper so `vm.expectRevert` only targets the arb call, not the
    /// per-test environment setup.
    function callArb4Wrapper(Float minimumIO) external {
        callArb4(minimumIO);
    }
}
