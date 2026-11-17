// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.19;

import {RaindexV7ArbCommon, RaindexV7ArbConfig, WrongTask} from "./RaindexV7ArbCommon.sol";
import {TaskV3} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";

/// @title RaindexV7ArbTaskGated
/// @notice Mixin that adds task-hash gating to arb contracts. Stores a task
/// hash at construction time. Concrete task-gated contracts override
/// `_beforeArb` from `RaindexV7ArbCommon` to call `_checkTaskHash`.
abstract contract RaindexV7ArbTaskGated is RaindexV7ArbCommon {
    /// @notice Hash of the configured task, or `bytes32(0)` if no task was set.
    bytes32 public immutable iTaskHash = 0;

    /// @param config The arb config for this contract.
    constructor(RaindexV7ArbConfig memory config) {
        emit Construct(msg.sender, config);

        if (config.task.evaluable.bytecode.length != 0) {
            iTaskHash = keccak256(abi.encode(config.task));
        }
    }

    /// @dev Reverts with `WrongTask` if `iTaskHash` is nonzero and does not
    /// match the hash of the provided task. Passes through if no task was
    /// configured at construction.
    //slither-disable-next-line dead-code
    function _checkTaskHash(TaskV3 memory task) internal pure {
        if (iTaskHash != bytes32(0) && iTaskHash != keccak256(abi.encode(task))) {
            revert WrongTask();
        }
    }
}
