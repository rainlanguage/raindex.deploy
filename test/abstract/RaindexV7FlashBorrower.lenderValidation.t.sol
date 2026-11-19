// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";

import {GenericPoolRaindexV7FlashBorrower} from "../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";
import {BadLender} from "../../src/abstract/RaindexV7FlashBorrower.sol";
import {MaliciousLender} from "test/util/concrete/MaliciousLender.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";

contract RaindexV7FlashBorrowerLenderValidationTest is Test {
    /// A malicious lender calls onFlashLoan directly. The call reverts with
    /// BadLender because msg.sender is not the configured raindex.
    function testMaliciousLenderCannotExploitOnFlashLoan() external {
        MockToken token = new MockToken("Token", "TKN", 18);

        GenericPoolRaindexV7FlashBorrower arb = new GenericPoolRaindexV7FlashBorrower();

        MaliciousLender attacker = new MaliciousLender();

        vm.expectRevert(abi.encodeWithSelector(BadLender.selector, address(attacker)));
        attacker.attack(arb, address(token), 0, hex"");
    }
}
