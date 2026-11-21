// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {GenericPoolRaindexV7ArbOrderTakerTest} from "test/util/abstract/GenericPoolRaindexV7ArbOrderTakerTest.sol";

import {GenericPoolRaindexV7ArbOrderTaker} from "../../src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol";
import {
    IRaindexV7,
    EvaluableV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    IInterpreterV4,
    IInterpreterStoreV3,
    TaskV3,
    SignedContextV2
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.40/test/lib/deploy/LibTestInterpreterDeploy.sol";

contract RaindexV7ArbOrderTakerNoOrdersTest is GenericPoolRaindexV7ArbOrderTakerTest {
    /// arb6 MUST revert with NoOrders when given an empty orders array.
    function testArb5NoOrders() external {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](0);

        vm.expectRevert(abi.encodeWithSelector(IRaindexV7.NoOrders.selector));
        GenericPoolRaindexV7ArbOrderTaker(iArb)
            .arb6(
                iRaindex,
                TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(0, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: abi.encode(iRefundoor, iRefundoor, "")
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
