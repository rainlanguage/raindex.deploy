// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ArbOrderTaker} from "../../../src/abstract/RaindexV7ArbOrderTaker.sol";

/// @dev We need a contract that is deployable in order to test the abstract
/// base contract.
contract ChildRaindexV7ArbOrderTaker is RaindexV7ArbOrderTaker {
    constructor() {}
}
