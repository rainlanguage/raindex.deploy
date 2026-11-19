// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ArbOrderTaker, Float} from "../../abstract/RaindexV7ArbOrderTaker.sol";
import {LibGenericPoolExchange} from "../../lib/LibGenericPoolExchange.sol";

/// @title GenericPoolRaindexV7ArbOrderTaker
/// @notice Order-taker arb that swaps via an arbitrary external pool call.
/// The `takeOrdersData` is decoded as `(spender, pool, encodedFunctionCall)`.
contract GenericPoolRaindexV7ArbOrderTaker is RaindexV7ArbOrderTaker {
    constructor() {}

    /// @inheritdoc RaindexV7ArbOrderTaker
    /// @dev Decodes `takeOrdersData` as `(spender, pool, encodedFunctionCall)`
    /// and routes the swap through the specified pool via `LibGenericPoolExchange`.
    function onTakeOrders2(
        address inputToken,
        address outputToken,
        Float inputAmountSent,
        Float totalOutputAmount,
        bytes calldata takeOrdersData
    ) public virtual override {
        super.onTakeOrders2(inputToken, outputToken, inputAmountSent, totalOutputAmount, takeOrdersData);
        LibGenericPoolExchange.exchange(inputToken, takeOrdersData);
    }

    /// Allow arbitrary calls and ETH transfers to this contract without
    /// reverting. Any ETH still held when `finalizeArb` runs is swept to
    /// `msg.sender`. The arb holds no ETH between operations, so anyone who
    /// sends ETH outside an `arb` call forfeits it: an in-flight generic pool
    /// exchange forwards the contract's entire ETH balance to the pool before
    /// `finalizeArb` is reached.
    receive() external payable {}
    fallback() external payable {}
}
