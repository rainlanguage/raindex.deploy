// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {RevertOnZeroTransferToken} from "test/util/concrete/RevertOnZeroTransferToken.sol";
import {LibRaindexArbFinalizeHarness} from "test/util/concrete/LibRaindexArbFinalizeHarness.sol";
import {RevertOnZeroReceive} from "test/util/concrete/RevertOnZeroReceive.sol";

/// @title LibRaindexArbFinalizeArbZeroGuardTest
/// @notice `finalizeArb` guards each sweep with `if (balance > 0)`. With a token
/// that reverts on a zero-value transfer (and a recipient that reverts on a
/// zero-value native send), a zero balance MUST be skipped: relaxing any guard
/// to `>= 0` would attempt the reverting zero transfer/send and fail the whole
/// call.
contract LibRaindexArbFinalizeArbZeroGuardTest is Test {
    LibRaindexArbFinalizeHarness internal harness;

    function setUp() external {
        harness = new LibRaindexArbFinalizeHarness();
    }

    /// Zero balance of a revert-on-zero INPUT token: the `if (inputBalance > 0)`
    /// guard skips the transfer. Relaxing it to `>= 0` attempts a zero transfer
    /// that reverts. The output token holds a real balance so only the input
    /// guard's zero path is under test.
    function testFinalizeSkipsZeroInputTransfer() external {
        RevertOnZeroTransferToken inputToken = new RevertOnZeroTransferToken("In", "IN", 18);
        MockToken outputToken = new MockToken("Out", "OUT", 18);
        // Harness holds ZERO input but a real output balance.
        outputToken.mint(address(harness), 5e18);

        // Does not revert: the zero input balance is skipped by the guard.
        harness.callFinalize(address(inputToken), 18, address(outputToken), 18);

        // The real output was still swept to the caller (this test).
        assertEq(outputToken.balanceOf(address(this)), 5e18, "output swept");
        assertEq(outputToken.balanceOf(address(harness)), 0, "harness output emptied");
    }

    /// Zero balance of a revert-on-zero OUTPUT token: the `if (outputBalance > 0)`
    /// guard skips the transfer. Relaxing it to `>= 0` reverts on the zero
    /// transfer.
    function testFinalizeSkipsZeroOutputTransfer() external {
        MockToken inputToken = new MockToken("In", "IN", 18);
        RevertOnZeroTransferToken outputToken = new RevertOnZeroTransferToken("Out", "OUT", 18);
        // Harness holds a real input but ZERO output balance.
        inputToken.mint(address(harness), 7e18);

        harness.callFinalize(address(inputToken), 18, address(outputToken), 18);

        assertEq(inputToken.balanceOf(address(this)), 7e18, "input swept");
        assertEq(inputToken.balanceOf(address(harness)), 0, "harness input emptied");
    }

    /// Zero native balance with a caller that reverts on any value receipt: the
    /// `if (gasBalance > 0)` guard skips the send. Relaxing it to `>= 0` performs
    /// a `call{value: 0}` to the reverting receiver and fails. Both token
    /// balances are zero too (and use revert-on-zero tokens) so every guard must
    /// hold simultaneously.
    function testFinalizeSkipsZeroGasSend() external {
        RevertOnZeroTransferToken inputToken = new RevertOnZeroTransferToken("In", "IN", 18);
        RevertOnZeroTransferToken outputToken = new RevertOnZeroTransferToken("Out", "OUT", 18);
        RevertOnZeroReceive caller = new RevertOnZeroReceive();

        // Harness holds no native balance; caller is msg.sender for finalizeArb.
        // If any guard were relaxed the call would revert (reverting token
        // transfer or reverting zero-value send).
        caller.go(harness, address(inputToken), address(outputToken));

        // Nothing moved: all balances zero, no revert.
        assertEq(address(harness).balance, 0, "harness no gas");
        assertEq(address(caller).balance, 0, "caller no gas");
    }
}
