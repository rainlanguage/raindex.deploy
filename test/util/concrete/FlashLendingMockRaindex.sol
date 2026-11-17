// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {
    TakeOrderConfigV5,
    IOV2,
    OrderV4,
    SignedContextV2,
    IRaindexV7,
    TakeOrdersConfigV6,
    OrderConfigV4,
    ClearConfigV2,
    EvaluableV4,
    TaskV3,
    QuoteV3,
    Float
} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";
import {IERC3156FlashBorrower} from "raindex-interface-0.1.14/src/interface/ierc3156/IERC3156FlashBorrower.sol";

contract FlashLendingMockRaindex is IRaindexV7 {
    function flashLoan(IERC3156FlashBorrower receiver, address token, uint256 amount, bytes calldata data)
        external
        returns (bool)
    {
        receiver.onFlashLoan(msg.sender, token, amount, 0, data);
        return true;
    }

    function entask3(TaskV3[] calldata) external pure {}

    /// @inheritdoc IRaindexV7
    function quote3(QuoteV3 calldata) external pure returns (bool, Float, Float) {
        revert("quote");
    }

    /// @inheritdoc IRaindexV7
    function takeOrders5(TakeOrdersConfigV6 calldata) external pure returns (Float, Float) {}

    /// @inheritdoc IRaindexV7
    function addOrder5(OrderConfigV4 calldata, TaskV3[] calldata) external pure returns (bool) {
        return false;
    }

    function orderExists(bytes32) external pure returns (bool) {
        return false;
    }

    /// @inheritdoc IRaindexV7
    function clear4(
        OrderV4 memory,
        OrderV4 memory,
        ClearConfigV2 calldata,
        SignedContextV2[] memory,
        SignedContextV2[] memory
    ) external {}
    function deposit5(address, bytes32, Float, TaskV3[] calldata) external {}
    function flashFee(address, uint256) external view returns (uint256) {}
    function maxFlashLoan(address) external view returns (uint256) {}
    function removeOrder4(OrderV4 calldata, TaskV3[] calldata) external returns (bool) {}

    function vaultBalance2(address, address, bytes32) external view returns (Float) {}
    function withdraw5(address, bytes32, Float, TaskV3[] calldata) external {}
}
