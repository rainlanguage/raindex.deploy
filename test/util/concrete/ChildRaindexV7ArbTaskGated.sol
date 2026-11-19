// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ArbTaskGated, RaindexV7ArbConfig} from "../../../src/abstract/RaindexV7ArbTaskGated.sol";
import {TaskV3} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";

/// @dev We need a contract that is deployable in order to test the abstract
/// base contract. Exposes `_checkTaskHash` as an external function.
contract ChildRaindexV7ArbTaskGated is RaindexV7ArbTaskGated {
    constructor(RaindexV7ArbConfig memory config) RaindexV7ArbTaskGated(config) {}

    function checkTaskHash(TaskV3 memory task) external pure {
        _checkTaskHash(task);
    }
}
