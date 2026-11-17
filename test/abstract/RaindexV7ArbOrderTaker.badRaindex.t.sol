// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {GenericPoolRaindexV7ArbOrderTakerTest} from "test/util/abstract/GenericPoolRaindexV7ArbOrderTakerTest.sol";

import {GenericPoolRaindexV7ArbOrderTaker} from "../../src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol";
import {BadRaindex} from "../../src/abstract/RaindexV7ArbCommon.sol";
import {LibRaindexDeploy} from "../../src/lib/deploy/LibRaindexDeploy.sol";
import {
    IRaindexV7,
    OrderV4,
    EvaluableV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    IInterpreterV4,
    IInterpreterStoreV3,
    TaskV3,
    SignedContextV2
} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.36/test/lib/deploy/LibTestInterpreterDeploy.sol";

contract RaindexV7ArbOrderTakerBadRaindexTest is GenericPoolRaindexV7ArbOrderTakerTest {
    /// arb6 MUST revert BadRaindex when `raindex` is not the trusted deterministic
    /// deployment, before any token approval is granted. A caller-supplied
    /// untrusted `raindex` would otherwise receive an unlimited approval and could
    /// drain the arb; the guard rejects it up front (the take-orders path is never
    /// reached). The non-revert control is exercised by every other arb6 test,
    /// which calls the etched raindex at RAINDEX_DEPLOYED_ADDRESS.
    function testArb5RevertsUntrustedRaindex(address badRaindex) external {
        vm.assume(badRaindex != LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);

        // A single valid order so the NoOrders guard passes and execution reaches
        // the raindex-trust check.
        OrderV4 memory order;
        TakeOrderConfigV5[] memory orders = buildTakeOrderConfig(order, 0, 0);

        vm.expectRevert(abi.encodeWithSelector(BadRaindex.selector, badRaindex));
        GenericPoolRaindexV7ArbOrderTaker(iArb)
            .arb6(
                IRaindexV7(badRaindex),
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
