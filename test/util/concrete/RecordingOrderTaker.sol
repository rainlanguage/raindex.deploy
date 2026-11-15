// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {TakeOrdersConfigV6, IRaindexV7} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IRaindexV7OrderTaker} from "raindex-interface-0.1.13/src/interface/IRaindexV7OrderTaker.sol";
import {Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// Records whether `onTakeOrders2` was invoked, so a test can assert the
/// callback fires only when at least one order was actually taken.
contract RecordingOrderTaker is IRaindexV7OrderTaker {
    bool public called;
    IRaindexV7 internal immutable iRaindex;

    constructor(IRaindexV7 raindex) {
        iRaindex = raindex;
    }

    function take(TakeOrdersConfigV6 memory config) external returns (Float, Float) {
        return iRaindex.takeOrders5(config);
    }

    function onTakeOrders2(address, address, Float, Float, bytes calldata) external {
        called = true;
    }
}
