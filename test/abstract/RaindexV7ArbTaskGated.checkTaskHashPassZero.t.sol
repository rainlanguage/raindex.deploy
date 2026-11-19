// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {ChildRaindexV7ArbTaskGated} from "test/util/concrete/ChildRaindexV7ArbTaskGated.sol";
import {RaindexV7ArbConfig} from "../../src/abstract/RaindexV7ArbCommon.sol";
import {TaskV3, EvaluableV4, SignedContextV2} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.40/test/lib/deploy/LibTestInterpreterDeploy.sol";

/// _checkTaskHash MUST pass through when iTaskHash is zero, regardless of
/// the task provided at runtime.
contract RaindexV7ArbTaskGatedCheckTaskHashPassZeroTest is Test {
    function testCheckTaskHashPassesWhenZero(bytes memory runtimeBytecode) external {
        // Construct with empty bytecode so iTaskHash == 0.
        TaskV3 memory constructTask = TaskV3(
            EvaluableV4(IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), ""),
            new SignedContextV2[](0)
        );
        ChildRaindexV7ArbTaskGated child = new ChildRaindexV7ArbTaskGated(RaindexV7ArbConfig(constructTask));

        // Any runtime task should pass.
        TaskV3 memory runtimeTask = TaskV3(
            EvaluableV4(
                IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), runtimeBytecode
            ),
            new SignedContextV2[](0)
        );
        child.checkTaskHash(runtimeTask);
    }
}
