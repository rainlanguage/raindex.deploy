// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {ChildRaindexV7ArbTaskGated} from "test/util/concrete/ChildRaindexV7ArbTaskGated.sol";
import {RaindexV7ArbConfig, WrongTask} from "../../src/abstract/RaindexV7ArbCommon.sol";
import {TaskV3, EvaluableV4, SignedContextV2} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.36/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.36/src/interface/IInterpreterStoreV3.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.36/test/lib/deploy/LibTestInterpreterDeploy.sol";

/// _checkTaskHash MUST revert with WrongTask when iTaskHash is nonzero and
/// the runtime task does not match.
contract RaindexV7ArbTaskGatedCheckTaskHashWrongTaskTest is Test {
    function testCheckTaskHashRevertsWrongTask(bytes memory constructBytecode, bytes memory runtimeBytecode) external {
        vm.assume(constructBytecode.length > 0);
        vm.assume(runtimeBytecode.length > 0);

        TaskV3 memory constructTask = TaskV3(
            EvaluableV4(
                IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), constructBytecode
            ),
            new SignedContextV2[](0)
        );
        TaskV3 memory runtimeTask = TaskV3(
            EvaluableV4(
                IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), runtimeBytecode
            ),
            new SignedContextV2[](0)
        );

        // Only revert when the tasks are actually different.
        vm.assume(keccak256(abi.encode(constructTask)) != keccak256(abi.encode(runtimeTask)));

        ChildRaindexV7ArbTaskGated child = new ChildRaindexV7ArbTaskGated(RaindexV7ArbConfig(constructTask));

        vm.expectRevert(abi.encodeWithSelector(WrongTask.selector));
        child.checkTaskHash(runtimeTask);
    }
}
