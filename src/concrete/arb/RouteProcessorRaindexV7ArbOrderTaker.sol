// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IRouteProcessor} from "../../interface/IRouteProcessor.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";

import {RaindexV7ArbOrderTaker, Float} from "../../abstract/RaindexV7ArbOrderTaker.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../lib/deploy/LibRaindexDeploy.sol";

/// @title RouteProcessorRaindexV7ArbOrderTaker
/// @notice Order-taker arb that swaps via the deterministic Sushi
/// RouteProcessor4 deployment.
contract RouteProcessorRaindexV7ArbOrderTaker is RaindexV7ArbOrderTaker {
    using SafeERC20 for IERC20;

    constructor() {}

    /// @inheritdoc RaindexV7ArbOrderTaker
    function onTakeOrders2(
        address inputToken,
        address outputToken,
        Float inputAmountSent,
        Float totalOutputAmount,
        bytes calldata takeOrdersData
    ) public virtual override {
        super.onTakeOrders2(inputToken, outputToken, inputAmountSent, totalOutputAmount, takeOrdersData);
        address routeProcessor = LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS;
        IERC20(inputToken).forceApprove(routeProcessor, type(uint256).max);
        bytes memory route = abi.decode(takeOrdersData, (bytes));
        // Input amount precision loss is acceptable as the route processor
        // only needs an approximate amount to execute the swap.
        //slither-disable-next-line unused-return
        (uint256 inputTokenAmount,) =
            LibDecimalFloat.toFixedDecimalLossy(inputAmountSent, LibTOFUTokenDecimals.safeDecimalsForToken(inputToken));
        (uint256 outputTokenAmount, bool lossless) = LibDecimalFloat.toFixedDecimalLossy(
            totalOutputAmount, LibTOFUTokenDecimals.safeDecimalsForToken(outputToken)
        );
        if (!lossless) {
            outputTokenAmount++;
        }
        // `processRoute` is `payable`, but this arb calls it with no value:
        // native-ETH input legs are unsupported here, the swap is funded purely
        // by the ERC20 `inputToken` approval above. This deliberately diverges
        // from `LibGenericPoolExchange.exchange`, which forwards
        // `address(this).balance` to the caller-chosen pool. Any ETH held by
        // this contract is not routed into the swap; it is swept to `msg.sender`
        // by `finalizeArb`'s native-gas sweep, matching the recovery behaviour
        // documented at `receive()`/`fallback()` below and on the base
        // `RaindexV7ArbOrderTaker`.
        //slither-disable-next-line unused-return
        IRouteProcessor(routeProcessor)
            .processRoute(inputToken, inputTokenAmount, outputToken, outputTokenAmount, address(this), route);
        IERC20(inputToken).forceApprove(routeProcessor, 0);
    }

    /// Allow arbitrary calls and ETH transfers to this contract without
    /// reverting. Any ETH still held when `finalizeArb` runs is swept to
    /// `msg.sender`. The arb holds no ETH between operations, so anyone who
    /// sends ETH outside an `arb` call forfeits it.
    receive() external payable {}
    fallback() external payable {}
}
