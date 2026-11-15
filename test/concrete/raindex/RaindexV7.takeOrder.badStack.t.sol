// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7FreshTakeOrderTest} from "test/util/abstract/RaindexV7FreshTakeOrderTest.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {
    IRaindexV7,
    OrderConfigV4,
    OrderV4,
    TaskV3,
    TakeOrdersConfigV6
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";
import {Vm} from "forge-std-1.17.0/src/Test.sol";

contract RaindexV7TakeOrderBadStackTest is RaindexV7FreshTakeOrderTest {
    using LibDecimalFloat for Float;

    function checkBadStackSkips(address alice, address bob, OrderConfigV4 memory config, bytes memory rainString)
        internal
    {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);

        config.evaluable.bytecode = iParserV2.parse2(rainString);

        OrderV4 memory order = OrderV4(alice, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        TakeOrdersConfigV6 memory takeOrdersConfig =
            LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        config.validInputs[0].token = address(iToken0);
        config.validOutputs[0].token = address(iToken1);

        vm.prank(alice);
        iRaindex.addOrder5(config, new TaskV3[](0));

        vm.recordLogs();
        vm.prank(bob);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(takeOrdersConfig);
        assertTrue(totalTakerInput.isZero(), "totalTakerInput");
        assertTrue(totalTakerOutput.isZero(), "totalTakerOutput");

        Vm.Log[] memory logs = vm.getRecordedLogs();
        assertEq(logs.length, 1, "only OrderCalculateReverted emitted");
        assertEq(logs[0].emitter, address(iRaindex), "emitter");
        assertEq(logs[0].topics[0], IRaindexV7.OrderCalculateReverted.selector, "event is OrderCalculateReverted");
        assertEq(logs[0].data, abi.encode(bob, alice, LibOrder.hash(order)), "event data");
    }

    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderEmptyCalculateStackSkips(address alice, address bob, OrderConfigV4 memory config) external {
        checkBadStackSkips(alice, bob, config, ":;:;");
    }

    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderOneItemCalculateStackSkips(address alice, address bob, OrderConfigV4 memory config) external {
        checkBadStackSkips(alice, bob, config, "_:1;:;");
    }
}
