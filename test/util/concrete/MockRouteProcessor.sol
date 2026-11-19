// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {IRouteProcessor} from "src/interface/IRouteProcessor.sol";

/// @dev Mock route processor that pulls inputToken from sender and sends
/// outputToken to the recipient. The route bytes are ignored.
contract MockRouteProcessor is IRouteProcessor {
    using SafeERC20 for IERC20;

    function processRoute(address tokenIn, uint256 amountIn, address tokenOut, uint256, address to, bytes memory)
        external
        payable
        returns (uint256 amountOut)
    {
        IERC20(tokenIn).safeTransferFrom(msg.sender, address(this), amountIn);
        amountOut = IERC20(tokenOut).balanceOf(address(this));
        IERC20(tokenOut).safeTransfer(to, amountOut);
    }
}
