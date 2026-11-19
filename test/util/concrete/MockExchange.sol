// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";

contract MockExchange {
    using SafeERC20 for IERC20;

    function swap(IERC20 tokenIn, IERC20 tokenOut, uint256 amountIn) external payable {
        tokenIn.safeTransferFrom(msg.sender, address(this), amountIn);
        tokenOut.safeTransfer(msg.sender, amountIn);
        // Return any ETH received back to the caller so it can be swept by
        // finalizeArb later.
        if (msg.value > 0) {
            payable(msg.sender).transfer(msg.value);
        }
    }
}
