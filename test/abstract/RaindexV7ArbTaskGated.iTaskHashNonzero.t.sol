// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {ChildRaindexV7ArbTaskGated} from "test/util/concrete/ChildRaindexV7ArbTaskGated.sol";
import {RaindexV7ArbConfig} from "../../src/abstract/RaindexV7ArbCommon.sol";
import {TaskV3, EvaluableV4, SignedContextV2} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.39/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.39/src/interface/IInterpreterStoreV3.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.39/test/lib/deploy/LibTestInterpreterDeploy.sol";

/// When constructed with non-empty bytecode the task hash MUST be the
/// keccak256 of the abi-encoded task.
contract RaindexV7ArbTaskGatedITaskHashNonzeroTest is Test {
    function testITaskHashNonzeroBytecode(bytes memory bytecode) external {
        vm.assume(bytecode.length > 0);

        TaskV3 memory task = TaskV3(
            EvaluableV4(IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), bytecode),
            new SignedContextV2[](0)
        );
        ChildRaindexV7ArbTaskGated child = new ChildRaindexV7ArbTaskGated(RaindexV7ArbConfig(task));
        assertEq(child.iTaskHash(), keccak256(abi.encode(task)));
    }
}
