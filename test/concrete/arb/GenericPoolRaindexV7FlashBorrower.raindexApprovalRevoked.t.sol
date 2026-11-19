// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibTestFlashBorrowerArb, FlashBorrowerSetup} from "test/util/lib/LibTestFlashBorrowerArb.sol";
import {LibTestArb} from "test/util/lib/LibTestArb.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";

/// arb5 grants the raindex max approval on BOTH the order's input and output
/// tokens before taking the flash loan, then MUST revoke both back to zero
/// after the flash loan returns. This guards against a stale max approval to
/// the orderbook persisting after the arb completes.
contract GenericPoolRaindexV7FlashBorrowerRaindexApprovalRevokedTest is Test {
    function testRaindexApprovalsRevokedAfterArb4() external {
        MockExchange exchange = new MockExchange();
        FlashBorrowerSetup memory setup = LibTestFlashBorrowerArb.setup(vm, address(exchange), 100e18);

        setup.arb.arb5(setup.raindex, setup.takeOrdersConfig, setup.exchangeData, LibTestArb.noopTask());

        // After arb5 the raindex's allowance on the OUTPUT token (the borrowed
        // token) is revoked to zero. If the post-flashLoan output revoke is
        // dropped or set non-zero this assertion fails.
        assertEq(
            setup.outputToken.allowance(address(setup.arb), address(setup.raindex)),
            0,
            "raindex output allowance revoked to zero after arb5"
        );

        // After arb5 the raindex's allowance on the INPUT token is also revoked
        // to zero. If the post-flashLoan input revoke is dropped or set
        // non-zero this assertion fails.
        assertEq(
            setup.inputToken.allowance(address(setup.arb), address(setup.raindex)),
            0,
            "raindex input allowance revoked to zero after arb5"
        );
    }
}
