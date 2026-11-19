// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {IPull} from "test/util/concrete/IPull.sol";

/// Carol pulls tokens from Alice then returns only some of them so the loan
/// fails.
contract Carol is IPull {
    using SafeERC20 for IERC20;

    uint256 immutable iAmountWithheld;

    constructor(uint256 amountWithheld) {
        iAmountWithheld = amountWithheld;
    }

    function pull(address token, uint256 amount) public override {
        IERC20(token).safeTransferFrom(msg.sender, address(this), amount);
        IERC20(token).safeTransfer(msg.sender, amount - iAmountWithheld);
    }
}
