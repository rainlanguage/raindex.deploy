// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// The input and output tokens shared by every order in a `takeOrders5` batch,
/// and the taker IO still remaining to fill as the batch is processed.
/// @param inputToken The input token every order in the batch must use.
/// @param outputToken The output token every order in the batch must use.
/// @param remainingTakerIO The taker IO left to fill across the batch.
struct TakeOrdersIO {
    address inputToken;
    address outputToken;
    Float remainingTakerIO;
}
