// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";

import {GenericPoolRaindexV7FlashBorrower} from "../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";
import {BadLender} from "../../src/abstract/RaindexV7FlashBorrower.sol";
import {LibRaindexDeploy} from "../../src/lib/deploy/LibRaindexDeploy.sol";

/// The lender check in onFlashLoan happens BEFORE the initiator check. When both
/// the caller (lender) and the initiator are invalid, the lender check fires
/// first and the call reverts with BadLender, not BadInitiator. This pins the
/// ordering of the two guards.
contract RaindexV7FlashBorrowerGuardOrderTest is Test {
    GenericPoolRaindexV7FlashBorrower arb;

    constructor() {
        arb = new GenericPoolRaindexV7FlashBorrower();
    }

    /// When BOTH the lender (msg.sender) and the initiator are bad, onFlashLoan
    /// reverts with BadLender carrying msg.sender (lender guard is checked
    /// first). A bad initiator alone would otherwise revert BadInitiator, so a
    /// BadLender revert here proves the lender guard runs before the initiator
    /// guard.
    function testGuardOrderLenderBeforeInitiator(address badLender, address badInitiator) external {
        // Lender must not be the deterministic raindex (else lender guard
        // passes) and initiator must not be the arb (else initiator guard
        // passes), so BOTH guards are violated simultaneously.
        vm.assume(badLender != LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
        vm.assume(badInitiator != address(arb));

        vm.prank(badLender);
        // BadLender is expected even though the initiator is ALSO bad, because
        // the lender guard is evaluated first.
        vm.expectRevert(abi.encodeWithSelector(BadLender.selector, badLender));
        arb.onFlashLoan(badInitiator, address(0), 0, 0, abi.encode(new bytes(0), new bytes(0)));
    }
}
