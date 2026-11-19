// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7FlashBorrower} from "../../../src/abstract/RaindexV7FlashBorrower.sol";

/// @dev Deployable child of the abstract flash borrower with a no-op `_exchange`
/// so the test exercises ONLY the abstract `arb5` decimals/finalize plumbing,
/// not any concrete swap.
contract NoExchangeFlashBorrower is RaindexV7FlashBorrower {
    constructor() {}
}
