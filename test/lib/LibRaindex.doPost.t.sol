// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibRaindexDoPostHarness} from "test/util/concrete/LibRaindexDoPostHarness.sol";
import {TaskV3} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IInterpreterV4, StackItem} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.35/src/interface/IInterpreterStoreV3.sol";
import {StateNamespace} from "rainlang-0.2.35/src/interface/deprecated/v2/IInterpreterStoreV2.sol";
import {EvaluableV4} from "rainlang-0.2.35/src/interface/IInterpreterCallerV5.sol";
import {SignedContextV2} from "rainlang-0.2.35/src/interface/IInterpreterCallerV5.sol";

/// @title LibRaindexDoPostTest
/// @notice Audit GAP-A11-1/2/3 (#2536): `doPost` skips work for an empty post
/// array, skips tasks with empty bytecode, and only calls `store.set` when the
/// evaluation produced writes.
contract LibRaindexDoPostTest is Test {
    LibRaindexDoPostHarness internal immutable iHarness = new LibRaindexDoPostHarness();

    /// GAP-A11-1: an empty post array is a no-op (no interpreter/store calls).
    function testDoPostEmptyPostArrayNoOp() external {
        iHarness.doPost(new bytes32[][](0), new TaskV3[](0));
    }

    /// GAP-A11-2: a task with empty bytecode is skipped — neither `eval4` nor
    /// `store.set` is called (both are mocked to revert if reached).
    function testDoPostSkipsEmptyBytecodeTask() external {
        address interpreter = makeAddr("interpreter");
        address store = makeAddr("store");
        vm.mockCallRevert(
            interpreter, abi.encodeWithSelector(IInterpreterV4.eval4.selector), bytes("eval must be skipped")
        );
        vm.mockCallRevert(store, abi.encodeWithSelector(IInterpreterStoreV3.set.selector), bytes("set must be skipped"));

        TaskV3[] memory post = new TaskV3[](1);
        post[0] = TaskV3({
            evaluable: EvaluableV4(IInterpreterV4(interpreter), IInterpreterStoreV3(store), hex""),
            signedContext: new SignedContextV2[](0)
        });
        iHarness.doPost(new bytes32[][](0), post);
    }

    /// GAP-A11-3: when `eval4` produces no writes, `store.set` is not called
    /// (mocked to revert if reached).
    function testDoPostNoStoreSetWhenNoWrites() external {
        address interpreter = makeAddr("interpreter");
        address store = makeAddr("store");
        vm.mockCall(
            interpreter,
            abi.encodeWithSelector(IInterpreterV4.eval4.selector),
            abi.encode(new StackItem[](0), new bytes32[](0))
        );
        // Assert eval4 actually ran, so the test can't pass vacuously if doPost
        // were to skip the task entirely (then store.set would also not be hit).
        vm.expectCall(interpreter, abi.encodeWithSelector(IInterpreterV4.eval4.selector));
        vm.mockCallRevert(store, abi.encodeWithSelector(IInterpreterStoreV3.set.selector), bytes("set must be skipped"));

        TaskV3[] memory post = new TaskV3[](1);
        post[0] = TaskV3({
            evaluable: EvaluableV4(IInterpreterV4(interpreter), IInterpreterStoreV3(store), hex"01"),
            signedContext: new SignedContextV2[](0)
        });
        iHarness.doPost(new bytes32[][](0), post);
    }

    /// Contrast for GAP-A11-3: when `eval4` produces writes, `store.set` IS
    /// called with them.
    function testDoPostStoreSetWhenWrites() external {
        address interpreter = makeAddr("interpreter");
        address store = makeAddr("store");
        bytes32[] memory writes = new bytes32[](2);
        writes[0] = bytes32(uint256(1));
        writes[1] = bytes32(uint256(2));
        vm.mockCall(
            interpreter, abi.encodeWithSelector(IInterpreterV4.eval4.selector), abi.encode(new StackItem[](0), writes)
        );
        // Pin the full path: eval4 runs, returns writes, and store.set receives
        // them. The expectCall pins the FULL calldata — the namespace
        // (derived from msg.sender == this test) and the exact writes — so a
        // mutation forwarding different writes or a different namespace to
        // store.set is caught, not just any call to the selector.
        vm.expectCall(interpreter, abi.encodeWithSelector(IInterpreterV4.eval4.selector));
        vm.mockCall(store, abi.encodeWithSelector(IInterpreterStoreV3.set.selector), bytes(""));
        vm.expectCall(
            store,
            abi.encodeCall(IInterpreterStoreV3.set, (StateNamespace.wrap(uint256(uint160(address(this)))), writes))
        );

        TaskV3[] memory post = new TaskV3[](1);
        post[0] = TaskV3({
            evaluable: EvaluableV4(IInterpreterV4(interpreter), IInterpreterStoreV3(store), hex"01"),
            signedContext: new SignedContextV2[](0)
        });
        iHarness.doPost(new bytes32[][](0), post);
    }
}
