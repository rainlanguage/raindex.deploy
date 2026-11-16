// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {OrderConfigV4, OrderV4, EvaluableV4, TaskV3} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";

contract RaindexV7AddOrderOwnerTest is RaindexV7ExternalRealTest {
    using LibOrder for OrderV4;

    /// forge-config: default.fuzz.runs = 100
    function testAddOrderOwnerSameOrderNoop(address owner, OrderConfigV4 memory config) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);

        OrderV4 memory order = OrderV4(owner, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.prank(owner);
        bool stateChange = iRaindex.addOrder5(config, new TaskV3[](0));
        assert(stateChange);
        assert(iRaindex.orderExists(order.hash()));

        vm.prank(owner);
        stateChange = iRaindex.addOrder5(config, new TaskV3[](0));
        assert(!stateChange);
        assert(iRaindex.orderExists(order.hash()));
    }

    /// forge-config: default.fuzz.runs = 100
    function testAddOrderOwnerDifferentOwnerStateChange(OrderConfigV4 memory config, address alice, address bob)
        public
    {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        vm.assume(alice != bob);

        OrderV4 memory orderAlice =
            OrderV4(alice, config.evaluable, config.validInputs, config.validOutputs, config.nonce);
        OrderV4 memory orderBob = OrderV4(bob, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.prank(alice);
        bool stateChange = iRaindex.addOrder5(config, new TaskV3[](0));
        assert(stateChange);
        assert(iRaindex.orderExists(orderAlice.hash()));

        vm.prank(bob);
        stateChange = iRaindex.addOrder5(config, new TaskV3[](0));
        assert(stateChange);
        assert(iRaindex.orderExists(orderBob.hash()));
    }
}
