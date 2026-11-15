// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Vm} from "forge-std-1.17.0/src/Test.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    OrderV4,
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    SignedContextV2
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";

/// @title RaindexV7TakeOrderNoopTest
/// @notice A test harness for testing the Raindex takeOrder function. Focuses
/// on the no-op case.
contract RaindexV7TakeOrderNoopTest is RaindexV7ExternalRealTest {
    using LibOrder for OrderV4;
    using LibDecimalFloat for Float;

    /// Take orders makes no sense without any orders in the input array and the
    /// caller has full control over this so we error.
    function testTakeOrderNoopZeroOrders() external {
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(new TakeOrderConfigV5[](0));
        vm.expectRevert(NoOrders.selector);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);
        (totalTakerInput, totalTakerOutput);
    }

    /// If there is some order in the input array but it is not live we don't
    /// error as the caller may not have control over this, e.g. the order may
    /// have been removed by its owner. We don't want to revert the whole
    /// transaction in this case as there may be other orders in the input array
    /// in the general case.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderNoopNonLiveOrderOne(
        OrderV4 memory order,
        uint256 inputIOIndex,
        uint256 outputIOIndex,
        SignedContextV2 memory signedContext
    ) external {
        vm.assume(order.validInputs.length > 0);
        inputIOIndex = bound(inputIOIndex, 0, order.validInputs.length - 1);
        vm.assume(order.validOutputs.length > 0);
        outputIOIndex = bound(outputIOIndex, 0, order.validOutputs.length - 1);

        order.validInputs[inputIOIndex].token = address(iToken0);
        order.validOutputs[outputIOIndex].token = address(iToken1);

        // We don't bound the input or output indexes as we want to allow
        // malformed orders to be passed in, and still show that nothing happens.
        SignedContextV2[] memory signedContexts = new SignedContextV2[](1);
        signedContexts[0] = signedContext;
        TakeOrderConfigV5 memory orderConfig = TakeOrderConfigV5(order, inputIOIndex, outputIOIndex, signedContexts);
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        orders[0] = orderConfig;
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(orders);
        vm.expectEmit(address(iRaindex));
        emit OrderNotFound(address(this), order.owner, order.hash());
        vm.recordLogs();
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);
        assertTrue(totalTakerInput.isZero());
        assertTrue(totalTakerOutput.isZero());
        Vm.Log[] memory logs = vm.getRecordedLogs();
        assertEq(logs.length, 1);
    }

    /// Same as above but with two orders.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderNoopNonLiveOrderTwo(
        OrderV4 memory order1,
        OrderV4 memory order2,
        uint256 inputIOIndex1,
        uint256 outputIOIndex1,
        uint256 inputIOIndex2,
        uint256 outputIOIndex2,
        SignedContextV2 memory signedContext1,
        SignedContextV2 memory signedContext2
    ) external {
        vm.assume(order1.validInputs.length > 0);
        inputIOIndex1 = bound(inputIOIndex1, 0, order1.validInputs.length - 1);
        vm.assume(order1.validOutputs.length > 0);
        outputIOIndex1 = bound(outputIOIndex1, 0, order1.validOutputs.length - 1);
        vm.assume(order2.validInputs.length > 0);
        inputIOIndex2 = bound(inputIOIndex2, 0, order2.validInputs.length - 1);
        vm.assume(order2.validOutputs.length > 0);
        outputIOIndex2 = bound(outputIOIndex2, 0, order2.validOutputs.length - 1);

        vm.assume(order1.validInputs[inputIOIndex1].token != order1.validOutputs[outputIOIndex1].token);
        vm.assume(order2.validInputs[inputIOIndex2].token != order2.validOutputs[outputIOIndex2].token);

        // The inputs and outputs need to match or we will trigger the token
        // mismatch error.
        order2.validInputs[inputIOIndex2].token = address(iToken0);
        order2.validOutputs[outputIOIndex2].token = address(iToken1);

        order1.validInputs[inputIOIndex1].token = order2.validInputs[inputIOIndex2].token;
        order1.validOutputs[outputIOIndex1].token = order2.validOutputs[outputIOIndex2].token;

        TakeOrdersConfigV6 memory config;
        {
            TakeOrderConfigV5[] memory orders;
            {
                // We don't bound the input or output indexes as we want to allow
                // malformed orders to be passed in, and still show that nothing happens.
                SignedContextV2[] memory signedContexts1 = new SignedContextV2[](1);
                signedContexts1[0] = signedContext1;
                TakeOrderConfigV5 memory orderConfig1 =
                    TakeOrderConfigV5(order1, inputIOIndex1, outputIOIndex1, signedContexts1);
                SignedContextV2[] memory signedContexts2 = new SignedContextV2[](1);
                signedContexts2[0] = signedContext2;
                TakeOrderConfigV5 memory orderConfig2 =
                    TakeOrderConfigV5(order2, inputIOIndex2, outputIOIndex2, signedContexts2);
                orders = new TakeOrderConfigV5[](2);
                orders[0] = orderConfig1;
                orders[1] = orderConfig2;
            }

            config = LibTestTakeOrder.defaultTakeConfig(orders);
        }

        vm.recordLogs();
        {
            (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);
            assertTrue(totalTakerInput.isZero());
            assertTrue(totalTakerOutput.isZero());
        }
        Vm.Log[] memory logs = vm.getRecordedLogs();
        assertEq(logs.length, 2);

        {
            assertEq(logs[0].topics.length, 1);
            assertEq(logs[0].topics[0], bytes32(uint256(keccak256("OrderNotFound(address,address,bytes32)"))));
            (address sender1, address owner1, bytes32 orderHash1) =
                abi.decode(logs[0].data, (address, address, bytes32));
            assertEq(sender1, address(this));
            assertEq(owner1, order1.owner);
            assertEq(orderHash1, order1.hash());
        }

        {
            assertEq(logs[1].topics.length, 1);
            assertEq(logs[1].topics[0], bytes32(uint256(keccak256("OrderNotFound(address,address,bytes32)"))));
            (address sender2, address owner2, bytes32 orderHash2) =
                abi.decode(logs[1].data, (address, address, bytes32));
            assertEq(sender2, address(this));
            assertEq(owner2, order2.owner);
            assertEq(orderHash2, order2.hash());
        }
    }
}
