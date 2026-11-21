// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RainDeployBroadcast} from "rain-deploy-0.1.38/src/abstract/RainDeployBroadcast.sol";
import {RaindexDeploySuites} from "../src/abstract/RaindexDeploySuites.sol";

/// @title Deploy
/// @notice `RainDeployBroadcast` over this repo's one suite declaration:
/// selects a suite by `DEPLOYMENT_SUITE` and deploys it through the Zoltu
/// factory, after anchoring every candidate snapshot to current source.
/// Broadcasts to every network in `LibRainDeploy.supportedNetworks()` — the
/// inherited default — so one dispatch puts the suite's one deterministic
/// address on every supported chain.
///
/// The sub-parser's described-by meta emission is NOT part of this broadcast —
/// `RainDeployBroadcast.run` has no post-deploy hook — it moved to
/// `script/DescribeSubParser.sol`, dispatched separately.
contract Deploy is RaindexDeploySuites, RainDeployBroadcast {}
