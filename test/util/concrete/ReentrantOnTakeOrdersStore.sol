// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {GenericPoolRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol";
import {StateNamespace} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {MockToken} from "./MockToken.sol";
import {Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// Store that, on `set`, calls the arb's `onTakeOrders2` naming itself as
/// spender and pool, and records the revert data.
contract ReentrantOnTakeOrdersStore {
    GenericPoolRaindexV7ArbOrderTaker internal immutable arb;
    MockToken internal immutable token;
    bytes public revertData;

    constructor(GenericPoolRaindexV7ArbOrderTaker arb_, MockToken token_) {
        arb = arb_;
        token = token_;
    }

    function set(StateNamespace, bytes32[] calldata) external {
        try arb.onTakeOrders2(
            address(token),
            address(token),
            Float.wrap(0),
            Float.wrap(0),
            abi.encode(address(this), address(this), bytes(""))
        ) {}
        catch (bytes memory err) {
            revertData = err;
        }
    }

    function get(bytes32, bytes32) external pure returns (bytes32) {
        return 0;
    }

    fallback() external payable {}
    receive() external payable {}
}
