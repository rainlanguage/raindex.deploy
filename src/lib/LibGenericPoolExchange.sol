// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.19;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {Address} from "@openzeppelin-contracts-5.7.0/utils/Address.sol";

/// @dev Thrown when the decoded spender is the zero address.
error ZeroExchangeSpender();

/// @dev Thrown when the decoded pool is the zero address.
error ZeroExchangePool();

/// @title LibGenericPoolExchange
/// @notice Shared approve-call-revoke pattern for generic pool exchanges.
library LibGenericPoolExchange {
    using SafeERC20 for IERC20;
    using Address for address;

    /// @notice Decodes `(spender, pool, encodedFunctionCall)` from `data`,
    /// approves `spender` to spend `token`, calls `pool` forwarding the
    /// contract's entire ETH balance, then revokes the approval.
    /// @param token The token to approve for the exchange.
    /// @param data ABI-encoded `(address spender, address pool, bytes encodedFunctionCall)`.
    function exchange(address token, bytes memory data) internal {
        (address spender, address pool, bytes memory encodedFunctionCall) = abi.decode(data, (address, address, bytes));

        // Fail loudly on a zero spender/pool rather than silently no-op'ing the
        // approval or making a value call to the zero address.
        if (spender == address(0)) {
            revert ZeroExchangeSpender();
        }
        if (pool == address(0)) {
            revert ZeroExchangePool();
        }

        // Approve-call-revoke: the caller controls spender and pool, which is
        // safe because the contract holds no tokens or ETH between arb
        // operations — there is nothing for a malicious caller to extract. Value
        // sent here by accident is recoverable through this permissionless path
        // rather than stranded; see `onTakeOrders2` on the base contract for why
        // unsolicited transfers cannot be rejected and gating would brick them.
        IERC20(token).forceApprove(spender, type(uint256).max);
        //slither-disable-next-line unused-return
        pool.functionCallWithValue(encodedFunctionCall, address(this).balance);
        IERC20(token).forceApprove(spender, 0);
    }
}
