// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalMockTest} from "test/util/abstract/RaindexV7ExternalMockTest.sol";
import {
    IERC3156FlashBorrower,
    ON_FLASH_LOAN_CALLBACK_SUCCESS
} from "raindex-interface-0.1.16/src/interface/ierc3156/IERC3156FlashBorrower.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

/// @title RaindexV7FlashLenderMockSuccessTest
/// Show that if the receiver is `RaindexV7FlashBorrower` and the token
/// movements do not error, then the flash loan will succeed.
contract RaindexV7FlashLenderMockSuccessTest is RaindexV7ExternalMockTest {
    /// Tests that if the receiver is `RaindexV7FlashBorrower` and the token
    /// movements do not error, then the flash loan will succeed.
    function testFlashLoanToReceiver(uint256 amount, bytes memory data) public {
        // Return true for all transfers.
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));

        // A call to a contract that implements `IERC3156FlashBorrower` will
        // succeed if the return value is `ON_FLASH_LOAN_CALLBACK_SUCCESS`.
        address receiver = address(0xDEADBEEF);
        vm.etch(receiver, hex"FE");
        vm.mockCall(
            receiver,
            abi.encodeWithSelector(IERC3156FlashBorrower.onFlashLoan.selector),
            abi.encode(ON_FLASH_LOAN_CALLBACK_SUCCESS)
        );
        assertTrue(iRaindex.flashLoan(IERC3156FlashBorrower(receiver), address(iToken0), amount, data));
    }

    /// The `onFlashLoan` callback MUST receive exactly the loan initiator
    /// (`msg.sender`), the loaned token, the loaned amount, the flash fee (which
    /// is always 0 for Raindex), and the verbatim data. Pinning every argument
    /// catches mutations that swap the initiator, the token, the amount, or the
    /// fee passed to the borrower.
    function testFlashLoanCallbackArgs(address initiator, uint256 amount, bytes memory data) public {
        vm.assume(initiator != address(0));

        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));

        address receiver = address(0xDEADBEEF);
        vm.etch(receiver, hex"FE");
        vm.mockCall(
            receiver,
            abi.encodeWithSelector(IERC3156FlashBorrower.onFlashLoan.selector),
            abi.encode(ON_FLASH_LOAN_CALLBACK_SUCCESS)
        );

        // The fee MUST be 0 and the initiator MUST be the caller of `flashLoan`.
        vm.expectCall(
            receiver,
            abi.encodeWithSelector(
                IERC3156FlashBorrower.onFlashLoan.selector, initiator, address(iToken0), amount, uint256(0), data
            )
        );
        vm.prank(initiator);
        assertTrue(iRaindex.flashLoan(IERC3156FlashBorrower(receiver), address(iToken0), amount, data));
    }
}
