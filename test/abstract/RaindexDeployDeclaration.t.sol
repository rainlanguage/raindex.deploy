// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RainDeployVerifyDeclaration} from "rain-deploy-0.1.37/src/abstract/RainDeployVerifyDeclaration.sol";
import {DeployDeclaration, DeploySuite} from "rain-deploy-0.1.37/src/abstract/RainDeploySuitesBase.sol";
import {LibRainDeploy} from "rain-deploy-0.1.37/src/lib/LibRainDeploy.sol";
import {RaindexDeploySuites, NoSourceForSuite} from "../../src/abstract/RaindexDeploySuites.sol";

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
        vm.expectRevert(abi.encodeWithSelector(NoSourceForSuite.selector, suite));
        this.externalSourceCreationCode(suite);
    }

    /// The `RaindexV6` releases are required on every declared network; the
    /// releases after them and before `zerog` joined only on the roster they
    /// were deployed for.
    function testReleasedNetworksAroundZerog() external pure {
        DeployDeclaration memory d = declaration();
        uint256 narrowed = 0;
        for (uint256 i = 0; i < d.released.length; i++) {
            DeploySuite memory suite = d.released[i];
            if (releasedBeforeZerog(suite.suite)) {
                narrowed++;
                assertEq(suite.networks, networksBeforeZerog(), suite.suite);
            } else {
                assertEq(suite.networks, d.networks, suite.suite);
            }
        }
        // Six contracts each in 0_1_16 and 0_1_17.
        assertEq(narrowed, 12);
    }

    /// The roster before `zerog` is the supported roster without it.
    function testNetworksBeforeZerogAreTheSupportedNetworksWithoutZerog() external pure {
        string[] memory before = networksBeforeZerog();
        string[] memory supported = LibRainDeploy.supportedNetworks();
        uint256 kept = 0;
        for (uint256 i = 0; i < supported.length; i++) {
            if (keccak256(bytes(supported[i])) == keccak256("zerog")) {
                continue;
            }
            assertLt(kept, before.length, supported[i]);
            assertEq(before[kept], supported[i]);
            kept++;
        }
        assertEq(kept, before.length);
    }

    function testReleasedBeforeZerog() external pure {
        assertFalse(releasedBeforeZerog("raindex"));
        assertFalse(releasedBeforeZerog("raindex@0_1_14"));
        assertFalse(releasedBeforeZerog("arb-generic-pool-flash-borrower@0_1_15"));
        assertTrue(releasedBeforeZerog("raindex@0_1_16"));
        assertTrue(releasedBeforeZerog("subparser@0_1_17"));
        assertFalse(releasedBeforeZerog("raindex@0_1_18"));
        assertFalse(releasedBeforeZerog("raindex@0_1_100"));
        assertFalse(releasedBeforeZerog("raindex@0_2_0"));
        assertFalse(releasedBeforeZerog("raindex@0_0_99"));
    }
}
