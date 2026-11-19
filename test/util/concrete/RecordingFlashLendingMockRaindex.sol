// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {TakeOrdersConfigV6, Float} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";
import {
    IERC3156FlashBorrower,
    ON_FLASH_LOAN_CALLBACK_SUCCESS
} from "raindex-interface-0.1.14/src/interface/ierc3156/IERC3156FlashBorrower.sol";
import {MockRaindexBase} from "test/util/abstract/MockRaindexBase.sol";

/// @dev Mock raindex identical to `RealisticFlashLendingMockRaindex` (real
/// ERC3156 transfers, real `takeOrders5` swap) but additionally records the
/// `token` and `amount` it was asked to flash loan so a test can pin the
/// exact base-unit amount the arb requested.
contract RecordingFlashLendingMockRaindex is MockRaindexBase {
    using SafeERC20 for IERC20;

    address public lastFlashLoanToken;
    uint256 public lastFlashLoanAmount;

    // Records and transfers whatever token the arb requested, including address(0).
    // forge-lint: disable-next-line(missing-zero-check)
    function flashLoan(IERC3156FlashBorrower receiver, address token, uint256 amount, bytes calldata data)
        external
        override
        returns (bool)
    {
        lastFlashLoanToken = token;
        lastFlashLoanAmount = amount;

        IERC20(token).safeTransfer(address(receiver), amount);

        bytes32 result = receiver.onFlashLoan(msg.sender, token, amount, 0, data);
        require(result == ON_FLASH_LOAN_CALLBACK_SUCCESS, "callback failed");

        IERC20(token).safeTransferFrom(address(receiver), address(this), amount);

        return true;
    }

    function takeOrders5(TakeOrdersConfigV6 calldata config) external override returns (Float, Float) {
        address inputToken = config.orders[0].order.validInputs[config.orders[0].inputIOIndex].token;
        address outputToken = config.orders[0].order.validOutputs[config.orders[0].outputIOIndex].token;
        uint256 inputBalance = IERC20(inputToken).balanceOf(msg.sender);
        IERC20(inputToken).safeTransferFrom(msg.sender, address(this), inputBalance);
        IERC20(outputToken).safeTransfer(msg.sender, inputBalance);
        return (Float.wrap(0), Float.wrap(0));
    }
}
