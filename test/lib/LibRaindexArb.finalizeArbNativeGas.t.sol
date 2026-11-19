// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibTestArb, ArbResult} from "test/util/lib/LibTestArb.sol";

contract LibRaindexArbFinalizeArbNativeGasTest is Test {
    /// finalizeArb MUST send native gas balance to msg.sender.
    function testFinalizeArbSendsNativeGas() external {
        uint256 senderBalanceBefore = address(this).balance;

        // Raindex has 100e18 output, pulls 100e18 input. Exchange has 100e18 input.
        // No token profit. 1 ether sent with arb6, returned by exchange, swept.
        ArbResult memory result =
            LibTestArb.setupAndArb(vm, 100e18, 100e18, 100e18, 100e18, LibTestArb.noopTask(), 1 ether);

        // ETH swept back to msg.sender by finalizeArb — net zero.
        assertEq(address(this).balance, senderBalanceBefore, "sender ETH");
        // Arb contract has no remaining ETH.
        assertEq(address(result.arb).balance, 0, "arb ETH");
        // Exchange has no remaining ETH.
        assertEq(address(result.exchange).balance, 0, "exchange ETH");
    }

    /// Needed to receive ETH from finalizeArb.
    receive() external payable {}
}
