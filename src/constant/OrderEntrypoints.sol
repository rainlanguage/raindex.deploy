// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {SourceIndexV2} from "rainlang-0.2.39/src/interface/IInterpreterV4.sol";

/// @dev Entrypoint to a calculate the amount and ratio of an order.
SourceIndexV2 constant CALCULATE_ORDER_ENTRYPOINT = SourceIndexV2.wrap(0);
/// @dev Entrypoint to handle the final internal vault movements resulting from
/// matching multiple calculated orders.
SourceIndexV2 constant HANDLE_IO_ENTRYPOINT = SourceIndexV2.wrap(1);

/// @dev Minimum outputs for calculate order are the amount and ratio.
uint256 constant CALCULATE_ORDER_MIN_OUTPUTS = 2;
