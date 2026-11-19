// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {OrderConfigV4, OrderV4, EvaluableV4, TaskV3} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";
import {NotOrderOwner} from "../../../src/error/ErrRaindexV7.sol";

contract RaindexV7RemoveOrderOwnerTest is RaindexV7ExternalRealTest {
    using LibOrder for OrderV4;

    /// forge-config: default.fuzz.runs = 100
    function testRemoveOrderOwnerSameOrderNoop(address owner, OrderConfigV4 memory config) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);

        OrderV4 memory order = OrderV4(owner, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.startPrank(owner);

        for (uint256 i = 0; i < 2; i++) {
            bool stateChange = iRaindex.addOrder5(config, new TaskV3[](0));
            assert(stateChange);
            assert(iRaindex.orderExists(order.hash()));
            stateChange = iRaindex.addOrder5(config, new TaskV3[](0));
            assert(!stateChange);
            assert(iRaindex.orderExists(order.hash()));

            stateChange = iRaindex.removeOrder4(order, new TaskV3[](0));
            assert(stateChange);
            assert(!iRaindex.orderExists(order.hash()));
            stateChange = iRaindex.removeOrder4(order, new TaskV3[](0));
            assert(!stateChange);
            assert(!iRaindex.orderExists(order.hash()));
        }

        vm.stopPrank();
    }

    /// forge-config: default.fuzz.runs = 100
    function testRemoveOrderOwnerDifferentOwnerStateChange(OrderConfigV4 memory config, address alice, address bob)
        public
    {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        vm.assume(alice != bob);

        OrderV4 memory orderAlice =
            OrderV4(alice, config.evaluable, config.validInputs, config.validOutputs, config.nonce);
        OrderV4 memory orderBob = OrderV4(bob, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        {
            vm.prank(alice);
            bool stateChange = iRaindex.addOrder5(config, new TaskV3[](0));

            assert(stateChange);
            assert(iRaindex.orderExists(orderAlice.hash()));
            assert(!iRaindex.orderExists(orderBob.hash()));

            vm.prank(bob);
            stateChange = iRaindex.removeOrder4(orderBob, new TaskV3[](0));
            assert(!stateChange);
            assert(iRaindex.orderExists(orderAlice.hash()));
            assert(!iRaindex.orderExists(orderBob.hash()));

            vm.prank(alice);
            stateChange = iRaindex.removeOrder4(orderAlice, new TaskV3[](0));
            assert(stateChange);
            assert(!iRaindex.orderExists(orderAlice.hash()));
            assert(!iRaindex.orderExists(orderBob.hash()));
        }

        {
            vm.prank(bob);
            bool stateChange = iRaindex.addOrder5(config, new TaskV3[](0));
            assert(stateChange);
            assert(iRaindex.orderExists(orderBob.hash()));
            assert(!iRaindex.orderExists(orderAlice.hash()));

            vm.prank(alice);
            stateChange = iRaindex.removeOrder4(orderAlice, new TaskV3[](0));
            assert(!stateChange);
            assert(iRaindex.orderExists(orderBob.hash()));
            assert(!iRaindex.orderExists(orderAlice.hash()));

            vm.prank(bob);
            stateChange = iRaindex.removeOrder4(orderBob, new TaskV3[](0));
            assert(stateChange);
            assert(!iRaindex.orderExists(orderBob.hash()));
            assert(!iRaindex.orderExists(orderAlice.hash()));
        }

        {
            vm.prank(alice);
            bool stateChange = iRaindex.addOrder5(config, new TaskV3[](0));
            assert(stateChange);
            assert(iRaindex.orderExists(orderAlice.hash()));
            assert(!iRaindex.orderExists(orderBob.hash()));

            vm.prank(bob);
            stateChange = iRaindex.addOrder5(config, new TaskV3[](0));
            assert(stateChange);
            assert(iRaindex.orderExists(orderBob.hash()));
            assert(iRaindex.orderExists(orderAlice.hash()));

            vm.prank(alice);
            stateChange = iRaindex.removeOrder4(orderAlice, new TaskV3[](0));
            assert(stateChange);
            assert(!iRaindex.orderExists(orderAlice.hash()));
            assert(iRaindex.orderExists(orderBob.hash()));

            vm.prank(bob);
            stateChange = iRaindex.removeOrder4(orderBob, new TaskV3[](0));
            assert(stateChange);
            assert(!iRaindex.orderExists(orderBob.hash()));
            assert(!iRaindex.orderExists(orderAlice.hash()));
        }
    }

    /// forge-config: default.fuzz.runs = 100
    function testRemoveOrderWrongOwner(OrderConfigV4 memory config, address alice, address bob) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        vm.assume(alice != bob);

        OrderV4 memory order = OrderV4(alice, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.prank(alice);
        bool stateChange = iRaindex.addOrder5(config, new TaskV3[](0));
        assert(stateChange);
        assert(iRaindex.orderExists(order.hash()));

        vm.prank(bob);
        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, alice));
        stateChange = iRaindex.removeOrder4(order, new TaskV3[](0));
        assert(!stateChange);
        assert(iRaindex.orderExists(order.hash()));
    }
}
