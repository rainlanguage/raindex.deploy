// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RainDeployBroadcast} from "rain-deploy-0.1.38/src/abstract/RainDeployBroadcast.sol";
import {DeployDeclaration, DeploySuite} from "rain-deploy-0.1.38/src/abstract/RainDeploySuitesBase.sol";
import {LibRainDeploy, SupportedNetwork} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {RaindexDeploySuites} from "../src/abstract/RaindexDeploySuites.sol";

/// Thrown when a suite that only `zerog` requires is about to be broadcast to
/// another network.
/// @param suite The suite key.
/// @param network The other network.
error ZerogOnlySuite(string suite, string network);

/// @title Deploy
/// @notice `RainDeployBroadcast` over this repo's one suite declaration:
/// selects a suite by `DEPLOYMENT_SUITE` and deploys it through the Zoltu
/// factory, after anchoring every candidate snapshot to current source.
/// Broadcasts to every network in `LibRainDeploy.supportedNetworks()` — the
/// inherited default — so one dispatch puts the suite's one deterministic
/// address on every supported chain. A suite that only `zerog` requires is
/// refused here; `script/DeployZerog.sol` deploys it.
///
/// The sub-parser's described-by meta emission is NOT part of this broadcast —
/// `RainDeployBroadcast.run` has no post-deploy hook — it moved to
/// `script/DescribeSubParser.sol`, dispatched separately.
contract Deploy is RaindexDeploySuites, RainDeployBroadcast {
    /// Refuses a suite that only `zerog` requires on any other network. The
    /// refusal reverts the simulation, so nothing is broadcast to any network.
    /// @param suite The suite about to be deployed.
    /// @param network The network whose fork is selected.
    function preDeployCheck(DeployDeclaration memory, DeploySuite memory suite, SupportedNetwork memory network)
        internal
        pure
        override
    {
        if (requiredOnZerogOnly(suite.suite) && keccak256(bytes(network.name)) != keccak256(bytes(LibRainDeploy.ZEROG)))
        {
            revert ZerogOnlySuite(suite.suite, network.name);
        }
    }
}
