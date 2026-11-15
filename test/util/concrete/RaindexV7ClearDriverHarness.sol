// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7} from "src/concrete/raindex/RaindexV7.sol";
import {IRaindexV7ClearDriverHarness} from "test/util/interface/IRaindexV7ClearDriverHarness.sol";
import {Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// @dev RaindexV7 subclass that exposes order-liveness and vault seeding so a
/// full `clear4` can be driven against fresh-compiled source bytecode.
contract RaindexV7ClearDriverHarness is RaindexV7, IRaindexV7ClearDriverHarness {
    function setOrderLive(bytes32 orderHash) external {
        sOrders[orderHash] = 1;
    }

    function seedVault(address owner, address token, bytes32 vaultId, Float amount) external {
        sVaultBalances[owner][token][vaultId] = amount;
    }
}
