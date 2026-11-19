// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

/// @dev Invoked as the `pool`; records, at call time, the caller and the
/// caller's token allowance to both `spender` and the pool itself — so a test
/// can prove the approval went to `spender` (not the pool) and the call routed
/// here.
contract AllowanceProbePool {
    address public caller;
    uint256 public spenderAllowance;
    uint256 public poolAllowance;

    function probe(IERC20 token, address arb, address spender) external payable {
        caller = msg.sender;
        spenderAllowance = token.allowance(arb, spender);
        poolAllowance = token.allowance(arb, address(this));
    }
}
