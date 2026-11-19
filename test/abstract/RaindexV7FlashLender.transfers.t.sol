// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalMockTest} from "test/util/abstract/RaindexV7ExternalMockTest.sol";
import {IERC20Errors} from "@openzeppelin-contracts-5.7.0/token/ERC20/ERC20.sol";

import {IERC3156FlashBorrower} from "raindex-interface-0.1.17/src/interface/ierc3156/IERC3156FlashBorrower.sol";
import {FlashLenderCallbackFailed} from "../../src/abstract/RaindexV7FlashLender.sol";
import {TKN} from "test/util/concrete/TKN.sol";
import {IPull} from "test/util/concrete/IPull.sol";
import {Alice} from "test/util/concrete/Alice.sol";
import {Bob} from "test/util/concrete/Bob.sol";
import {Carol} from "test/util/concrete/Carol.sol";

/// @title RaindexV7FlashLenderTransferTest
/// Tests the `RaindexV7FlashLender` transfer functions.
contract RaindexV7FlashLenderTransferTest is RaindexV7ExternalMockTest {
    /// Alice can send tokens to Bob, who will return them and then the loan will
    /// be repaid.
    /// forge-config: default.fuzz.runs = 100
    function testFlashLoanTransferSuccess(uint256 amount, bool success) public {
        TKN tkn = new TKN(address(iRaindex), amount);

        Bob bob = new Bob();
        Alice alice = new Alice(IPull(address(bob)), success);

        if (!success) {
            vm.expectRevert(abi.encodeWithSelector(FlashLenderCallbackFailed.selector, bytes32(0)));
        }
        bool result = iRaindex.flashLoan(IERC3156FlashBorrower(address(alice)), address(tkn), amount, "");
        if (success) {
            assertTrue(result);
        }
    }

    /// A successful flash loan is net-neutral for the lender: the exact `amount`
    /// is sent out to the receiver and the exact `amount + FLASH_FEE` (== amount,
    /// fee 0) is pulled back, so the lender's balance is unchanged and the
    /// receiver holds zero afterwards. Uses a fixed nonzero amount and asserts
    /// concrete balances so it kills both the transfer-OUT amount mutation
    /// (`amount`->`amount+1` over-sends and the round-trip can't repay) and the
    /// transfer-BACK amount mutation (`amount`->`amount-1` leaves the lender
    /// short) for ANY nonzero amount, not just the fuzzed `amount == 0` underflow
    /// edge.
    function testFlashLoanNetNeutralBalances() public {
        uint256 amount = 1e24;
        TKN tkn = new TKN(address(iRaindex), amount);

        uint256 lenderBalanceBefore = tkn.balanceOf(address(iRaindex));
        assertEq(lenderBalanceBefore, amount);

        Bob bob = new Bob();
        Alice alice = new Alice(IPull(address(bob)), true);

        // Receiver starts with nothing.
        assertEq(tkn.balanceOf(address(alice)), 0);

        bool result = iRaindex.flashLoan(IERC3156FlashBorrower(address(alice)), address(tkn), amount, "");
        assertTrue(result);

        // Net-neutral: the lender's balance is exactly restored and the receiver
        // is left holding nothing (it sent the full amount back).
        assertEq(tkn.balanceOf(address(iRaindex)), lenderBalanceBefore);
        assertEq(tkn.balanceOf(address(alice)), 0);
        // Bob (the intermediary) also nets to zero.
        assertEq(tkn.balanceOf(address(bob)), 0);
    }

    /// Alice can send tokens to Carol, who will return not all of them and then
    /// the loan will fail.
    /// forge-config: default.fuzz.runs = 100
    function testFlashLoanTransferFail(uint256 amount, uint256 amountWithheld, bool success) public {
        amount = bound(amount, 1, type(uint256).max);
        amountWithheld = bound(amountWithheld, 1, amount);
        TKN tkn = new TKN(address(iRaindex), amount);

        Carol carol = new Carol(amountWithheld);
        Alice alice = new Alice(IPull(address(carol)), success);

        if (!success) {
            vm.expectRevert(abi.encodeWithSelector(FlashLenderCallbackFailed.selector, bytes32(0)));
        } else {
            vm.expectRevert(
                abi.encodeWithSelector(
                    IERC20Errors.ERC20InsufficientBalance.selector, address(alice), amount - amountWithheld, amount
                )
            );
        }
        iRaindex.flashLoan(IERC3156FlashBorrower(address(alice)), address(tkn), amount, "");
    }
}
