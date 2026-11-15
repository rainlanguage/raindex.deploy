// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalMockTest} from "test/util/abstract/RaindexV7ExternalMockTest.sol";
import {OrderConfigV4, OrderV4, IOV2, EvaluableV4, TaskV3} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";
import {NotRainMetaV1, META_MAGIC_NUMBER_V1} from "rain-metadata-0.1.46/src/interface/unstable/IMetaV1_2.sol";
import {LibMeta} from "rain-metadata-0.1.46/src/lib/LibMeta.sol";
import {IExpressionDeployerV3} from "rainlang-0.2.35/src/interface/deprecated/v1/IExpressionDeployerV3.sol";

/// @title RaindexV7AddOrderMockTest
/// @notice Tests the addOrder function of the RaindexV7 contract.
contract RaindexV7AddOrderMockTest is RaindexV7ExternalMockTest {
    /// Adding an order without calculations is unevaluable so it reverts.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderWithoutCalculationsReverts(address owner, OrderConfigV4 memory config) public {
        vm.prank(owner);
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.evaluable.bytecode = "";
        vm.expectRevert(abi.encodeWithSelector(OrderNoSources.selector));
        iRaindex.addOrder5(config, new TaskV3[](0));
        (OrderV4 memory order, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        (order);
        assertTrue(!iRaindex.orderExists(orderHash));
    }

    /// Adding an order without inputs reverts.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderWithoutInputsReverts(address owner, OrderConfigV4 memory config) public {
        vm.prank(owner);
        config.evaluable.bytecode =
            hex"0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000d02000000040000000000000000";
        config.validInputs = new IOV2[](0);
        vm.expectRevert(abi.encodeWithSelector(OrderNoInputs.selector));
        iRaindex.addOrder5(config, new TaskV3[](0));
        (OrderV4 memory order, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        (order);
        assertTrue(!iRaindex.orderExists(orderHash));
    }

    /// Adding an order without token outputs reverts.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderWithoutOutputsReverts(address owner, OrderConfigV4 memory config) public {
        vm.prank(owner);
        config.evaluable.bytecode =
            hex"0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000d02000000040000000000000000";
        vm.assume(config.validInputs.length > 0);
        config.validOutputs = new IOV2[](0);
        vm.expectRevert(abi.encodeWithSelector(OrderNoOutputs.selector));
        iRaindex.addOrder5(config, new TaskV3[](0));
        (OrderV4 memory order, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        (order);
        assertTrue(!iRaindex.orderExists(orderHash));
    }

    /// Adding an order with calculations, inputs and outputs will succeed if
    /// the expression is valid according to the deployer. The resulting order
    /// MUST be emitted. This test assumes empty meta.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderWithCalculationsInputsAndOutputsSucceeds(
        address owner,
        OrderConfigV4 memory config,
        bytes memory expression
    ) public {
        config.evaluable.bytecode =
            hex"0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000d02000000040000000000000000";
        vm.assume(config.validInputs.length > 0);
        vm.assume(config.validOutputs.length > 0);
        config.meta = new bytes(0);
        (OrderV4 memory order, bytes32 orderhash) = addOrderWithChecks(owner, config, expression);
        (order);
        (orderhash);
    }

    /// Adding a valid order with a non-empty meta MUST revert if the meta is
    /// not self describing as a rain meta document.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderWithNonEmptyMetaReverts(address owner, OrderConfigV4 memory config, bytes memory) public {
        vm.prank(owner);
        config.evaluable.bytecode =
            hex"0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000d02000000040000000000000000";
        vm.assume(config.validInputs.length > 0);
        vm.assume(config.validOutputs.length > 0);
        vm.assume(!LibMeta.isRainMetaV1(config.meta));
        vm.assume(config.meta.length > 0);

        vm.expectRevert(abi.encodeWithSelector(NotRainMetaV1.selector, config.meta));
        iRaindex.addOrder5(config, new TaskV3[](0));

        (OrderV4 memory order, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        (order);
        assertTrue(!iRaindex.orderExists(orderHash));
    }

    /// Adding a valid order with a non-empty meta MUST emit MetaV1 if the meta
    /// is self describing as a rain meta document.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderWithNonEmptyMetaEmitsMetaV1(
        address owner,
        OrderConfigV4 memory config,
        bytes memory expression
    ) public {
        config.evaluable.bytecode =
            hex"0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000d02000000040000000000000000";
        vm.assume(config.validInputs.length > 0);
        vm.assume(config.validOutputs.length > 0);
        vm.assume(config.meta.length > 0);

        // This is a bit of a hack, but it's the easiest way to get a valid
        // meta document.
        config.meta = abi.encodePacked(META_MAGIC_NUMBER_V1, config.meta);

        (OrderV4 memory order, bytes32 orderHash) = addOrderWithChecks(owner, config, expression);
        (order);
        (orderHash);
    }

    /// The `MetaV1_2` event emitted by a successful add MUST carry the EXACT
    /// data: sender = order owner, subject = order hash, meta = the config meta.
    /// `addOrderWithChecks` only checks that some `MetaV1_2` is emitted (its
    /// `expectEmit(false,false,true,false)` does not pin the non-indexed
    /// subject/meta data), so this asserts all three fields are correct.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderMetaV1EventExactData(address owner, OrderConfigV4 memory config) public {
        config.evaluable.bytecode =
            hex"0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000d02000000040000000000000000";
        vm.assume(config.validInputs.length > 0);
        vm.assume(config.validOutputs.length > 0);
        vm.assume(config.meta.length > 0);

        // Make the meta self-describe as a rain meta document so it passes the
        // `checkMetaUnhashedV1` guard.
        config.meta = abi.encodePacked(META_MAGIC_NUMBER_V1, config.meta);

        (OrderV4 memory order, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        (order);

        // Full-data check on every field of the event, pinned to the raindex
        // emitter.
        vm.expectEmit(true, true, true, true, address(iRaindex));
        emit MetaV1_2(owner, orderHash, config.meta);

        vm.prank(owner);
        assertTrue(iRaindex.addOrder5(config, new TaskV3[](0)));

        assertTrue(iRaindex.orderExists(orderHash));
    }

    /// Alice and Bob can add orders with the same config. The resulting orders
    /// MUST be different.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderTwoAccountsWithSameConfig(
        address alice,
        address bob,
        OrderConfigV4 memory config,
        bytes memory expression
    ) public {
        vm.assume(alice != bob);
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        (OrderV4 memory aliceOrder, bytes32 aliceOrderHash) = addOrderWithChecks(alice, config, expression);
        (OrderV4 memory bobOrder, bytes32 bobOrderHash) = addOrderWithChecks(bob, config, expression);
        (aliceOrder);
        (bobOrder);
        assertTrue(aliceOrderHash != bobOrderHash);
    }

    /// Alice and Bob can add orders with different configs. The resulting orders
    /// MUST be different.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderTwoAccountsWithDifferentConfig(
        address alice,
        address bob,
        OrderConfigV4 memory aliceConfig,
        OrderConfigV4 memory bobConfig,
        bytes memory aliceExpression,
        bytes memory bobExpression
    ) public {
        vm.assume(alice != bob);
        LibTestAddOrder.conformConfig(aliceConfig, iInterpreter, iStore);
        LibTestAddOrder.conformConfig(bobConfig, iInterpreter, iStore);
        (OrderV4 memory aliceOrder, bytes32 aliceOrderHash) = addOrderWithChecks(alice, aliceConfig, aliceExpression);
        (OrderV4 memory bobOrder, bytes32 bobOrderHash) = addOrderWithChecks(bob, bobConfig, bobExpression);
        (aliceOrder);
        (bobOrder);
        assertTrue(aliceOrderHash != bobOrderHash);
    }

    /// Alice can add orders with different configs. The resulting orders MUST
    /// be different.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderSameAccountWithDifferentConfig(
        address alice,
        OrderConfigV4 memory configOne,
        OrderConfigV4 memory configTwo,
        bytes memory expressionOne,
        bytes memory expressionTwo
    ) public {
        LibTestAddOrder.conformConfig(configOne, iInterpreter, iStore);
        LibTestAddOrder.conformConfig(configTwo, iInterpreter, iStore);
        (OrderV4 memory expectedOrderOne, bytes32 expectedOrderOneHash) =
            LibTestAddOrder.expectedOrder(alice, configOne);
        (OrderV4 memory expectedOrderTwo, bytes32 expectedOrderTwoHash) =
            LibTestAddOrder.expectedOrder(alice, configTwo);
        (expectedOrderOne);
        (expectedOrderTwo);
        assertTrue(expectedOrderOneHash != expectedOrderTwoHash);
        (OrderV4 memory orderOne, bytes32 orderOneHash) = addOrderWithChecks(alice, configOne, expressionOne);
        (OrderV4 memory orderTwo, bytes32 orderTwoHash) = addOrderWithChecks(alice, configTwo, expressionTwo);
        (orderOne);
        (orderTwo);
        assertTrue(orderOneHash != orderTwoHash);
    }
}
