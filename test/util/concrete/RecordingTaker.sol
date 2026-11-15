// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {TakeOrdersConfigV6, IRaindexV7} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IRaindexV7OrderTaker} from "raindex-interface-0.1.13/src/interface/IRaindexV7OrderTaker.sol";
import {Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// Records onTakeOrders2 invocation + the args it was called with so a test can
/// assert the callback fires only when something was taken and with the right
/// token/amount arguments.
contract RecordingTaker is IRaindexV7OrderTaker {
    uint256 public calls;
    address public lastOutputToken;
    address public lastInputToken;
    Float public lastTakerInput;
    Float public lastTakerOutput;
    bytes public lastData;
    IRaindexV7 internal immutable iRaindex;

    constructor(IRaindexV7 raindex) {
        iRaindex = raindex;
    }

    function take(TakeOrdersConfigV6 memory config) external returns (Float, Float) {
        return iRaindex.takeOrders5(config);
    }

    function onTakeOrders2(
        // Records the token it was given, including address(0).
        // forge-lint: disable-next-line(missing-zero-check)
        address outputToken,
        // Records the token it was given, including address(0).
        // forge-lint: disable-next-line(missing-zero-check)
        address inputToken,
        Float takerInput,
        Float takerOutput,
        bytes calldata data
    ) external {
        calls++;
        lastOutputToken = outputToken;
        lastInputToken = inputToken;
        lastTakerInput = takerInput;
        lastTakerOutput = takerOutput;
        lastData = data;
    }
}
