// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Address} from "@openzeppelin-contracts-5.7.0/utils/Address.sol";
import {
    IERC3156FlashBorrower,
    ON_FLASH_LOAN_CALLBACK_SUCCESS
} from "raindex-interface-0.1.16/src/interface/ierc3156/IERC3156FlashBorrower.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

/// @title Reenteroor
/// A contract that reenters the caller with a configurable call.
/// This is compatible with flash loans and can be used as a borrower that will
/// handle `onFlashLoan` and return success to the lender correctly, as well as
/// reentering.
contract Reenteroor is IERC3156FlashBorrower {
    using Address for address;

    /// The call to reenter with. Set by `reenterWith`.
    bytes internal _sEncodedCall;

    /// Set the call to reenter with. The encoding will be used by the fallback
    /// to call back into the caller.
    function reenterWith(bytes memory encodedCall) external {
        _sEncodedCall = encodedCall;
    }

    /// @inheritdoc IERC3156FlashBorrower
    function onFlashLoan(address, address token, uint256 amount, uint256, bytes calldata)
        external
        override
        returns (bytes32)
    {
        address(msg.sender).functionCall(_sEncodedCall);
        // Approve the lender to pull the tokens back and repay the loan.
        IERC20(token).approve(msg.sender, amount);
        return ON_FLASH_LOAN_CALLBACK_SUCCESS;
    }

    /// Reenter the caller with the call set by `reenterWith`. This will bubble
    /// up any reverts from the reentrant call so tests can check that
    /// reentrancy guards are working.
    fallback() external {
        address(msg.sender).functionCall(_sEncodedCall);
    }
}
