// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {
    IERC3156FlashBorrower,
    ON_FLASH_LOAN_CALLBACK_SUCCESS
} from "raindex-interface-0.1.17/src/interface/ierc3156/IERC3156FlashBorrower.sol";
import {IPull} from "test/util/concrete/IPull.sol";

/// Alice has some daisy contract pull tokens from her.
/// If the tokens are returned to her then she can complete the flash loan else
/// the loan must be reverted.
contract Alice is IERC3156FlashBorrower {
    IPull immutable iPull;
    bool immutable iSuccess;

    constructor(IPull pull, bool success) {
        iPull = pull;
        iSuccess = success;
    }

    function onFlashLoan(address, address token, uint256 amount, uint256, bytes calldata)
        public
        override
        returns (bytes32)
    {
        // Approve the puller to pull the tokens.
        IERC20(token).approve(address(iPull), amount);
        iPull.pull(token, amount);
        // Approve the lender to pull the tokens back and repay the loan.
        IERC20(token).approve(msg.sender, amount);
        // Magic number for success.
        return iSuccess ? ON_FLASH_LOAN_CALLBACK_SUCCESS : bytes32(0);
    }
}
