// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// This will exist in a future version of Open Zeppelin if their main branch is
/// to be believed.
error ReentrancyGuardReentrantCall();

/// Thrown when the `msg.sender` modifying an order is not its owner.
/// @param owner The owner of the order.
error NotOrderOwner(address owner);

/// Thrown when the input and output tokens don't match, in either direction.
error TokenMismatch();

/// Thrown when the minimum input is not met.
/// @param minimumIO The minimum io required.
/// @param actualIO The io that was achieved.
error MinimumIO(Float minimumIO, Float actualIO);

/// Thrown when two orders have the same owner during clear.
error SameOwner();

/// Thrown when calculate order expression offers too few outputs.
/// @param outputs The outputs the expression offers.
error UnsupportedCalculateOutputs(uint256 outputs);

/// Thrown when a negative vault balance is being recorded.
/// @param vaultBalance The negative vault balance being recorded.
error NegativeVaultBalance(Float vaultBalance);

/// Thrown when a negative amount is being applied to a vault balance.
/// @param amount The negative amount being applied.
error NegativeVaultBalanceChange(Float amount);

/// Thrown when a negative pull is attempted.
error NegativePull();

/// Thrown when a negative push is attempted.
error NegativePush();

/// Throws when a negative bounty is calculated.
error NegativeBounty();

/// Thrown when either side of a clear has a non-positive output or a negative
/// IO ratio.
error ClearZeroAmount();
