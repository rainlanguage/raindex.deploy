// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.19;

import {Vm} from "forge-std-1.17.0/src/Vm.sol";
import {
    OrderV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    IOV2,
    EvaluableV4,
    OrderConfigV4,
    TaskV3,
    IRaindexV7
} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.39/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.39/src/interface/IInterpreterStoreV3.sol";
import {IParserV2} from "rainlang-0.2.39/src/interface/IParserV2.sol";
import {SignedContextV2} from "rainlang-0.2.39/src/interface/IInterpreterCallerV5.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS,
    TEST_COMPILER_ADDRESS
} from "rainlang-0.2.39/test/lib/deploy/LibTestInterpreterDeploy.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";

library LibTestTakeOrder {
    /// Extract OrderV4 from the first log entry emitted by addOrder5.
    function extractOrderFromLogs(Vm.Log[] memory entries) internal pure returns (OrderV4 memory) {
        (,, OrderV4 memory order) = abi.decode(entries[0].data, (address, bytes32, OrderV4));
        return order;
    }

    /// Parse an expression string, add the order as `owner`, and return the
    /// resulting OrderV4. Uses deploy constants for interpreter/store/parser
    /// and the raindex.
    function addOrderWithExpression(
        Vm vm,
        address owner,
        bytes memory expression,
        address inputToken,
        bytes32 inputVaultId,
        address outputToken,
        bytes32 outputVaultId
    ) internal returns (OrderV4 memory) {
        IParserV2 parser = IParserV2(TEST_COMPILER_ADDRESS);
        IRaindexV7 raindex = IRaindexV7(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);

        bytes memory bytecode = parser.parse2(expression);
        IOV2[] memory inputs = new IOV2[](1);
        inputs[0] = IOV2(inputToken, inputVaultId);
        IOV2[] memory outputs = new IOV2[](1);
        outputs[0] = IOV2(outputToken, outputVaultId);

        EvaluableV4 memory evaluable =
            EvaluableV4(IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), bytecode);
        OrderConfigV4 memory orderConfig = OrderConfigV4(evaluable, inputs, outputs, bytes32(0), bytes32(0), "");

        vm.prank(owner);
        vm.recordLogs();
        raindex.addOrder5(orderConfig, new TaskV3[](0));
        return extractOrderFromLogs(vm.getRecordedLogs());
    }

    /// Wrap a single order into a TakeOrderConfigV5 array with default IO
    /// indices (0, 0) and empty signed context.
    function wrapSingle(OrderV4 memory order) internal pure returns (TakeOrderConfigV5[] memory) {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        orders[0] = TakeOrderConfigV5(order, 0, 0, new SignedContextV2[](0));
        return orders;
    }

    /// Build a TakeOrdersConfigV6 with standard defaults: minimumIO=0,
    /// maximumIO=max, maximumIORatio=max, IOIsInput=true, data="".
    function defaultTakeConfig(TakeOrderConfigV5[] memory orders) internal pure returns (TakeOrdersConfigV6 memory) {
        return TakeOrdersConfigV6({
            minimumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
            maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
            IOIsInput: true,
            orders: orders,
            data: ""
        });
    }
}
