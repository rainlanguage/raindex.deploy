// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalMockTest} from "test/util/abstract/RaindexV7ExternalMockTest.sol";
import {OrderConfigV4, OrderV4, EvaluableV4, TaskV3} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";
import {NotOrderOwner} from "../../../src/error/ErrRaindexV7.sol";

/// @title RaindexV7RemoveOrderMockTest
/// @notice A contract to test the Raindex removeOrder function.
contract RaindexV7RemoveOrderMockTest is RaindexV7ExternalMockTest {
    /// An order MUST ONLY be removable by its owner.
    /// forge-config: default.fuzz.runs = 100
    function testRemoveOrderOnlyOwner(address alice, address bob, OrderConfigV4 memory config, bytes memory expression)
        external
    {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        vm.assume(alice != bob);

        (OrderV4 memory expectedOrder, bytes32 expectedOrderHash) = LibTestAddOrder.expectedOrder(alice, config);

        // It will revert even if the order has not been added yet.
        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, alice));
        vm.prank(bob);
        iRaindex.removeOrder4(expectedOrder, new TaskV3[](0));

        // And will revert after the order is added.
        (OrderV4 memory order, bytes32 orderHash) = addOrderWithChecks(alice, config, expression);
        assertEq(orderHash, expectedOrderHash);

        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, alice));
        vm.prank(bob);
        iRaindex.removeOrder4(order, new TaskV3[](0));

        // Alice can remove the order.
        removeOrderWithChecks(alice, order);

        // It will revert even after the order has been removed.
        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, alice));
        vm.prank(bob);
        iRaindex.removeOrder4(order, new TaskV3[](0));
    }

    /// The same order can be added and removed multiple times.
    /// forge-config: default.fuzz.runs = 100
    function testRemoveOrderAddRemoveMulti(address alice, OrderConfigV4 memory config, bytes memory expression)
        external
    {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);

        OrderV4 memory order;
        bytes32 orderHashA;
        bytes32 orderHashB;
        // Each iteration is quite slow so 3 is about as much as we want to do.
        for (uint256 i = 0; i < 3; i++) {
            (order, orderHashB) = addOrderWithChecks(alice, config, expression);
            removeOrderWithChecks(alice, order);
            if (i > 0) {
                assertEq(orderHashA, orderHashB);
            }
            orderHashA = orderHashB;
        }
    }

    /// An order MUST NOT change state if it does not exist.
    /// forge-config: default.fuzz.runs = 100
    function testRemoveOrderDoesNotExist(address alice, OrderConfigV4 memory config, bytes memory) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        (OrderV4 memory order, bytes32 orderHash) = LibTestAddOrder.expectedOrder(alice, config);
        assertFalse(iRaindex.orderExists(orderHash));
        vm.record();
        vm.recordLogs();
        vm.prank(alice);
        assertFalse(iRaindex.removeOrder4(order, new TaskV3[](0)));
        assertEq(vm.getRecordedLogs().length, 0);
        (bytes32[] memory reads, bytes32[] memory writes) = vm.accesses(address(iRaindex));
        // 3x for reentrancy guard, 1x for dead order check.
        assertEq(reads.length, 4);
        // 2x for reentrancy guard.
        assertEq(writes.length, 2);
    }

    /// Can add and remove different orders.
    /// forge-config: default.fuzz.runs = 100
    function testRemoveOrderDifferent(
        address alice,
        OrderConfigV4 memory configOne,
        bytes memory expressionOne,
        OrderConfigV4 memory configTwo,
        bytes memory expressionTwo
    ) external {
        LibTestAddOrder.conformConfig(configOne, iInterpreter, iStore);
        LibTestAddOrder.conformConfig(configTwo, iInterpreter, iStore);

        (OrderV4 memory expectedOrderOne, bytes32 expectedOrderHashOne) =
            LibTestAddOrder.expectedOrder(alice, configOne);
        (OrderV4 memory expectedOrderTwo, bytes32 expectedOrderHashTwo) =
            LibTestAddOrder.expectedOrder(alice, configTwo);
        (expectedOrderOne);
        (expectedOrderTwo);
        vm.assume(expectedOrderHashOne != expectedOrderHashTwo);

        (OrderV4 memory orderOne, bytes32 orderHashOne) = addOrderWithChecks(alice, configOne, expressionOne);
        (OrderV4 memory orderTwo, bytes32 orderHashTwo) = addOrderWithChecks(alice, configTwo, expressionTwo);
        assertEq(orderHashOne, expectedOrderHashOne);
        assertEq(orderHashTwo, expectedOrderHashTwo);
        removeOrderWithChecks(alice, orderOne);
        removeOrderWithChecks(alice, orderTwo);
    }

    /// Different owners can add and remove the same order.
    /// forge-config: default.fuzz.runs = 100
    function testRemoveOrderDifferentOwners(
        address alice,
        address bob,
        OrderConfigV4 memory config,
        bytes memory expression
    ) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        vm.assume(alice != bob);
        (OrderV4 memory orderAlice, bytes32 orderHashAlice) = addOrderWithChecks(alice, config, expression);
        (OrderV4 memory orderBob, bytes32 orderHashBob) = addOrderWithChecks(bob, config, expression);
        assertTrue(orderHashAlice != orderHashBob);

        // Owners can't interfere with each other.
        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, bob));
        vm.prank(alice);
        iRaindex.removeOrder4(orderBob, new TaskV3[](0));

        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, alice));
        vm.prank(bob);
        iRaindex.removeOrder4(orderAlice, new TaskV3[](0));

        removeOrderWithChecks(alice, orderAlice);
        removeOrderWithChecks(bob, orderBob);
    }

    /// Different owners can add and remove different orders.
    /// forge-config: default.fuzz.runs = 100
    function testRemoveOrderDifferentOwnersDifferent(
        address alice,
        address bob,
        OrderConfigV4 memory configOne,
        bytes memory expressionOne,
        OrderConfigV4 memory configTwo,
        bytes memory expressionTwo
    ) external {
        {
            LibTestAddOrder.conformConfig(configOne, iInterpreter, iStore);
            LibTestAddOrder.conformConfig(configTwo, iInterpreter, iStore);
            vm.assume(alice != bob);

            // Ensure the configs are different.
            (OrderV4 memory expectedOrderOne, bytes32 expectedOrderHashOne) =
                LibTestAddOrder.expectedOrder(address(0), configOne);
            (OrderV4 memory expectedOrderTwo, bytes32 expectedOrderHashTwo) =
                LibTestAddOrder.expectedOrder(address(0), configTwo);
            (expectedOrderOne);
            (expectedOrderTwo);
            vm.assume(expectedOrderHashOne != expectedOrderHashTwo);
        }

        OrderV4 memory orderAliceOne;
        OrderV4 memory orderBobOne;
        OrderV4 memory orderAliceTwo;
        OrderV4 memory orderBobTwo;
        {
            bytes32 orderHashAliceOne;
            bytes32 orderHashBobOne;
            bytes32 orderHashAliceTwo;
            bytes32 orderHashBobTwo;

            (orderAliceOne, orderHashAliceOne) = addOrderWithChecks(alice, configOne, expressionOne);
            (orderBobOne, orderHashBobOne) = addOrderWithChecks(bob, configOne, expressionOne);
            (orderAliceTwo, orderHashAliceTwo) = addOrderWithChecks(alice, configTwo, expressionTwo);
            (orderBobTwo, orderHashBobTwo) = addOrderWithChecks(bob, configTwo, expressionTwo);
            assertTrue(orderHashAliceOne != orderHashAliceTwo);
            assertTrue(orderHashAliceOne != orderHashBobOne);
            assertTrue(orderHashAliceOne != orderHashBobTwo);
            assertTrue(orderHashAliceTwo != orderHashBobOne);
            assertTrue(orderHashAliceTwo != orderHashBobTwo);
            assertTrue(orderHashBobOne != orderHashBobTwo);
        }

        // Owners can't interfere with each other.
        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, bob));
        vm.prank(alice);
        iRaindex.removeOrder4(orderBobOne, new TaskV3[](0));
        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, bob));
        vm.prank(alice);
        iRaindex.removeOrder4(orderBobTwo, new TaskV3[](0));

        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, alice));
        vm.prank(bob);
        iRaindex.removeOrder4(orderAliceOne, new TaskV3[](0));
        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, alice));
        vm.prank(bob);
        iRaindex.removeOrder4(orderAliceTwo, new TaskV3[](0));

        removeOrderWithChecks(alice, orderAliceOne);
        removeOrderWithChecks(bob, orderBobOne);
        removeOrderWithChecks(alice, orderAliceTwo);
        removeOrderWithChecks(bob, orderBobTwo);
    }
}
