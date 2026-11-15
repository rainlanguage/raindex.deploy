// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7} from "src/concrete/raindex/RaindexV7.sol";
import {IRaindexV7VaultBalanceHarness} from "test/util/interface/IRaindexV7VaultBalanceHarness.sol";
import {Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// @dev Exposes RaindexV7's internal vault-balance / token helpers for testing.
contract RaindexV7VaultBalanceHarness is RaindexV7, IRaindexV7VaultBalanceHarness {
    function exposedIncrease(address owner, address token, bytes32 vaultId, Float amount)
        external
        returns (Float, Float)
    {
        return increaseVaultBalance(owner, token, vaultId, amount);
    }

    function exposedDecrease(address owner, address token, bytes32 vaultId, Float amount)
        external
        returns (Float, Float)
    {
        return decreaseVaultBalance(owner, token, vaultId, amount);
    }

    function exposedPull(address account, address token, Float amount) external returns (uint256, uint8) {
        return pullTokens(account, token, amount);
    }

    function exposedPush(address account, address token, Float amount) external returns (uint256, uint8) {
        return pushTokens(account, token, amount);
    }
}
