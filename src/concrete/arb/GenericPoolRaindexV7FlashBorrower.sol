// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7FlashBorrower, TakeOrdersConfigV6} from "../../abstract/RaindexV7FlashBorrower.sol";
import {LibGenericPoolExchange} from "../../lib/LibGenericPoolExchange.sol";

/// @title GenericPoolRaindexV7FlashBorrower
/// @notice Flash-loan arb that swaps via an arbitrary external pool call.
/// @dev Implements the RaindexV7FlashBorrower interface for an external
/// liquidity source that behaves vaguely like a standard AMM. The
/// `exchangeData` from `arb` is decoded into a spender, pool and callData.
/// The `callData` is literally the encoded function call to the pool. This
/// allows the `arb` caller to process a trade against any liquidity source
/// that can swap tokens within a single function call.
/// The `spender` is the address that will be approved to spend the input token
/// on `takeOrders`, which is almost always going to be the pool itself. If you
/// are unsure, simply set it to the pool address.
contract GenericPoolRaindexV7FlashBorrower is RaindexV7FlashBorrower {
    constructor() {}

    /// @inheritdoc RaindexV7FlashBorrower
    /// @dev Decodes `exchangeData` as `(spender, pool, encodedFunctionCall)`
    /// and routes the swap through the specified pool via `LibGenericPoolExchange`.
    function _exchange(TakeOrdersConfigV6 memory takeOrders, bytes memory exchangeData) internal virtual override {
        address borrowedToken = takeOrders.orders[0].order.validOutputs[takeOrders.orders[0].outputIOIndex].token;
        LibGenericPoolExchange.exchange(borrowedToken, exchangeData);
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
