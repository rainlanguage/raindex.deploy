// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibTestArb, OrderTakerSetup} from "test/util/lib/LibTestArb.sol";
import {AllowanceCheckingExchange} from "test/util/concrete/AllowanceCheckingExchange.sol";

/// After a successful arb6, the spender's allowance on the output token
/// from the arb contract must be zero (approve-call-revoke).
contract GenericPoolRaindexV7ArbOrderTakerApprovalRevokedTest is Test {
    function testApprovalRevokedAfterOnTakeOrders2() external {
        AllowanceCheckingExchange exchange = new AllowanceCheckingExchange();
        OrderTakerSetup memory setup = LibTestArb.setup(vm, address(exchange), 100e18);

        setup.arb.arb6(setup.raindex, setup.takeOrdersConfig, LibTestArb.noopTask());

        // During the call the exchange saw max approval.
        assertEq(exchange.lastAllowance(), type(uint256).max, "exchange saw max allowance during call");

        // After arb6 completes, the spender allowance is revoked to zero.
        assertEq(setup.outputToken.allowance(address(setup.arb), address(exchange)), 0, "allowance revoked to zero");
    }
}
