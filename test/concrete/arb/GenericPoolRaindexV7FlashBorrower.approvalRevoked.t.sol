// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibTestFlashBorrowerArb, FlashBorrowerSetup} from "test/util/lib/LibTestFlashBorrowerArb.sol";
import {LibTestArb} from "test/util/lib/LibTestArb.sol";
import {AllowanceCheckingExchange} from "test/util/concrete/AllowanceCheckingExchange.sol";

/// After a successful arb5, the spender's allowance on the borrowed token
/// from the arb contract must be zero (approve-call-revoke).
contract GenericPoolRaindexV7FlashBorrowerApprovalRevokedTest is Test {
    function testApprovalRevokedAfterExchange() external {
        AllowanceCheckingExchange exchange = new AllowanceCheckingExchange();
        FlashBorrowerSetup memory setup = LibTestFlashBorrowerArb.setup(vm, address(exchange), 100e18);

        setup.arb.arb5(setup.raindex, setup.takeOrdersConfig, setup.exchangeData, LibTestArb.noopTask());

        // During the call the exchange saw max approval.
        assertEq(exchange.lastAllowance(), type(uint256).max);

        // After arb5 completes, the spender allowance is revoked to zero.
        assertEq(setup.outputToken.allowance(address(setup.arb), address(exchange)), 0);
    }
}
