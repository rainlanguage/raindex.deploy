// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RainDeployVerifyDeclaration} from "rain-deploy-0.1.33/src/abstract/RainDeployVerifyDeclaration.sol";
import {RaindexDeploySuites} from "../../src/abstract/RaindexDeploySuites.sol";

contract RaindexDeployDeclarationTest is RaindexDeploySuites, RainDeployVerifyDeclaration {}
