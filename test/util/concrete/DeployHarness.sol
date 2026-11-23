// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {DeployDeclaration, DeploySuite} from "rain-deploy-0.1.38/src/abstract/RainDeploySuitesBase.sol";
import {SupportedNetwork} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {Deploy} from "script/Deploy.sol";

/// @title DeployHarness
/// @notice An external seam onto the declaration `Deploy` broadcasts from.
contract DeployHarness is Deploy {
    /// @return The networks one broadcast goes to.
    function externalDeployNetworks() external pure returns (string[] memory) {
        return declaration().deployNetworks;
    }

    /// @return Every network the declaration deals with.
    function externalNetworks() external pure returns (string[] memory) {
        return declaration().networks;
    }

    /// @return The hash of the released suites and the candidates.
    function externalSuitesHash() external pure returns (bytes32) {
        DeployDeclaration memory d = declaration();
        return keccak256(abi.encode(d.released, d.candidates));
    }

    /// `preDeployCheck` for a suite key on a network name.
    /// @param suite The suite key.
    /// @param network The network name.
    function externalPreDeployCheck(string memory suite, string memory network) external pure {
        DeployDeclaration memory d;
        DeploySuite memory deploySuite;
        deploySuite.suite = suite;
        SupportedNetwork memory supportedNetwork;
        supportedNetwork.name = network;
        preDeployCheck(d, deploySuite, supportedNetwork);
    }
}
