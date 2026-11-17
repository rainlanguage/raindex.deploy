// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RainDeployVerifyDeclaration} from "rain-deploy-0.1.33/src/abstract/RainDeployVerifyDeclaration.sol";
import {DeployDeclaration, DeploySuite} from "rain-deploy-0.1.33/src/abstract/RainDeploySuitesBase.sol";
import {RaindexDeploySuites} from "../../src/abstract/RaindexDeploySuites.sol";

contract RaindexDeployDeclarationTest is RaindexDeploySuites, RainDeployVerifyDeclaration {
    /// @param suite The suite key.
    /// @return The declared source of `suite`.
    function externalSourceCreationCode(string memory suite) external view returns (bytes memory) {
        return declaration().sourceCreationCode(suite);
    }

    /// `script/Build.sol` regenerates an unanchorable candidate from its declared source.
    function testCandidateCreationCodeIsTheDeclaredSource() external view {
        DeployDeclaration memory d = declaration();
        for (uint256 i = 0; i < d.candidates.length; i++) {
            DeploySuite memory suite = d.candidates[i].snapshot;
            assertEq(keccak256(d.sourceCreationCode(suite.suite)), keccak256(suite.creationCode), suite.suite);
        }
    }

    function testSourceCreationCodeRefusesAnUndeclaredSuite(string memory suite) external {
        DeployDeclaration memory d = declaration();
        for (uint256 i = 0; i < d.candidates.length; i++) {
            vm.assume(keccak256(bytes(suite)) != keccak256(bytes(d.candidates[i].snapshot.suite)));
        }
        vm.expectRevert(bytes(string.concat("no source for suite: ", suite)));
        this.externalSourceCreationCode(suite);
    }
}
