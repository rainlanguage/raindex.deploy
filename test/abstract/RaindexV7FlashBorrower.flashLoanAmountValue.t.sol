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
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";
import {RecordingFlashLendingMockRaindex} from "test/util/concrete/RecordingFlashLendingMockRaindex.sol";

/// arb5 borrows exactly `minimumIO` worth of the order's OUTPUT token, expressed
/// in that token's base units. The full arb cycle closes regardless of small
/// perturbations to the borrowed amount (the leftover-output accounting absorbs
/// them), so this pins the exact recorded `flashLoan(token, amount)` arguments
/// rather than relying on the cycle completing.
contract RaindexV7FlashBorrowerFlashLoanAmountValueTest is Test {
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

    function noopTask() internal pure returns (TaskV3 memory) {
        return TaskV3({
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            signedContext: new SignedContextV2[](0)
        });
    }

    /// minimumIO = 100 whole tokens against a 6-decimal output token. arb5 MUST
    /// request a flash loan of exactly 100e6 base units of the output token. An
    /// off-by-one (or any other) perturbation to the computed amount records a
    /// different value here even though the arb cycle still completes.
    function testArb4FlashLoanAmountIsExactlyMinimumIOInOutputBaseUnits() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        // 6-decimal output (USDT-like); 18-decimal input. The borrowed amount is
        // computed at the OUTPUT token's decimals.
        MockToken inputToken = new MockToken("DAI", "DAI", 18);
        MockToken outputToken = new MockToken("USDT", "USDT", 6);

        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(new RecordingFlashLendingMockRaindex()).code);
        RecordingFlashLendingMockRaindex raindex =
            RecordingFlashLendingMockRaindex(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
        MockExchange exchange = new MockExchange();

        // Raindex has plenty to lend; exchange swaps the borrowed output for
        // input one-for-one in base units.
        outputToken.mint(address(raindex), 1000e6);
        inputToken.mint(address(exchange), 1000e6);

        GenericPoolRaindexV7FlashBorrower arb = new GenericPoolRaindexV7FlashBorrower();

        arb.arb5(
            IRaindexV7(address(raindex)),
            TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(100, 0),
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
            noopTask()
        );

        // Exactly 100 tokens at 6 decimals == 100e6 base units of the OUTPUT
        // token were flash loaned.
        assertEq(raindex.lastFlashLoanToken(), address(outputToken), "flash loaned the output token");
        assertEq(raindex.lastFlashLoanAmount(), 100e6, "flash loan amount == minimumIO in output base units");
    }
}
