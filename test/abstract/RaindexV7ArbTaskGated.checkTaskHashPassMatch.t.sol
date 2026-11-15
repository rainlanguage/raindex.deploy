// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {ChildRaindexV7ArbTaskGated} from "test/util/concrete/ChildRaindexV7ArbTaskGated.sol";
import {RaindexV7ArbConfig} from "../../src/abstract/RaindexV7ArbCommon.sol";
import {TaskV3, EvaluableV4, SignedContextV2} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.35/src/interface/IInterpreterStoreV3.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.35/test/lib/deploy/LibTestInterpreterDeploy.sol";

/// _checkTaskHash MUST pass when the runtime task matches the configured task.
contract RaindexV7ArbTaskGatedCheckTaskHashPassMatchTest is Test {
    function testCheckTaskHashPassesWhenMatch(bytes memory bytecode) external {
        vm.assume(bytecode.length > 0);

        TaskV3 memory task = TaskV3(
            EvaluableV4(IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), bytecode),
            new SignedContextV2[](0)
        );
        ChildRaindexV7ArbTaskGated child = new ChildRaindexV7ArbTaskGated(RaindexV7ArbConfig(task));

        // Same task should pass.
        child.checkTaskHash(task);
    }
}
