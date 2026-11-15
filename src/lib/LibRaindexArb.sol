// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.19;

import {TaskV3} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {LibRaindex} from "./LibRaindex.sol";
import {Address} from "@openzeppelin-contracts-5.7.0/utils/Address.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {LibDecimalFloat, Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// @title LibRaindexArb
library LibRaindexArb {
    using SafeERC20 for IERC20;

    /// @dev Sweeps the orders' input and output token balances and any native
    /// gas held by this contract to `msg.sender`, then evaluates the post-arb
    /// task with a context column containing those balances as Floats. Only the
    /// two order tokens are swept; any other token left on the arb stays put.
    /// The Float columns are the balances read immediately before each transfer,
    /// so for fee-on-transfer tokens they can exceed the amount actually
    /// received by `msg.sender`.
    function finalizeArb(
        TaskV3 memory task,
        address ordersInputToken,
        uint8 inputDecimals,
        address ordersOutputToken,
        uint8 outputDecimals
    ) internal {
        bytes32[][] memory context = new bytes32[][](1);
        bytes32[] memory col = new bytes32[](3);

        {
            // Send all unspent input tokens to the sender.
            uint256 inputBalance = IERC20(ordersInputToken).balanceOf(address(this));
            if (inputBalance > 0) {
                IERC20(ordersInputToken).safeTransfer(msg.sender, inputBalance);
            }
            //slither-disable-next-line unused-return
            (Float input,) = LibDecimalFloat.fromFixedDecimalLossyPacked(inputBalance, inputDecimals);
            col[0] = Float.unwrap(input);
        }

        {
            // Send all unspent output tokens to the sender.
            uint256 outputBalance = IERC20(ordersOutputToken).balanceOf(address(this));
            if (outputBalance > 0) {
                IERC20(ordersOutputToken).safeTransfer(msg.sender, outputBalance);
            }

            //slither-disable-next-line unused-return
            (Float output,) = LibDecimalFloat.fromFixedDecimalLossyPacked(outputBalance, outputDecimals);
            col[1] = Float.unwrap(output);
        }

        {
            // Send any remaining gas to the sender.
            // Slither false positive here. We want to send everything to the sender
            // because this contract should be empty of all gas and tokens between
            // uses. Anyone who sends tokens or gas to an arb contract without
            // calling `arb` is going to lose their tokens/gas.
            // See https://github.com/crytic/slither/issues/1658
            uint256 gasBalance = address(this).balance;
            if (gasBalance > 0) {
                Address.sendValue(payable(msg.sender), gasBalance);
            }
            // Native gas is 18 decimals; lossy-packed like the input/output
            // columns, so an extreme balance saturates precision rather than
            // overflowing the Float coefficient.
            //slither-disable-next-line unused-return
            (Float gasFloat,) = LibDecimalFloat.fromFixedDecimalLossyPacked(gasBalance, 18);
            col[2] = Float.unwrap(gasFloat);
        }

        context[0] = col;

        TaskV3[] memory post = new TaskV3[](1);
        post[0] = task;
        LibRaindex.doPost(context, post);
    }
}
