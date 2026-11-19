// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.19;

import {TaskV3} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";

/// @param task The task to run as post for each arb.
struct RaindexV7ArbConfig {
    TaskV3 task;
}

/// Thrown when the task does not match the expected hash.
error WrongTask();

/// Thrown when an arb is called with a `raindex` that is not the trusted
/// deterministic deployment. The arb grants `raindex` an unlimited token
/// approval and calls it, so an untrusted address could drain the contract's
/// balance.
/// @param badRaindex The untrusted `raindex` passed to the arb.
error BadRaindex(address badRaindex);

/// @title RaindexV7ArbCommon
/// @notice Common base for arb contracts that interact with `Raindex`.
/// Provides a `_beforeArb` hook that is called at the start of every arb
/// operation. The base implementation is a no-op; task-gated variants
/// override it to validate the task hash.
abstract contract RaindexV7ArbCommon {
    /// @notice Emitted on construction with the full config.
    /// @param sender The deployer address.
    /// @param config The arb config used to construct this contract.
    event Construct(address sender, RaindexV7ArbConfig config);

    /// @dev Hook called at the start of every arb. Base implementation is a
    /// no-op. Overridden by `RaindexV7ArbTaskGated` to validate the task
    /// hash.
    function _beforeArb(TaskV3 memory task) internal virtual {}
}
