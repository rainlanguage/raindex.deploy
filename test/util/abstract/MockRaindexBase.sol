// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {
    IRaindexV7,
    TakeOrdersConfigV6,
    OrderConfigV4,
    ClearConfigV2,
    OrderV4,
    SignedContextV2,
    TaskV3,
    QuoteV3,
    Float
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IERC3156FlashBorrower} from "raindex-interface-0.1.17/src/interface/ierc3156/IERC3156FlashBorrower.sol";

/// @dev Base for mock raindex contracts that stub all IRaindexV7 functions.
/// Inheritors override flashLoan and/or takeOrders5 with real token transfers.
abstract contract MockRaindexBase is IRaindexV7 {
    using SafeERC20 for IERC20;

    function flashLoan(IERC3156FlashBorrower, address, uint256, bytes calldata) external virtual returns (bool) {
        return false;
    }

    function takeOrders5(TakeOrdersConfigV6 calldata) external virtual returns (Float, Float) {
        return (Float.wrap(0), Float.wrap(0));
    }

    function entask3(TaskV3[] calldata) external pure {}

    function quote3(QuoteV3 calldata) external pure returns (bool, Float, Float) {
        revert("quote");
    }

    function addOrder5(OrderConfigV4 calldata, TaskV3[] calldata) external pure returns (bool) {
        return false;
    }

    function orderExists(bytes32) external pure returns (bool) {
        return false;
    }

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
