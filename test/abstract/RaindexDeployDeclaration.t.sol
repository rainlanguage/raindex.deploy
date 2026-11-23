// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RainDeployVerifyDeclaration} from "rain-deploy-0.1.38/src/abstract/RainDeployVerifyDeclaration.sol";
import {
    DeployDeclaration,
    DeployDependency,
    DeploySuite
} from "rain-deploy-0.1.38/src/abstract/RainDeploySuitesBase.sol";
import {LibRainDeploy} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {UnreleasableVersion} from "rain-deploy-0.1.38/src/lib/LibRainDeploySnapshot.sol";
import {
    RaindexDeploySuites,
    NoSourceForSuite,
    ROUTE_PROCESSOR_6_SUITE,
    ROUTE_PROCESSOR_6_ARB_ORDER_TAKER_V6_SUITE
} from "../../src/abstract/RaindexDeploySuites.sol";
import {
    DEPLOYED_ADDRESS as ROUTE_PROCESSOR_6_ADDRESS,
    RUNTIME_CODE as ROUTE_PROCESSOR_6_RUNTIME_CODE
} from "../../src/generated/candidate/RouteProcessor6.sol";
import {
    DEPENDENCIES as RP_ARB_OT_V6_DEPENDENCIES
} from "../../src/generated/0_1_15/RouteProcessorRaindexV6ArbOrderTaker.sol";
import {DEPLOYED_ADDRESS as ROUTE_PROCESSOR_4_V6_ADDRESS} from "../../src/generated/0_1_15/RouteProcessor4.sol";

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

    /// @param suite The suite key.
    /// @return Whether `suite` is not required on `zerog`.
    function externalNotRequiredOnZerog(string memory suite) external pure returns (bool) {
        return notRequiredOnZerog(suite);
    }

    /// @param suite A suite key.
    /// @param tag A release tag.
    /// @return Whether `suite` is the release `tag` of its contract.
    function isRelease(string memory suite, string memory tag) internal pure returns (bool) {
        return keccak256(bytes(suite)) == keccak256(bytes(string.concat(suiteKeyName(suite), "@", tag)));
    }

    /// @param networks A roster.
    /// @param network A network name.
    /// @return Whether `networks` holds `network`.
    function holds(string[] memory networks, string memory network) internal pure returns (bool) {
        for (uint256 i = 0; i < networks.length; i++) {
            if (keccak256(bytes(networks[i])) == keccak256(bytes(network))) {
                return true;
            }
        }
        return false;
    }

    /// `zerog` requires the six contracts of the audited `RaindexV6` releases
    /// `0_1_14` and `0_1_15`, and none of the `RaindexV7` releases `0_1_16` to
    /// `0_1_18`, which keep the roster they were deployed for. The keys are
    /// named here, not selected with `notRequiredOnZerog`, so this checks the
    /// declaration and not the predicate.
    function testZerogRequiresTheRaindexV6ReleasesOnly() external pure {
        DeployDeclaration memory d = declaration();
        // The narrowing is visible only while the declaration has `zerog`.
        assertTrue(holds(d.networks, LibRainDeploy.ZEROG));
        assertGt(d.networks.length, networksBeforeZerog().length);
        uint256 v6 = 0;
        uint256 v7 = 0;
        for (uint256 i = 0; i < d.released.length; i++) {
            DeploySuite memory suite = d.released[i];
            if (
                isRelease(suite.suite, "0_1_16") || isRelease(suite.suite, "0_1_17") || isRelease(suite.suite, "0_1_18")
            ) {
                v7++;
                assertEq(suite.networks, networksBeforeZerog(), suite.suite);
                assertFalse(requiresSuiteOn(suite, LibRainDeploy.ZEROG), suite.suite);
            } else if (requiredOnZerogOnly(suite.suite)) {
                assertEq(suite.networks, zerogNetworks(), suite.suite);
            } else {
                if (isRelease(suite.suite, "0_1_14") || isRelease(suite.suite, "0_1_15")) {
                    v6++;
                }
                assertEq(suite.networks, d.networks, suite.suite);
                assertTrue(requiresSuiteOn(suite, LibRainDeploy.ZEROG), suite.suite);
            }
        }
        assertEq(v6, 12);
        assertEq(v7, 18);

        DeploySuite memory raindex = suiteByName(d, "raindex@0_1_15");
        assertEq(raindex.storedDeployedAddress, 0x37FC0EFec37D19f8A221aa4F8F7600C9ba2AcD20);
        assertTrue(requiresSuiteOn(raindex, LibRainDeploy.ZEROG));
        assertTrue(requiresSuiteOn(suiteByName(d, "subparser@0_1_15"), LibRainDeploy.ZEROG));
        assertTrue(requiresSuiteOn(suiteByName(d, "route-processor@0_1_14"), LibRainDeploy.ZEROG));
        assertFalse(requiresSuiteOn(suiteByName(d, "raindex@0_1_16"), LibRainDeploy.ZEROG));
        assertFalse(requiresSuiteOn(suiteByName(d, "subparser@0_1_17"), LibRainDeploy.ZEROG));
        assertFalse(requiresSuiteOn(suiteByName(d, "route-processor@0_1_18"), LibRainDeploy.ZEROG));
    }

    /// `narrowReleasedNetworks` replaces the networks of the releases after
    /// `LAST_RAINDEX_V6_RELEASE` and up to `LAST_RELEASE_BEFORE_ZEROG` with
    /// `networksBeforeZerog()`, and leaves every other suite as it was, whatever
    /// the catalogue holds.
    function testNarrowReleasedNetworksReplacesOnlyTheReleasesBeforeZerog() external pure {
        string[] memory sentinel = new string[](1);
        sentinel[0] = "sentinel";
        string[7] memory keys = [
            "raindex",
            "raindex@0_1_14",
            "arb-generic-pool-flash-borrower@0_1_15",
            "raindex@0_1_16",
            "subparser@0_1_17",
            "route-processor@0_1_18",
            "raindex@0_1_19"
        ];
        bool[7] memory narrowedKeys = [false, false, false, true, true, true, false];
        DeploySuite[] memory released = new DeploySuite[](keys.length);
        for (uint256 i = 0; i < keys.length; i++) {
            released[i].suite = keys[i];
            released[i].networks = sentinel;
        }
        DeploySuite[] memory narrowed = narrowReleasedNetworks(released);
        assertEq(narrowed.length, keys.length);
        for (uint256 i = 0; i < keys.length; i++) {
            assertEq(narrowed[i].suite, keys[i]);
            assertEq(narrowed[i].networks, narrowedKeys[i] ? networksBeforeZerog() : sentinel, keys[i]);
        }
    }

    /// The roster before `zerog` is the nine networks the `RaindexV7` releases
    /// were deployed to, each still in the catalogue, and never `zerog`. It is
    /// pinned here rather than derived from the catalogue, which grows.
    function testNetworksBeforeZerogAreFrozenAndSupported() external pure {
        string[] memory before = networksBeforeZerog();
        string[9] memory frozen =
            ["arbitrum", "base", "base_sepolia", "bsc", "ethereum", "flare", "hyperevm", "polygon", "robinhood"];
        assertEq(before.length, frozen.length);
        string[] memory supported = LibRainDeploy.supportedNetworks();
        for (uint256 i = 0; i < before.length; i++) {
            assertEq(before[i], frozen[i]);
            assertTrue(holds(supported, before[i]), before[i]);
        }
        assertFalse(holds(before, LibRainDeploy.ZEROG));
    }

    /// `notRequiredOnZerog` selects the tags after `0_1_15` and up to `0_1_18`,
    /// comparing every component as a number.
    function testNotRequiredOnZerog() external pure {
        assertFalse(notRequiredOnZerog("raindex"));
        assertFalse(notRequiredOnZerog("raindex@0_1_14"));
        assertFalse(notRequiredOnZerog("arb-generic-pool-flash-borrower@0_1_15"));
        assertTrue(notRequiredOnZerog("raindex@0_1_16"));
        assertTrue(notRequiredOnZerog("subparser@0_1_17"));
        assertTrue(notRequiredOnZerog("route-processor@0_1_18"));
        assertFalse(notRequiredOnZerog("raindex@0_1_19"));
        assertFalse(notRequiredOnZerog("raindex@0_1_100"));
        assertFalse(notRequiredOnZerog("raindex@0_2_0"));
        assertFalse(notRequiredOnZerog("raindex@0_0_99"));
        // A later major with minor and patch inside the window.
        assertFalse(notRequiredOnZerog("raindex@1_1_16"));
        assertFalse(notRequiredOnZerog("raindex@1_0_0"));
        // A multi-digit minor that would read as `0_1_16` with its digits run together.
        assertFalse(notRequiredOnZerog("raindex@0_11_6"));
        // Multi-digit patches whose digit sums fall inside the window.
        assertFalse(notRequiredOnZerog("raindex@0_1_25"));
        assertFalse(notRequiredOnZerog("raindex@0_1_160"));
    }

    /// The `RouteProcessor6` candidates are required on `zerog` and on no
    /// other network.
    function testRouteProcessor6SuitesRequiredOnZerogOnly() external pure {
        DeployDeclaration memory d = declaration();
        string[2] memory keys = [ROUTE_PROCESSOR_6_SUITE, ROUTE_PROCESSOR_6_ARB_ORDER_TAKER_V6_SUITE];
        for (uint256 i = 0; i < keys.length; i++) {
            DeploySuite memory suite = suiteByName(d, keys[i]);
            assertEq(suite.networks, zerogNetworks(), keys[i]);
            for (uint256 j = 0; j < d.networks.length; j++) {
                bool isZerog = keccak256(bytes(d.networks[j])) == keccak256(bytes(LibRainDeploy.ZEROG));
                assertEq(requiresSuiteOn(suite, d.networks[j]), isZerog, keys[i]);
            }
        }
    }

    /// The `RouteProcessor6` arb depends on what the `0_1_15`
    /// `RouteProcessorRaindexV6ArbOrderTaker` depends on, with
    /// `RouteProcessor4` replaced by the `RouteProcessor6` candidate.
    function testRouteProcessor6ArbOrderTakerV6Dependencies() external pure {
        DeployDependency[] memory frozen = abi.decode(RP_ARB_OT_V6_DEPENDENCIES, (DeployDependency[]));
        DeployDependency[] memory deps =
        suiteByName(declaration(), ROUTE_PROCESSOR_6_ARB_ORDER_TAKER_V6_SUITE).dependencies;
        assertEq(frozen.length, 3);
        assertEq(deps.length, 3);
        assertEq(deps[0].deployedAddress, 0x37FC0EFec37D19f8A221aa4F8F7600C9ba2AcD20);
        assertEq(deps[1].deployedAddress, 0x200e12D10bb0c5E4a17e7018f0F1161919bb9389);
        for (uint256 i = 0; i < 2; i++) {
            assertEq(deps[i].deployedAddress, frozen[i].deployedAddress);
            assertEq(deps[i].runtimeCode, frozen[i].runtimeCode);
        }
        assertEq(frozen[2].deployedAddress, ROUTE_PROCESSOR_4_V6_ADDRESS);
        assertEq(deps[2].deployedAddress, ROUTE_PROCESSOR_6_ADDRESS);
        assertEq(deps[2].runtimeCode, ROUTE_PROCESSOR_6_RUNTIME_CODE);
    }

    /// `narrowReleasedNetworks` puts every release of the `RouteProcessor6`
    /// suites on `zerog` only, and leaves the `RouteProcessor4` suites after
    /// `LAST_RELEASE_BEFORE_ZEROG` as they were.
    function testNarrowReleasedNetworksKeepsRouteProcessor6OnZerog() external pure {
        string[] memory sentinel = new string[](1);
        sentinel[0] = "sentinel";
        string[4] memory keys = [
            "route-processor-six@0_1_19",
            "arb-route-processor-six-order-taker@0_1_19",
            "route-processor@0_1_19",
            "arb-route-processor-order-taker@0_1_19"
        ];
        bool[4] memory zerogOnly = [true, true, false, false];
        DeploySuite[] memory released = new DeploySuite[](keys.length);
        for (uint256 i = 0; i < keys.length; i++) {
            released[i].suite = keys[i];
            released[i].networks = sentinel;
        }
        DeploySuite[] memory narrowed = narrowReleasedNetworks(released);
        for (uint256 i = 0; i < keys.length; i++) {
            assertEq(narrowed[i].networks, zerogOnly[i] ? zerogNetworks() : sentinel, keys[i]);
        }
    }

    /// A key whose tag is not `X_Y_Z` digits is refused, not read as a version.
    function testNotRequiredOnZerogRefusesAMalformedTag() external {
        string[6] memory tags = ["0_1_1a", "0_1_16_1", "0_1_16@0_1_14", "0_1", "-0_1_16", ""];
        for (uint256 i = 0; i < tags.length; i++) {
            vm.expectRevert(abi.encodeWithSelector(UnreleasableVersion.selector, tags[i]));
            this.externalNotRequiredOnZerog(string.concat("raindex@", tags[i]));
        }
    }
}
