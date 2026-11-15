// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RainDeployVerifySnapshot} from "rain-deploy-0.1.32/src/abstract/RainDeployVerifySnapshot.sol";
import {RaindexDeploySuites} from "../../src/abstract/RaindexDeploySuites.sol";

/// @title RaindexDeploySnapshotTest
/// @notice Binds this repo's declaration to `RainDeployVerifySnapshot`: every
/// deploy-pin assertion over the raindex suites that needs no network.
contract RaindexDeploySnapshotTest is RaindexDeploySuites, RainDeployVerifySnapshot {}
