// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {ChildGatedRaindexV7FlashBorrower} from "test/util/concrete/ChildGatedRaindexV7FlashBorrower.sol";
import {RaindexV7ArbConfig, WrongTask} from "../../src/abstract/RaindexV7ArbCommon.sol";
import {
    IRaindexV7,
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    TaskV3,
    EvaluableV4,
    SignedContextV2
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.35/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.35/test/lib/deploy/LibTestInterpreterDeploy.sol";

/// @title RaindexV7FlashBorrowerBeforeArbOrderTest
/// @notice Pins the ORDERING of `arb5`: `_beforeArb(task)` is called BEFORE the
/// zero-orders check. On a task-gated arb, supplying empty orders together with
/// a task that does NOT match the configured hash MUST revert `WrongTask` (the
/// gate runs first), NOT `NoOrders`. If `_beforeArb` were moved below the
/// `orders.length == 0` check, the empty-orders path would revert `NoOrders`
/// before the gate ran, so the discriminator is the SELECTOR of the revert.
/// This is the arb5 (flash borrower) analog of the arb6 ordering test.
contract RaindexV7FlashBorrowerBeforeArbOrderTest is Test {
    function _gatedTask(bytes memory bytecode) internal pure returns (TaskV3 memory) {
        return TaskV3({
            evaluable: EvaluableV4(
                IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), bytecode
            ),
            signedContext: new SignedContextV2[](0)
        });
    }

    function _emptyOrdersConfig() internal pure returns (TakeOrdersConfigV6 memory) {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](0);
        return TakeOrdersConfigV6({
            minimumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
            maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
            IOIsInput: true,
            orders: orders,
            data: ""
        });
    }

    /// With empty orders AND a non-matching task, the gate reverts WrongTask
    /// (it ran before the zero-orders check).
    function testBeforeArbRunsBeforeZeroOrdersCheck() external {
        // Configure the gate with a nonzero-bytecode task so iTaskHash != 0.
        TaskV3 memory configuredTask = _gatedTask(hex"01");
        ChildGatedRaindexV7FlashBorrower arb = new ChildGatedRaindexV7FlashBorrower(RaindexV7ArbConfig(configuredTask));

        // A DIFFERENT runtime task so the hash will not match -> gate reverts.
        TaskV3 memory wrongTask = _gatedTask(hex"02");

        vm.expectRevert(abi.encodeWithSelector(WrongTask.selector));
        arb.arb5(IRaindexV7(address(0xdead)), _emptyOrdersConfig(), "", wrongTask);
    }

    /// Control: with empty orders and a MATCHING task, the gate passes and the
    /// zero-orders check then reverts NoOrders. This proves the WrongTask above
    /// is the gate firing (not an unconditional revert), and that an
    /// empty-orders call still reverts NoOrders once the gate is satisfied.
    function testGatePassesThenNoOrders() external {
        TaskV3 memory configuredTask = _gatedTask(hex"01");
        ChildGatedRaindexV7FlashBorrower arb = new ChildGatedRaindexV7FlashBorrower(RaindexV7ArbConfig(configuredTask));

        vm.expectRevert(abi.encodeWithSelector(IRaindexV7.NoOrders.selector));
        arb.arb5(IRaindexV7(address(0xdead)), _emptyOrdersConfig(), "", configuredTask);
    }
}
