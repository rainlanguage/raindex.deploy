// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {DeployDeclaration} from "rain-deploy-0.1.38/src/abstract/RainDeploySuitesBase.sol";
import {DeployZerog} from "script/DeployZerog.sol";

/// @title DeployZerogHarness
/// @notice An external seam onto the declaration `DeployZerog` broadcasts
/// from.
contract DeployZerogHarness is DeployZerog {
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
}
