// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {
    IRaindexV7,
    OrderConfigV4,
    OrderV4,
    ClearConfigV2,
    SignedContextV2,
    TakeOrdersConfigV6,
    EvaluableV4,
    TaskV3,
    QuoteV3,
    Float
} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {IERC3156FlashLender} from "raindex-interface-0.1.16/src/interface/ierc3156/IERC3156FlashLender.sol";
import {IERC3156FlashBorrower} from "raindex-interface-0.1.16/src/interface/ierc3156/IERC3156FlashBorrower.sol";

abstract contract IRaindexV7Stub is IRaindexV7 {
    /// @inheritdoc IRaindexV7
    function entask3(TaskV3[] calldata) external pure {
        revert("eval");
    }

    /// @inheritdoc IRaindexV7
    function quote3(QuoteV3 calldata) external pure returns (bool, Float, Float) {
        revert("quote");
    }

    /// @inheritdoc IRaindexV7
    function addOrder5(OrderConfigV4 calldata, TaskV3[] calldata) external pure returns (bool) {
        revert("addOrder");
    }

    /// @inheritdoc IRaindexV7
    function orderExists(bytes32) external pure returns (bool) {
        revert("orderExists");
    }

    /// @inheritdoc IRaindexV7
    function removeOrder4(OrderV4 calldata, TaskV3[] calldata) external pure returns (bool) {
        revert("removeOrder");
    }

    /// @inheritdoc IRaindexV7
    function clear4(
        OrderV4 memory,
        OrderV4 memory,
        ClearConfigV2 calldata,
        SignedContextV2[] memory,
        SignedContextV2[] memory
    ) external pure {
        revert("clear");
    }

    /// @inheritdoc IRaindexV7
    function deposit5(address, bytes32, Float, TaskV3[] calldata) external pure {
        revert("deposit");
    }

    /// @inheritdoc IRaindexV7
    function takeOrders5(TakeOrdersConfigV6 calldata) external pure returns (Float, Float) {
        revert("takeOrders");
    }

    /// @inheritdoc IRaindexV7
    function vaultBalance2(address, address, bytes32) external pure returns (Float) {
        revert("vaultBalance");
    }

    /// @inheritdoc IRaindexV7
    function withdraw5(address, bytes32, Float, TaskV3[] calldata) external pure {
        revert("withdraw");
    }

    /// @inheritdoc IERC3156FlashLender
    function flashLoan(IERC3156FlashBorrower, address, uint256, bytes calldata) external pure returns (bool) {
        revert("flashLoan");
    }

    /// @inheritdoc IERC3156FlashLender
    function flashFee(address, uint256) external pure returns (uint256) {
        revert("flashFee");
    }

    /// @inheritdoc IERC3156FlashLender
    function maxFlashLoan(address) external pure returns (uint256) {
        revert("maxFlashLoan");
    }
}
