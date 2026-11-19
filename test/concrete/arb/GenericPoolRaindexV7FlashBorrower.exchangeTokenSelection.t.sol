// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

import {GenericPoolRaindexV7FlashBorrower} from "../../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";
import {
    IRaindexV7,
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    OrderV4,
    IOV2,
    EvaluableV4,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.37/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";
import {RealisticFlashLendingMockRaindex} from "test/util/concrete/RealisticFlashLendingMockRaindex.sol";

/// Deterministic coverage of how the GenericPool `_exchange` override selects
/// the borrowed token: it reads `takeOrders.orders[0]` (the FIRST order) and
/// `validOutputs[outputIOIndex]` (the configured output IO). Both are pinned
/// here without relying on the fuzzer, by making any other selection approve a
/// decoy token that the exchange cannot pull (so the swap reverts).
contract GenericPoolRaindexV7FlashBorrowerExchangeTokenSelectionTest is Test {
    MockToken internal inputToken;
    MockToken internal outputToken;
    MockToken internal decoyToken;
    address internal raindex;
    MockExchange internal exchange;
    GenericPoolRaindexV7FlashBorrower internal arb;

    function setUp() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        inputToken = new MockToken("Input", "IN", 18);
        outputToken = new MockToken("Output", "OUT", 18);
        // A token the exchange holds none of and that is never the order's real
        // borrowed token; if `_exchange` approves THIS, the exchange's pull of
        // the real output token reverts on zero allowance.
        decoyToken = new MockToken("Decoy", "DEC", 18);

        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(new RealisticFlashLendingMockRaindex()).code);
        raindex = LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS;
        exchange = new MockExchange();

        outputToken.mint(raindex, 1000e18);
        inputToken.mint(address(exchange), 100e18);

        arb = new GenericPoolRaindexV7FlashBorrower();
    }

    function exchangeData() internal view returns (bytes memory) {
        return abi.encode(
            address(exchange),
            address(exchange),
            abi.encodeCall(MockExchange.swap, (IERC20(address(outputToken)), IERC20(address(inputToken)), 100e18))
        );
    }

    function noopTask() internal pure returns (TaskV3 memory) {
        return TaskV3({
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            signedContext: new SignedContextV2[](0)
        });
    }

    function takeOrdersConfig(TakeOrderConfigV5[] memory orders) internal pure returns (TakeOrdersConfigV6 memory) {
        return TakeOrdersConfigV6({
            minimumIO: LibDecimalFloat.packLossless(100, 0),
            maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
            maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
            IOIsInput: true,
            orders: orders,
            data: ""
        });
    }

    /// `_exchange` reads the borrowed token from `orders[0]` (the FIRST order).
    /// A two-order config whose SECOND order has a decoy validOutputs[0] still
    /// completes using the first order's output token. If `_exchange` read
    /// `orders[1]` it would approve the decoy and the exchange's pull of the
    /// real output token would revert.
    function testExchangeSelectsFirstOrderBorrowedToken() external {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](2);
        orders[0] = singleIoOrder(address(inputToken), address(outputToken));
        orders[1] = singleIoOrder(address(inputToken), address(decoyToken));

        arb.arb5(IRaindexV7(raindex), takeOrdersConfig(orders), exchangeData(), noopTask());

        // The real output token was the one swapped through the exchange.
        assertEq(outputToken.balanceOf(address(exchange)), 100e18, "exchange pulled first order output");
        // The decoy (second order output) was never approved/pulled.
        assertEq(decoyToken.allowance(address(arb), address(exchange)), 0, "decoy never approved");
    }

    /// `_exchange` reads the borrowed token from `validOutputs[outputIOIndex]`.
    /// A single order with two validOutputs (index 0 = decoy, index 1 = real)
    /// and outputIOIndex == 1 completes using validOutputs[1]. If `_exchange`
    /// read validOutputs[0] (or validOutputs[inputIOIndex]) it would approve the
    /// decoy and the exchange's pull of the real output token would revert.
    function testExchangeUsesOutputIoIndexForBorrowedToken() external {
        IOV2[] memory validInputs = new IOV2[](1);
        validInputs[0] = IOV2(address(inputToken), bytes32(0));
        IOV2[] memory validOutputs = new IOV2[](2);
        validOutputs[0] = IOV2(address(decoyToken), bytes32(0));
        validOutputs[1] = IOV2(address(outputToken), bytes32(0));

        OrderV4 memory order = OrderV4({
            owner: address(0x1234),
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            validInputs: validInputs,
            validOutputs: validOutputs,
            nonce: bytes32(0)
        });

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        // inputIOIndex 0, outputIOIndex 1.
        orders[0] = TakeOrderConfigV5(order, 0, 1, new SignedContextV2[](0));

        arb.arb5(IRaindexV7(raindex), takeOrdersConfig(orders), exchangeData(), noopTask());

        // validOutputs[1] (the real output) was swapped through the exchange.
        assertEq(outputToken.balanceOf(address(exchange)), 100e18, "exchange pulled outputIOIndex token");
        // validOutputs[0] (the decoy) was never approved/pulled.
        assertEq(decoyToken.allowance(address(arb), address(exchange)), 0, "decoy at index 0 never approved");
    }

    function singleIoOrder(address inTok, address outTok) internal pure returns (TakeOrderConfigV5 memory) {
        IOV2[] memory validInputs = new IOV2[](1);
        validInputs[0] = IOV2(inTok, bytes32(0));
        IOV2[] memory validOutputs = new IOV2[](1);
        validOutputs[0] = IOV2(outTok, bytes32(0));
        OrderV4 memory order = OrderV4({
            owner: address(0x1234),
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            validInputs: validInputs,
            validOutputs: validOutputs,
            nonce: bytes32(0)
        });
        return TakeOrderConfigV5(order, 0, 0, new SignedContextV2[](0));
    }
}
