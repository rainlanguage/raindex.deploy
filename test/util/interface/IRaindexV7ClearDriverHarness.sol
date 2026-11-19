// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {IRaindexV7, Float} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";

interface IRaindexV7ClearDriverHarness is IRaindexV7 {
    function setOrderLive(bytes32 orderHash) external;

    function seedVault(address owner, address token, bytes32 vaultId, Float amount) external;
}
