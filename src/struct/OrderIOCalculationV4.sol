// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {OrderV4, Float} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";
import {StateNamespace} from "rainlang-0.2.36/src/interface/IInterpreterV4.sol";

/// All information resulting from an order calculation that allows for vault IO
/// to be calculated and applied, then the handle IO entrypoint to be dispatched.
/// @param outputMax The maximum output calculated by the order expression as a
/// Float. WILL BE RESCALED ACCORDING TO TOKEN DECIMALS (read via TOFU) for the
/// purpose of calculating actual vault movements.
/// The output max is CAPPED AT THE OUTPUT VAULT BALANCE OF THE ORDER OWNER.
/// The order is guaranteed that the total output of this single clearance cannot
/// exceed this (subject to rescaling). It is up to the order expression to track
/// values over time if the output max is to impose a global limit across many
/// transactions and counterparties.
/// @param IORatio The order ratio as input/output from the perspective of the
/// order, as a Float. As each counterparty's input is the other's output, the
/// IORatio calculated by each order is inverse of its counterparty. IORatio is
/// RESCALED ACCORDING TO TOKEN DECIMALS (read via TOFU) when applied to vault
/// balances. I.e. a ratio of `1` is ALWAYS "one" as ECONOMIC EQUIVALENCE
/// between two tokens, but this will be rescaled according to the decimals of
/// the token. For example, if DAI and USDT have a ratio of `1` then in reality
/// `1e12` DAI will move in the vault for every `1` USDT that moves, because DAI
/// has `1e18` decimals per $1 peg and USDT has `1e6` decimals per $1 peg.
/// @param context The entire 2D context array, initialized from the context
/// passed into the order calculations and then populated with the order
/// calculations and vault IO before being passed back to handle IO entrypoint.
/// @param namespace The `StateNamespace` to be passed to the store for calculate
/// IO state changes.
/// @param kvs KVs returned from calculate order entrypoint to pass to the store
/// before calling handle IO entrypoint.
struct OrderIOCalculationV4 {
    OrderV4 order;
    uint256 outputIOIndex;
    Float outputMax;
    //solhint-disable-next-line var-name-mixedcase
    Float IORatio;
    bytes32[][] context;
    StateNamespace namespace;
    bytes32[] kvs;
}
