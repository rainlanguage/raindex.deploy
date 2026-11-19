// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {IRaindexV7, Float} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";

interface IRaindexV7VaultBalanceHarness is IRaindexV7 {
    function exposedIncrease(address owner, address token, bytes32 vaultId, Float amount)
        external
        returns (Float, Float);

    function exposedDecrease(address owner, address token, bytes32 vaultId, Float amount)
        external
        returns (Float, Float);

    function exposedPull(address account, address token, Float amount) external returns (uint256, uint8);

    function exposedPush(address account, address token, Float amount) external returns (uint256, uint8);
}
