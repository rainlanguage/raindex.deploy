// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalMockTest} from "test/util/abstract/RaindexV7ExternalMockTest.sol";
import {LibDecimalFloat, Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {IRaindexV7} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";

/// @title RaindexV7VaultBalanceTest
/// Tests the basic functionality of reading from the vault balance.
contract RaindexV7VaultBalanceTest is RaindexV7ExternalMockTest {
    using LibDecimalFloat for Float;

    /// Test that reading the vault balance without deposits is always zero.
    function testVaultBalanceNoDeposits(address owner, address token, bytes32 vaultId) external view {
        vm.assume(vaultId != bytes32(0));
        assertTrue(iRaindex.vaultBalance2(owner, token, vaultId).isZero());
    }
}
