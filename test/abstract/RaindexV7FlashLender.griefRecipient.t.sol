// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalMockTest} from "test/util/abstract/RaindexV7ExternalMockTest.sol";
import {
    IERC3156FlashBorrower,
    ON_FLASH_LOAN_CALLBACK_SUCCESS
} from "raindex-interface-0.1.17/src/interface/ierc3156/IERC3156FlashBorrower.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {FlashLenderCallbackFailed} from "../../src/abstract/RaindexV7FlashLender.sol";

/// @title RaindexV7FlashLenderGriefRecipientTest
/// Try to grief the recipient of the flash loan.
contract RaindexV7FlashLenderGriefRecipientTest is RaindexV7ExternalMockTest {
    /// Tests that no matter who the receiver is, and no matter what happens with
    /// the tokens, the flash loan will revert if the receiver is not
    /// `RaindexV7FlashBorrower`.
    function testFlashLoanToNonReceiver(
        uint256 amount,
        bytes memory data,
        bytes32 notFlashLoanSuccess,
        bytes memory notFlashLoanSuccessBytes
    ) public {
        vm.assume(notFlashLoanSuccess != ON_FLASH_LOAN_CALLBACK_SUCCESS);
        vm.assume(keccak256(notFlashLoanSuccessBytes) != keccak256(abi.encode(ON_FLASH_LOAN_CALLBACK_SUCCESS)));

        // Return true for all transfers.
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));

        // A call to an EOA will revert with no data.
        address receiver = address(0xDEADBEEF);
        vm.expectRevert();
        iRaindex.flashLoan(IERC3156FlashBorrower(receiver), address(iToken0), amount, data);

        // A call to a contract that does not implement `IERC3156FlashBorrower`
        // will revert with `FlashLenderCallbackFailed`.
        vm.etch(receiver, hex"FE");
        vm.mockCall(
            receiver,
            abi.encodeWithSelector(IERC3156FlashBorrower.onFlashLoan.selector),
            abi.encode(notFlashLoanSuccess)
        );
        vm.expectRevert(abi.encodeWithSelector(FlashLenderCallbackFailed.selector, notFlashLoanSuccess));
        iRaindex.flashLoan(IERC3156FlashBorrower(receiver), address(iToken0), amount, data);

        // A call to a contract that does not implement `IERC3156FlashBorrower`
        // will revert with no data if the return value is not `bytes32`.
        vm.mockCall(
            receiver, abi.encodeWithSelector(IERC3156FlashBorrower.onFlashLoan.selector), notFlashLoanSuccessBytes
        );
        vm.expectRevert();
        iRaindex.flashLoan(IERC3156FlashBorrower(receiver), address(iToken0), amount, data);
    }
}
