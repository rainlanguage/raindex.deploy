// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {
    RouteProcessorRaindexV7ArbOrderTakerTest
} from "test/util/abstract/RouteProcessorRaindexV7ArbOrderTakerTest.sol";
import {
    OrderV4,
    EvaluableV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    IInterpreterV4,
    IInterpreterStoreV3,
    TaskV3,
    SignedContextV2
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {RouteProcessorRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/RouteProcessorRaindexV7ArbOrderTaker.sol";
import {LibDecimalFloat, Float} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.40/test/lib/deploy/LibTestInterpreterDeploy.sol";

contract RouteProcessorRaindexV7ArbOrderTakerSenderTest is RouteProcessorRaindexV7ArbOrderTakerTest {
    /// forge-config: default.fuzz.runs = 100
    function testRouteProcessorTakeOrdersSender(OrderV4 memory order, uint256 inputIOIndex, uint256 outputIOIndex)
        public
    {
        TakeOrderConfigV5[] memory orders = buildTakeOrderConfig(order, inputIOIndex, outputIOIndex);

        RouteProcessorRaindexV7ArbOrderTaker(iArb)
            .arb6(
                iRaindex,
                TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(0, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: abi.encode(bytes("0x00"))
            }),
                TaskV3({
                evaluable: EvaluableV4(
                    IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), ""
                ),
                signedContext: new SignedContextV2[](0)
            })
            );
    }
}
