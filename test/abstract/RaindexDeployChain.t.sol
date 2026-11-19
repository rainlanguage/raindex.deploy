// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RainDeployVerifyChain} from "rain-deploy-0.1.36/src/abstract/RainDeployVerifyChain.sol";
import {RaindexDeploySuites} from "../../src/abstract/RaindexDeploySuites.sol";

/// @title RaindexDeployChainTest
/// @notice Binds this repo's declaration to `RainDeployVerifyChain`: every
/// released suite is live, with the code it froze, on every supported network.
contract RaindexDeployChainTest is RaindexDeploySuites, RainDeployVerifyChain {}
