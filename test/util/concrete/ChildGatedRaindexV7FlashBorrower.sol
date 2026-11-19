// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7FlashBorrower} from "../../../src/abstract/RaindexV7FlashBorrower.sol";
import {RaindexV7ArbTaskGated, RaindexV7ArbConfig} from "../../../src/abstract/RaindexV7ArbTaskGated.sol";
import {TaskV3} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";

/// @dev A task-gated flash borrower arb: combines the `arb5` entrypoint from
/// `RaindexV7FlashBorrower` with the task-hash gating mixin
/// `RaindexV7ArbTaskGated`, wiring `_beforeArb` to `_checkTaskHash` (the
/// documented override mechanism). Used to test that `_beforeArb` runs before
/// the zero-orders check in `arb5`.
contract ChildGatedRaindexV7FlashBorrower is RaindexV7FlashBorrower, RaindexV7ArbTaskGated {
    constructor(RaindexV7ArbConfig memory config) RaindexV7ArbTaskGated(config) {}

    function _beforeArb(TaskV3 memory task) internal pure override {
        _checkTaskHash(task);
    }
}
