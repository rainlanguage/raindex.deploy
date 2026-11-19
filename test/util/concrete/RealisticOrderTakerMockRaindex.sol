// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {TakeOrdersConfigV6, Float} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {IRaindexV7OrderTaker} from "raindex-interface-0.1.16/src/interface/IRaindexV7OrderTaker.sol";
import {MockRaindexBase} from "test/util/abstract/MockRaindexBase.sol";

/// @dev Mock raindex with real takeOrders5 transfers and onTakeOrders2
/// callback. Pulls a configurable amount of inputToken from the taker,
/// leaving any surplus as profit for finalizeArb to sweep.
contract RealisticOrderTakerMockRaindex is MockRaindexBase {
    using SafeERC20 for IERC20;

    uint256 public immutable iPullAmount;

    constructor(uint256 pullAmount) {
        iPullAmount = pullAmount;
    }

    function takeOrders5(TakeOrdersConfigV6 calldata config) external override returns (Float, Float) {
        address ordersInputToken = config.orders[0].order.validInputs[config.orders[0].inputIOIndex].token;
        address ordersOutputToken = config.orders[0].order.validOutputs[config.orders[0].outputIOIndex].token;

        uint256 outputAmount = IERC20(ordersOutputToken).balanceOf(address(this));

        // Send ordersOutputToken to taker (taker's input).
        IERC20(ordersOutputToken).safeTransfer(msg.sender, outputAmount);

        // Callback: taker swaps received tokens for the tokens the Raindex will pull.
        IRaindexV7OrderTaker(msg.sender)
            .onTakeOrders2(ordersOutputToken, ordersInputToken, Float.wrap(0), Float.wrap(0), config.data);

        // Pull iPullAmount of inputToken from taker.
        IERC20(ordersInputToken).safeTransferFrom(msg.sender, address(this), iPullAmount);

        return (Float.wrap(0), Float.wrap(0));
    }
}
