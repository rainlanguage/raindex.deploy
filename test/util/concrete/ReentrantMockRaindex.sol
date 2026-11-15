// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {GenericPoolRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol";
import {
    IRaindexV7,
    TakeOrdersConfigV6,
    TaskV3,
    EvaluableV4,
    SignedContextV2,
    Float
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.35/src/interface/IInterpreterStoreV3.sol";
import {MockRaindexBase} from "test/util/abstract/MockRaindexBase.sol";

/// @dev Mock Raindex whose takeOrders5 callback re-enters arb6 on the taker.
contract ReentrantMockRaindex is MockRaindexBase {
    using SafeERC20 for IERC20;

    bool internal sReentered;

    function takeOrders5(TakeOrdersConfigV6 calldata config) external override returns (Float, Float) {
        address ordersOutputToken = config.orders[0].order.validOutputs[config.orders[0].outputIOIndex].token;

        // Send output token to taker so callback has tokens to work with.
        uint256 bal = IERC20(ordersOutputToken).balanceOf(address(this));
        IERC20(ordersOutputToken).safeTransfer(msg.sender, bal);

        if (!sReentered) {
            sReentered = true;
            // Re-enter arb6 from the Raindex callback.
            GenericPoolRaindexV7ArbOrderTaker(payable(msg.sender))
                .arb6(
                    IRaindexV7(address(this)),
                    config,
                    TaskV3({
                    evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
                    signedContext: new SignedContextV2[](0)
                })
                );
        }

        return (Float.wrap(0), Float.wrap(0));
    }
}
