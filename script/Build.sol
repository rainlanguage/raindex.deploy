// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {BuildScript} from "rain-deploy-0.1.33/src/abstract/BuildScript.sol";
import {LibRainDeploySnapshot} from "rain-deploy-0.1.33/src/lib/LibRainDeploySnapshot.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {LibCodeGen} from "rain-sol-codegen-0.1.39/src/lib/LibCodeGen.sol";
import {LibFs} from "rain-sol-codegen-0.1.39/src/lib/LibFs.sol";
import {LibGenParseMeta} from "rainlang-0.2.35/src/lib/codegen/LibGenParseMeta.sol";
import {DeployCandidate} from "../src/abstract/RainDeploySuitesBase.sol";
import {RaindexDeploySuites} from "../src/abstract/RaindexDeploySuites.sol";
import {RaindexV7SubParser} from "../src/concrete/parser/RaindexV7SubParser.sol";
import {ROUTE_PROCESSOR_4_CREATION_CODE} from "../src/lib/deploy/LibRouteProcessor4CreationCode.sol";
import {LibRaindexSubParser, EXTERN_PARSE_META_BUILD_DEPTH} from "../src/lib/LibRaindexSubParser.sol";

/// One contract's generated snapshot and the released-suites lib emitted from
/// its record.
struct GeneratedContract {
    /// Places the snapshot inside `src/generated/<dir>/` and names the
    /// generated released-suites lib.
    string contractName;
    /// The released lib takes its suite key and artifact path from its
    /// `snapshot`.
    DeployCandidate candidate;
    /// Creation code for an unanchorable candidate; empty means read the
    /// `artifactPath` artifact, keeping restricted compiler profiles out of here.
    bytes pinnedCreationCode;
}

/// @title Build
/// @notice Generates the deploy pins for every contract this repo deploys.
/// `generatedContracts()` is the only list, read by every hook below.
///
/// `run()` (what CI regenerates against) rewrites the rolling
/// `src/generated/candidate/` snapshots, the sub-parser's non-deploy codegen
/// and the released-suites libs. `cutRelease()` freezes the candidates into
/// `src/generated/<tag>/` first. Frozen snapshots are append-only historical
/// records, never regenerated here.
///
/// Unlike the org's template deploy repos there are no generated alias libs:
/// `src/lib/deploy/LibRaindexDeploy.sol` is hand-written over the candidate
/// snapshots because its constant names are consumed across this repo and its
/// crates.
contract Build is BuildScript, RaindexDeploySuites {
    /// Every contract this repo generates deploy pins for.
    /// @return The generated contracts.
    function generatedContracts() internal pure returns (GeneratedContract[] memory) {
        GeneratedContract[] memory contracts = new GeneratedContract[](6);
        contracts[0] =
            GeneratedContract({contractName: "RaindexV7", candidate: raindexCandidate(), pinnedCreationCode: ""});
        contracts[1] = GeneratedContract({
            contractName: "RaindexV7SubParser", candidate: subParserCandidate(), pinnedCreationCode: ""
        });
        contracts[2] = GeneratedContract({
            contractName: "RouteProcessor4",
            candidate: routeProcessorCandidate(),
            pinnedCreationCode: ROUTE_PROCESSOR_4_CREATION_CODE
        });
        contracts[3] = GeneratedContract({
            contractName: "GenericPoolRaindexV7ArbOrderTaker",
            candidate: genericPoolArbOrderTakerCandidate(),
            pinnedCreationCode: ""
        });
        contracts[4] = GeneratedContract({
            contractName: "RouteProcessorRaindexV7ArbOrderTaker",
            candidate: routeProcessorArbOrderTakerCandidate(),
            pinnedCreationCode: ""
        });
        contracts[5] = GeneratedContract({
            contractName: "GenericPoolRaindexV7FlashBorrower",
            candidate: genericPoolFlashBorrowerCandidate(),
            pinnedCreationCode: ""
        });
        return contracts;
    }

    /// Frozen records cut under a contract's former name, each released under
    /// the current candidate it was renamed to, keeping the artifact path it
    /// was cut with.
    /// @return The renamed records.
    function renamedRecords() internal pure returns (GeneratedContract[] memory) {
        GeneratedContract[] memory contracts = new GeneratedContract[](5);
        contracts[0] = renamedRecord("RaindexV6", raindexCandidate(), "src/concrete/raindex/RaindexV6.sol:RaindexV6");
        contracts[1] = renamedRecord(
            "RaindexV6SubParser", subParserCandidate(), "src/concrete/parser/RaindexV6SubParser.sol:RaindexV6SubParser"
        );
        contracts[2] = renamedRecord(
            "GenericPoolRaindexV6ArbOrderTaker",
            genericPoolArbOrderTakerCandidate(),
            "src/concrete/arb/GenericPoolRaindexV6ArbOrderTaker.sol:GenericPoolRaindexV6ArbOrderTaker"
        );
        contracts[3] = renamedRecord(
            "RouteProcessorRaindexV6ArbOrderTaker",
            routeProcessorArbOrderTakerCandidate(),
            "src/concrete/arb/RouteProcessorRaindexV6ArbOrderTaker.sol:RouteProcessorRaindexV6ArbOrderTaker"
        );
        contracts[4] = renamedRecord(
            "GenericPoolRaindexV6FlashBorrower",
            genericPoolFlashBorrowerCandidate(),
            "src/concrete/arb/GenericPoolRaindexV6FlashBorrower.sol:GenericPoolRaindexV6FlashBorrower"
        );
        return contracts;
    }

    /// A record cut as `contractName` from `formerArtifactPath`, released under
    /// `candidate`.
    /// @param contractName The former contract name.
    /// @param candidate The current candidate it was renamed to.
    /// @param formerArtifactPath The artifact path the record was cut with.
    /// @return The renamed record.
    function renamedRecord(
        string memory contractName,
        DeployCandidate memory candidate,
        string memory formerArtifactPath
    ) internal pure returns (GeneratedContract memory) {
        candidate.snapshot.artifactPath = formerArtifactPath;
        return GeneratedContract({contractName: contractName, candidate: candidate, pinnedCreationCode: ""});
    }

    /// Every contract with a released-suites lib: the renamed records first,
    /// so each suite key's releases stay in tag order across the aggregate.
    /// @return The released contracts.
    function releasedContracts() internal pure returns (GeneratedContract[] memory) {
        GeneratedContract[] memory renamed = renamedRecords();
        GeneratedContract[] memory generated = generatedContracts();
        GeneratedContract[] memory contracts = new GeneratedContract[](renamed.length + generated.length);
        for (uint256 i = 0; i < renamed.length; i++) {
            contracts[i] = renamed[i];
        }
        for (uint256 i = 0; i < generated.length; i++) {
            contracts[renamed.length + i] = generated[i];
        }
        return contracts;
    }

    /// The names of `contracts`, in order.
    /// @param contracts The contracts.
    /// @return The names.
    function contractNames(GeneratedContract[] memory contracts) internal pure returns (string[] memory) {
        string[] memory names = new string[](contracts.length);
        for (uint256 i = 0; i < contracts.length; i++) {
            names[i] = contracts[i].contractName;
        }
        return names;
    }

    /// @inheritdoc BuildScript
    /// @dev `releasedContracts()` in order — the order the aggregate emits its
    /// entries in.
    function snapshotContractNames() internal pure override returns (string[] memory) {
        return contractNames(releasedContracts());
    }

    /// @inheritdoc BuildScript
    function releaseRecordNames() internal pure override returns (string[] memory) {
        return contractNames(generatedContracts());
    }

    /// @inheritdoc BuildScript
    /// @dev Every released-suites lib and the aggregate over them. No alias
    /// libs: `LibRaindexDeploy` is hand-written.
    function regenerateLibs() internal override {
        GeneratedContract[] memory contracts = releasedContracts();
        for (uint256 i = 0; i < contracts.length; i++) {
            LibRainDeploySnapshot.writeReleasedSuitesLib(
                vm,
                LibRainDeploySnapshot.LIB_DIR,
                recordRoot(),
                contracts[i].contractName,
                contracts[i].candidate.snapshot
            );
        }
        LibRainDeploySnapshot.writeReleasedSuitesAggregate(vm, LibRainDeploySnapshot.LIB_DIR, snapshotContractNames());
    }

    /// @inheritdoc BuildScript
    function regenerateSnapshots() internal override {
        GeneratedContract[] memory contracts = generatedContracts();
        for (uint256 i = 0; i < contracts.length; i++) {
            LibRainDeploySnapshot.writeSnapshot(
                vm,
                recordRoot(),
                LibRainDeploySnapshot.CANDIDATE,
                contracts[i].contractName,
                bytes(contracts[i].candidate.unanchorableReason).length == 0
                    ? vm.getCode(contracts[i].candidate.snapshot.artifactPath)
                    : contracts[i].pinnedCreationCode,
                contracts[i].candidate.snapshot.dependencies
            );
        }
        buildSubParserPointers();
    }

    /// The sub-parser's non-deploy codegen: parse meta, function pointers and
    /// described-by meta hash. `LibRainDeploySnapshot.writeSnapshot` only ever
    /// emits deploy constants, so these live in their own generated file,
    /// `src/generated/RaindexV7SubParserPointers.sol`, consumed by
    /// `RaindexV7SubParser` itself (the usual committed-codegen cycle).
    ///
    /// The name is distinct from the contract's so the file cannot be taken
    /// for a snapshot of `RaindexV7SubParser`, and so it survives
    /// `LibFs.requireNoOrphanedArtifact` beside the candidate record.
    ///
    /// The instance read here is the one the `writeSnapshot` loop in
    /// `regenerateSnapshots` already deployed via the Zoltu factory, reached
    /// again through its deterministic address — deploying twice through the
    /// factory would collide.
    function buildSubParserPointers() internal {
        RaindexV7SubParser subParser =
            RaindexV7SubParser(LibRainDeploy.zoltuAddress(type(RaindexV7SubParser).creationCode));

        LibFs.buildFileForContract(
            vm,
            address(subParser),
            "RaindexV7SubParserPointers",
            string.concat(
                LibCodeGen.describedByMetaHashConstantString(vm, "RaindexV7SubParser"),
                LibGenParseMeta.parseMetaConstantString(
                    vm, LibRaindexSubParser.authoringMetaV2(), EXTERN_PARSE_META_BUILD_DEPTH
                ),
                LibCodeGen.subParserWordParsersConstantString(vm, subParser),
                LibCodeGen.operandHandlerFunctionPointersConstantString(vm, subParser),
                LibCodeGen.literalParserFunctionPointersConstantString(vm, subParser)
            )
        );
    }
}
