// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {
    IERC3156FlashBorrower,
    ON_FLASH_LOAN_CALLBACK_SUCCESS
} from "raindex-interface-0.1.16/src/interface/ierc3156/IERC3156FlashBorrower.sol";
import {MockRaindexBase} from "test/util/abstract/MockRaindexBase.sol";

/// @dev Mock raindex whose `flashLoan` performs a real net-neutral ERC3156
/// transfer of the loaned token (out then back), and whose `takeOrders5` is a
/// no-op (the inherited base does nothing). This leaves whatever token balances
/// the arb already holds completely untouched, so a test can pre-fund the arb
/// with a deterministic leftover of each token and assert exactly how
/// `finalizeArb` scales those leftovers into the post-arb task context column.
contract LeftoverFlashLendingMockRaindex is MockRaindexBase {
    using SafeERC20 for IERC20;

    function flashLoan(IERC3156FlashBorrower receiver, address token, uint256 amount, bytes calldata data)
        external
        override
        returns (bool)
    {
        IERC20(token).safeTransfer(address(receiver), amount);
        bytes32 result = receiver.onFlashLoan(msg.sender, token, amount, 0, data);
        require(result == ON_FLASH_LOAN_CALLBACK_SUCCESS, "callback failed");
        IERC20(token).safeTransferFrom(address(receiver), address(this), amount);
        return true;
    }
}
