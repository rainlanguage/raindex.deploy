// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {IRaindexV7, OrderV4, SignedContextV2, Float} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {TOFUOutcome} from "rain-tofu-erc20-decimals-0.1.5/src/interface/ITOFUTokenDecimals.sol";
import {OrderIOCalculationV4} from "src/struct/OrderIOCalculationV4.sol";

interface IRaindexV7CalculateOrderIOHarness is IRaindexV7 {
    function exposedCalculateOrderIO(
        OrderV4 memory order,
        uint256 inputIOIndex,
        uint256 outputIOIndex,
        address counterparty,
        SignedContextV2[] memory signedContext
    ) external view returns (OrderIOCalculationV4 memory);

    function exposedCalculateOrderIOClearing(
        OrderV4 memory order,
        uint256 inputIOIndex,
        uint256 outputIOIndex,
        address counterparty,
        SignedContextV2[] memory signedContext
    ) external view returns (OrderIOCalculationV4 memory);

    function seedTofu(address token) external returns (TOFUOutcome, uint8);

    function exposedIncrease(address owner, address token, bytes32 vaultId, Float amount) external;
}
