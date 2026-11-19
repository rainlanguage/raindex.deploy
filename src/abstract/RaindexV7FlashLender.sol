// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.19;

import {ERC165, IERC165} from "@openzeppelin-contracts-5.7.0/utils/introspection/ERC165.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";

import {
    IERC3156FlashBorrower,
    ON_FLASH_LOAN_CALLBACK_SUCCESS
} from "raindex-interface-0.1.16/src/interface/ierc3156/IERC3156FlashBorrower.sol";
import {IERC3156FlashLender} from "raindex-interface-0.1.16/src/interface/ierc3156/IERC3156FlashLender.sol";

/// Thrown when the `onFlashLoan` callback returns anything other than
/// ON_FLASH_LOAN_CALLBACK_SUCCESS.
/// @param result The value that was returned by `onFlashLoan`.
error FlashLenderCallbackFailed(bytes32 result);

/// Thrown by `flashFee` for a token Raindex cannot lend (it holds none, i.e.
/// `maxFlashLoan(token) == 0`), as ERC-3156 requires `flashFee` to revert for
/// unsupported tokens.
/// @param token The unsupported token.
error FlashLenderUnsupportedToken(address token);

/// @dev Flash fee is always 0 for raindex as there's no entity to take
/// revenue for `Raindex` and its more important anyway that flashloans happen
/// to connect external liquidity to live orders via arbitrage.
uint256 constant FLASH_FEE = 0;

/// @title RaindexV7FlashLender
/// @notice Implements `IERC3156FlashLender` for `Raindex`. Based on the
/// reference implementation by Alberto Cuesta Cañada found at
/// https://eips.ethereum.org/EIPS/eip-3156#flash-loan-reference-implementation
abstract contract RaindexV7FlashLender is IERC3156FlashLender, ERC165 {
    using SafeERC20 for IERC20;

    /// @inheritdoc IERC165
    function supportsInterface(bytes4 interfaceId) public view virtual override returns (bool) {
        return interfaceId == type(IERC3156FlashLender).interfaceId || super.supportsInterface(interfaceId);
    }

    /// @inheritdoc IERC3156FlashLender
    function flashLoan(IERC3156FlashBorrower receiver, address token, uint256 amount, bytes calldata data)
        external
        override
        returns (bool)
    {
        IERC20(token).safeTransfer(address(receiver), amount);

        bytes32 result = receiver.onFlashLoan(msg.sender, token, amount, FLASH_FEE, data);
        if (result != ON_FLASH_LOAN_CALLBACK_SUCCESS) {
            revert FlashLenderCallbackFailed(result);
        }

        // This behaviour is copied almost verbatim from the ERC3156 spec.
        // Slither is complaining because this kind of logic can normally be used
        // to grief the token holder. Consider if alice were to approve raindex
        // for the sake of depositing and then bob could cause alice to send
        // tokens to raindex without their consent. However, in this case the
        // flash loan spec provides two reasons that this is not a problem:
        // - We just sent this exact amount to the receiver as the loan, so
        // transferring them back with a 0 fee is net neutral.
        // - The receiver is a contract that has explicitly opted in to this
        // behaviour by implementing `IERC3156FlashBorrower`. The success check
        // for `onFlashLoan` guarantees the receiver has opted into this
        // behaviour independently of any approvals, etc.
        // https://github.com/crytic/slither/issues/1658
        //slither-disable-next-line arbitrary-send-erc20
        IERC20(token).safeTransferFrom(address(receiver), address(this), amount + FLASH_FEE);

        return true;
    }

    /// @inheritdoc IERC3156FlashLender
    /// @dev Reverts for an unsupported token (one Raindex holds no balance of,
    /// i.e. `maxFlashLoan(token) == 0`) as ERC-3156 requires, rather than
    /// returning a fee for a loan that could never settle.
    function flashFee(address token, uint256) external view override returns (uint256) {
        // Strict equality is intentional: a token Raindex holds zero balance of
        // is by definition unsupported. Any nonzero balance means supported.
        //slither-disable-next-line incorrect-equality
        if (IERC20(token).balanceOf(address(this)) == 0) {
            revert FlashLenderUnsupportedToken(token);
        }
        return FLASH_FEE;
    }

    /// There's no limit to the size of a flash loan from `Raindex` other than
    /// the current tokens deposited in `Raindex`.
    /// @inheritdoc IERC3156FlashLender
    function maxFlashLoan(address token) external view override returns (uint256) {
        return IERC20(token).balanceOf(address(this));
    }
}
