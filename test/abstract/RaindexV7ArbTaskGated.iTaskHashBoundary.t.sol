// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {ChildRaindexV7ArbTaskGated} from "test/util/concrete/ChildRaindexV7ArbTaskGated.sol";
import {RaindexV7ArbConfig} from "../../src/abstract/RaindexV7ArbCommon.sol";
import {TaskV3, EvaluableV4, SignedContextV2} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.36/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.36/src/interface/IInterpreterStoreV3.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.36/test/lib/deploy/LibTestInterpreterDeploy.sol";

/// The constructor branch on `bytecode.length != 0` MUST trip at the exact
/// boundary of one byte: empty bytecode leaves `iTaskHash` at zero (branch not
/// taken), but a SINGLE byte of bytecode is enough to store the keccak hash.
/// This pins the comparator to `!= 0` (i.e. `>= 1`); an off-by-one mutation
/// such as `> 1` would wrongly leave a one-byte task ungated.
contract RaindexV7ArbTaskGatedITaskHashBoundaryTest is Test {
    function buildTask(bytes memory bytecode) internal pure returns (TaskV3 memory) {
        return TaskV3(
            EvaluableV4(IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), bytecode),
            new SignedContextV2[](0)
        );
    }

    /// Zero-length bytecode: hash stays exactly zero.
    function testITaskHashBoundaryZeroLength() external {
        TaskV3 memory task = buildTask("");
        ChildRaindexV7ArbTaskGated child = new ChildRaindexV7ArbTaskGated(RaindexV7ArbConfig(task));
        assertEq(child.iTaskHash(), bytes32(0), "empty bytecode must leave iTaskHash zero");
    }

    /// One-byte bytecode is over the boundary: hash is the keccak of the task
    /// and is necessarily nonzero.
    function testITaskHashBoundaryOneByteLength() external {
        TaskV3 memory task = buildTask(hex"01");
        ChildRaindexV7ArbTaskGated child = new ChildRaindexV7ArbTaskGated(RaindexV7ArbConfig(task));
        bytes32 expected = keccak256(abi.encode(task));
        assertEq(child.iTaskHash(), expected, "one byte of bytecode must store the keccak task hash");
        assertTrue(child.iTaskHash() != bytes32(0), "one-byte task hash must be nonzero");
    }
}
