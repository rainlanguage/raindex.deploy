// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {ChildRaindexV7ArbOrderTaker} from "../util/concrete/ChildRaindexV7ArbOrderTaker.sol";
import {TaskV3, SignedContextV2, EvaluableV4} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {RaindexV7ExternalRealTest} from "../util/abstract/RaindexV7ExternalRealTest.sol";
import {
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    IOV2,
    OrderConfigV4,
    OrderV4,
    IInterpreterV4
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// @title RaindexV7ArbOrderTakerFinalizeTaskArgTest
/// @notice Proves that arb6 forwards the CALLER's `task` (the arb6 `task`
/// parameter) into `finalizeArb`, not a default/empty TaskV3. The context
/// tests only ASSERT context values via the task's `ensure()` — but if arb6
/// passed an empty task to finalizeArb, those `ensure()` checks would silently
/// not run and the context tests would still pass. This test pins the forward
/// itself: the supplied task UNCONDITIONALLY reverts when its expression runs,
/// so a correct arb6 (forwarding the real task) MUST revert. Replacing the
/// forwarded task with a default/empty one makes the call SUCCEED, killing the
/// mutant.
contract RaindexV7ArbOrderTakerFinalizeTaskArgTest is RaindexV7ExternalRealTest {
    function testArb5ForwardsTaskToFinalize() external {
        address bob = address(999998);
        ChildRaindexV7ArbOrderTaker arbOrderTaker = new ChildRaindexV7ArbOrderTaker();

        IOV2[] memory validInputs = new IOV2[](1);
        validInputs[0] = IOV2({token: address(iToken0), vaultId: 0});
        IOV2[] memory validOutputs = new IOV2[](1);
        validOutputs[0] = IOV2({token: address(iToken1), vaultId: 0});

        OrderV4 memory order = OrderV4({
            owner: address(999999),
            evaluable: EvaluableV4(iInterpreter, iStore, ""),
            validInputs: validInputs,
            validOutputs: validOutputs,
            nonce: 0
        });

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        orders[0] = TakeOrderConfigV5({
            order: order, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)
        });

        TakeOrdersConfigV6 memory takeOrdersConfig = TakeOrdersConfigV6({
            minimumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
            maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
            IOIsInput: true,
            orders: orders,
            data: ""
        });

        // A task whose body unconditionally reverts when it runs. If arb6
        // forwards this task into finalizeArb (correct), the call reverts with
        // "task-arg-forwarded". If arb6 passes a default/empty task instead, the
        // body never runs and the call succeeds.
        TaskV3 memory task = TaskV3({
            evaluable: EvaluableV4({
                interpreter: iInterpreter,
                store: iStore,
                bytecode: iParserV2.parse2(bytes(":ensure(0 \"task-arg-forwarded\");"))
            }),
            signedContext: new SignedContextV2[](0)
        });

        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.approve.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.allowance.selector), abi.encode(0));
        vm.mockCall(
            address(iToken0), abi.encodeWithSelector(IERC20.balanceOf.selector, address(arbOrderTaker)), abi.encode(0)
        );
        vm.mockCall(
            address(iToken1), abi.encodeWithSelector(IERC20.balanceOf.selector, address(arbOrderTaker)), abi.encode(0)
        );
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(18));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(18));

        vm.prank(bob);
        vm.expectRevert(bytes("task-arg-forwarded"));
        arbOrderTaker.arb6(iRaindex, takeOrdersConfig, task);
    }
}
