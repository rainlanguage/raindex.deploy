// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

/// @dev Exchange that always reverts.
contract RevertingExchange {
    function swap(IERC20, IERC20, uint256) external pure {
        revert("exchange failed");
    }
}
