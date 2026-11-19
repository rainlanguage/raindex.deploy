// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {GenericPoolRaindexV7FlashBorrower} from "../../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";

/// @dev Malicious lender that calls onFlashLoan directly, pretending to be a
/// flash loan provider while passing the arb's own address as initiator.
contract MaliciousLender {
    using SafeERC20 for IERC20;

    function attack(GenericPoolRaindexV7FlashBorrower arb, address token, uint256 amount, bytes calldata data)
        external
    {
        IERC20(token).safeTransfer(address(arb), amount);
        arb.onFlashLoan(address(arb), token, amount, 0, data);
        uint256 balance = IERC20(token).balanceOf(address(arb));
        if (balance > 0) {
            IERC20(token).safeTransferFrom(address(arb), address(this), balance);
        }
    }
}
