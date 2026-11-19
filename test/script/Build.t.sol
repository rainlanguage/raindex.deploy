// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {GeneratedContract, RetiredContract} from "script/Build.sol";
import {DeployCandidate, DeploySuite} from "src/abstract/RainDeploySuitesBase.sol";
import {LibRainDeploySnapshot} from "rain-deploy-0.1.36/src/lib/LibRainDeploySnapshot.sol";
import {LibReleasedSuites} from "src/lib/LibReleasedSuites.sol";
import {BuildHarness} from "test/util/concrete/BuildHarness.sol";

/// @title BuildTest
/// @notice `script/Build.sol`'s own declaration.
///
/// `generatedContracts()` is the one list every hook inside `Build` reads, so
/// the regeneration, the released-lib writer and the freeze cannot disagree
/// with each other. The boundary is where they can: what this repo deploys is
/// declared in `RaindexDeploySuites.declaration()`, and the generator's
/// list is a second hard-coded array in a second file.
///
/// A candidate dropped from `generatedContracts()` by a refactor or a merge
/// still compiles, because its committed snapshot is still there and still
/// imported. `cutRelease()` then freezes only the contracts the generator
/// names, `regenerateLibs()` writes a released-suites lib only for those, and
/// the release permanently omits that contract. Nothing downstream can see it:
/// `testEveryFrozenSnapshotIsReleased` walks record -> declaration, so a record
/// entry never written is invisible to it, and `RaindexDeployChainTest` is
/// never handed the omitted suite. The frozen tag then cannot be re-cut, so the
/// hole is permanent.
///
/// Deliberately nothing here calls `run()` or `cutRelease()`. Both rewrite the
/// committed `src/generated/` snapshots and `src/lib/` libs that other test
/// contracts read, and forge runs test contracts in parallel — a contract
/// rewriting what another one is reading is a race, not a check. Nothing below
/// writes anything.
///
/// There is no constant-prefix property here, unlike the org's template deploy
/// repos: this repo emits no generated alias libs, so `GeneratedContract` has
/// no prefix to collide.
contract BuildTest is Test {
    /// The harness the declarations are read through.
    BuildHarness internal sBuild;

    function setUp() external {
        sBuild = new BuildHarness();
    }

    /// PROPERTY: the generator's list and the deploy declaration are the SAME
    /// SET of contracts, matched both ways. A candidate the generator does not
    /// name is a contract silently absent from every release cut from here, and
    /// neither the frozen-record check nor the chain check can see it, because
    /// both are only ever handed what was written.
    ///
    /// Matched by the suite key rather than by index, because a positional pass
    /// would go green on a reordered list that had silently swapped two
    /// contracts. The creation code is compared on the matched pair for the
    /// same reason: a generated entry can name the right key while carrying
    /// another contract's candidate, which writes one contract's pins into the
    /// other's snapshot file.
    function testGeneratedContractsAreExactlyTheDeclaredCandidates() external view {
        GeneratedContract[] memory generated = sBuild.externalGeneratedContracts();
        DeployCandidate[] memory candidates = sBuild.externalCandidateSuites();

        assertEq(
            generated.length, candidates.length, "a candidate is not generated, or a generated contract is not declared"
        );

        for (uint256 i = 0; i < generated.length; i++) {
            bool found = false;
            for (uint256 j = 0; j < candidates.length; j++) {
                if (
                    keccak256(bytes(generated[i].candidate.snapshot.suite))
                        == keccak256(bytes(candidates[j].snapshot.suite))
                ) {
                    assertEq(
                        keccak256(generated[i].candidate.snapshot.creationCode),
                        keccak256(candidates[j].snapshot.creationCode),
                        "generated entry carries another contract's creation code"
                    );
                    found = true;
                    break;
                }
            }
            assertTrue(
                found, string.concat("generated contract is not a declared candidate: ", generated[i].contractName)
            );
        }

        for (uint256 j = 0; j < candidates.length; j++) {
            bool found = false;
            for (uint256 i = 0; i < generated.length; i++) {
                if (
                    keccak256(bytes(generated[i].candidate.snapshot.suite))
                        == keccak256(bytes(candidates[j].snapshot.suite))
                ) {
                    found = true;
                    break;
                }
            }
            assertTrue(found, string.concat("declared candidate is not generated: ", candidates[j].snapshot.suite));
        }
    }

    /// PROPERTY: `snapshotContractNames()` is every `generatedContracts()`
    /// entry's `contractName`, positionally.
    ///
    /// It is the list `cutRelease()` freezes, reached only through `forge
    /// script`. A name list shorter than the declaration freezes one contract
    /// fewer, while the test above stays green: it reads
    /// `generatedContracts()`, which this list does not pass through.
    function testSnapshotContractNamesAreTheDeclarationInOrder() external view {
        GeneratedContract[] memory generated = sBuild.externalGeneratedContracts();
        string[] memory names = sBuild.externalSnapshotContractNames();

        assertEq(names.length, generated.length, "a different number of names than generated contracts");

        for (uint256 i = 0; i < generated.length; i++) {
            assertEq(names[i], generated[i].contractName, "the names are not the declaration in order");
        }
    }

    /// PROPERTY: `releasedContractNames()` is every `retiredContracts()`
    /// entry's `contractName` followed by every `generatedContracts()` entry's,
    /// positionally.
    ///
    /// It is the list the released-suites aggregate is emitted from. A name
    /// missing from it is a contract whose releases the aggregate declares as
    /// nothing at all. Positional because the aggregate documents its entries
    /// as being in this order and nothing else pins that.
    function testReleasedContractNamesAreRetiredThenDeclarationInOrder() external view {
        RetiredContract[] memory retired = sBuild.externalRetiredContracts();
        GeneratedContract[] memory generated = sBuild.externalGeneratedContracts();
        string[] memory names = sBuild.externalReleasedContractNames();

        assertEq(names.length, retired.length + generated.length, "a different number of names than released contracts");

        for (uint256 i = 0; i < retired.length; i++) {
            assertEq(names[i], retired[i].contractName, "the retired contracts do not lead in order");
        }
        for (uint256 i = 0; i < generated.length; i++) {
            assertEq(names[retired.length + i], generated[i].contractName, "the declaration does not follow in order");
        }
    }

    /// PROPERTY: every retired contract is released under a generated
    /// candidate's suite key, and its name is no released contract's other
    /// than its own. A retired contract under an undeclared key is a release no
    /// chain check is handed, and a shared name writes one released lib twice.
    ///
    /// Only the template's key and artifact path reach the released lib; every
    /// other field is read from the record.
    function testRetiredContractsReleaseUnderAGeneratedCandidate() external view {
        RetiredContract[] memory retired = sBuild.externalRetiredContracts();
        GeneratedContract[] memory generated = sBuild.externalGeneratedContracts();
        string[] memory names = sBuild.externalReleasedContractNames();

        for (uint256 i = 0; i < retired.length; i++) {
            bool found = false;
            for (uint256 j = 0; j < generated.length; j++) {
                if (
                    keccak256(bytes(retired[i].template.suite))
                        == keccak256(bytes(generated[j].candidate.snapshot.suite))
                ) {
                    found = true;
                    break;
                }
            }
            assertTrue(found, string.concat("retired contract is not a generated candidate: ", retired[i].contractName));

            uint256 count = 0;
            for (uint256 k = 0; k < names.length; k++) {
                if (keccak256(bytes(names[k])) == keccak256(bytes(retired[i].contractName))) {
                    count++;
                }
            }
            assertEq(count, 1, string.concat("retired contract name is not unique: ", retired[i].contractName));
        }
    }

    /// PROPERTY: no two entries name one contract. Two entries sharing a
    /// `contractName` write one snapshot file twice and freeze one file for two
    /// declarations, so the second silently replaces the first.
    function testGeneratedContractNamesAreUnique() external view {
        GeneratedContract[] memory generated = sBuild.externalGeneratedContracts();
        for (uint256 i = 0; i < generated.length; i++) {
            for (uint256 j = i + 1; j < generated.length; j++) {
                assertNotEq(
                    generated[i].contractName, generated[j].contractName, "two generated entries name one contract"
                );
            }
        }
    }

    /// PROPERTY: `contractName` is the name the snapshot path and the generated
    /// released lib are built from, and it MUST be the contract the candidate's
    /// artifact path names. A disagreement writes one contract's pins into
    /// another contract's file, which every downstream check then reads as
    /// self-consistent.
    function testGeneratedContractNameMatchesTheArtifactPath() external view {
        GeneratedContract[] memory generated = sBuild.externalGeneratedContracts();
        for (uint256 i = 0; i < generated.length; i++) {
            string[] memory components = vm.split(generated[i].candidate.snapshot.artifactPath, ":");
            assertEq(
                components[components.length - 1],
                generated[i].contractName,
                "generated name is not the artifact's contract"
            );
        }
    }

    /// PROPERTY: a retired contract keeps the artifact path it was cut with,
    /// the one naming its former contract, so its released-suites lib re-emits
    /// the frozen record unchanged.
    function testRetiredContractArtifactPathNamesTheFormerContract() external view {
        RetiredContract[] memory retired = sBuild.externalRetiredContracts();
        for (uint256 i = 0; i < retired.length; i++) {
            string[] memory components = vm.split(retired[i].template.artifactPath, ":");
            assertEq(
                components[components.length - 1], retired[i].contractName, "retired contract's artifact is not its own"
            );
        }
    }

    /// The committed aggregate is what `regenerateLibs` emits from these lists: each released contract's records
    /// in tag order, under its template's key and artifact path, in `releasedContractNames()` order.
    function testReleasedSuitesAreTheRecordUnderTheBuildLists() external view {
        RetiredContract[] memory retired = sBuild.externalRetiredContracts();
        GeneratedContract[] memory generated = sBuild.externalGeneratedContracts();
        DeploySuite[] memory released = LibReleasedSuites.releasedSuites();

        uint256 k = 0;
        for (uint256 i = 0; i < retired.length; i++) {
            k = checkReleasedRecords(released, k, retired[i].contractName, retired[i].template);
        }
        for (uint256 i = 0; i < generated.length; i++) {
            k = checkReleasedRecords(released, k, generated[i].contractName, generated[i].candidate.snapshot);
        }
        assertEq(k, released.length, "a released suite is no released contract's record");
    }

    /// @return The position in `released` after `contractName`'s records.
    function checkReleasedRecords(
        DeploySuite[] memory released,
        uint256 k,
        string memory contractName,
        DeploySuite memory template
    ) internal view returns (uint256) {
        string[] memory records = LibRainDeploySnapshot.recordPathsForContract(
            vm, sBuild.externalRecordRoot(), contractName
        );
        for (uint256 j = 0; j < records.length; j++) {
            assertLt(k, released.length, string.concat("record is not released: ", records[j]));
            assertEq(
                released[k].suite,
                string.concat(template.suite, "@", LibRainDeploySnapshot.tagForRecordPath(vm, records[j])),
                string.concat("released key is not the template's: ", records[j])
            );
            assertEq(
                released[k].artifactPath,
                template.artifactPath,
                string.concat("released artifact path is not the template's: ", records[j])
            );
            k++;
        }
        return k;
    }

    function testEveryGeneratedPathIsCommitted() external view {
        string[] memory paths = sBuild.externalGeneratedPaths();
        for (uint256 i = 0; i < paths.length; i++) {
            assertTrue(vm.isFile(paths[i]), string.concat("generated path is not committed: ", paths[i]));
        }
    }
}
