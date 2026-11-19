// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibTestArb, OrderTakerSetup} from "test/util/lib/LibTestArb.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";

/// arb6 approves the raindex to spend the order's input token (max) before
/// calling takeOrders5, then MUST revoke that allowance back to zero after
/// takeOrders5 returns. This guards against a stale max approval remaining on
/// the orderbook after the arb completes.
contract GenericPoolRaindexV7ArbOrderTakerRaindexApprovalRevokedTest is Test {
    function testRaindexInputApprovalRevokedAfterArb5() external {
        MockExchange exchange = new MockExchange();
        OrderTakerSetup memory setup = LibTestArb.setup(vm, address(exchange), 100e18);

        // Sanity: before the arb there is no allowance to the raindex.
        assertEq(setup.inputToken.allowance(address(setup.arb), address(setup.raindex)), 0, "no allowance before arb");

        setup.arb.arb6(setup.raindex, setup.takeOrdersConfig, LibTestArb.noopTask());

        // After arb6 the raindex's allowance on the input token is revoked to
        // zero. If the post-takeOrders revoke is dropped or set non-zero this
        // assertion fails.
        assertEq(
            setup.inputToken.allowance(address(setup.arb), address(setup.raindex)),
            0,
            "raindex input allowance revoked to zero after arb6"
        );
    }
}
