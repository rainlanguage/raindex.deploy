// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {BuildScript, GeneratedContract, RetiredContract} from "rain-deploy-0.1.33/src/abstract/BuildScript.sol";
import {LibRainDeploySnapshot} from "rain-deploy-0.1.33/src/lib/LibRainDeploySnapshot.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {DeployDeclaration, DeploySuite} from "rain-deploy-0.1.33/src/abstract/RainDeploySuitesBase.sol";
import {LibCodeGen} from "rain-sol-codegen-0.1.39/src/lib/LibCodeGen.sol";
import {LibFs} from "rain-sol-codegen-0.1.39/src/lib/LibFs.sol";
import {LibGenParseMeta} from "rainlang-0.2.36/src/lib/codegen/LibGenParseMeta.sol";
import {
    RaindexDeploySuites,
    RAINDEX_SUITE,
    SUB_PARSER_SUITE,
    ROUTE_PROCESSOR_SUITE,
    GENERIC_POOL_ARB_ORDER_TAKER_SUITE,
    ROUTE_PROCESSOR_ARB_ORDER_TAKER_SUITE,
    GENERIC_POOL_FLASH_BORROWER_SUITE
} from "../src/abstract/RaindexDeploySuites.sol";
import {RaindexV7SubParser} from "../src/concrete/parser/RaindexV7SubParser.sol";
import {LibRaindexSubParser, EXTERN_PARSE_META_BUILD_DEPTH} from "../src/lib/LibRaindexSubParser.sol";

/// @title Build
/// @notice Generates the deploy pins for every contract this repo deploys.
///
/// `run()` (what CI regenerates against) rewrites the rolling
/// `src/generated/candidate/` snapshots, the sub-parser's non-deploy codegen
/// and the released-suites libs. `cutRelease()` freezes the candidates into
/// `src/generated/<tag>/` first. Frozen snapshots are append-only historical
/// records, never regenerated here.
///
/// The V6 contracts are retired: their records were cut under the V6 names and
/// stay released under the V7 suite keys.
///
/// There are no generated alias libs: `src/lib/deploy/LibRaindexDeploy.sol` is
/// hand-written over the candidate snapshots because its constant names are
/// consumed across this repo and its crates.
contract Build is BuildScript, RaindexDeploySuites {
    /// @inheritdoc BuildScript
    function generatedContracts(DeployDeclaration memory d)
        internal
        pure
        override
        returns (GeneratedContract[] memory)
    {
        GeneratedContract[] memory contracts = new GeneratedContract[](6);
        contracts[0] = GeneratedContract({
            contractName: "RaindexV7", constantPrefix: "", candidate: declaredCandidate(d, RAINDEX_SUITE)
        });
        contracts[1] = GeneratedContract({
            contractName: "RaindexV7SubParser", constantPrefix: "", candidate: declaredCandidate(d, SUB_PARSER_SUITE)
        });
        contracts[2] = GeneratedContract({
            contractName: "RouteProcessor4", constantPrefix: "", candidate: declaredCandidate(d, ROUTE_PROCESSOR_SUITE)
        });
        contracts[3] = GeneratedContract({
            contractName: "GenericPoolRaindexV7ArbOrderTaker",
            constantPrefix: "",
            candidate: declaredCandidate(d, GENERIC_POOL_ARB_ORDER_TAKER_SUITE)
        });
        contracts[4] = GeneratedContract({
            contractName: "RouteProcessorRaindexV7ArbOrderTaker",
            constantPrefix: "",
            candidate: declaredCandidate(d, ROUTE_PROCESSOR_ARB_ORDER_TAKER_SUITE)
        });
        contracts[5] = GeneratedContract({
            contractName: "GenericPoolRaindexV7FlashBorrower",
            constantPrefix: "",
            candidate: declaredCandidate(d, GENERIC_POOL_FLASH_BORROWER_SUITE)
        });
        return contracts;
    }

    /// @inheritdoc BuildScript
    function retiredContracts() internal pure override returns (RetiredContract[] memory) {
        RetiredContract[] memory retired = new RetiredContract[](5);
        retired[0] =
            retiredContract("RaindexV6", raindexCandidate().snapshot, "src/concrete/raindex/RaindexV6.sol:RaindexV6");
        retired[1] = retiredContract(
            "RaindexV6SubParser",
            subParserCandidate().snapshot,
            "src/concrete/parser/RaindexV6SubParser.sol:RaindexV6SubParser"
        );
        retired[2] = retiredContract(
            "GenericPoolRaindexV6ArbOrderTaker",
            genericPoolArbOrderTakerCandidate().snapshot,
            "src/concrete/arb/GenericPoolRaindexV6ArbOrderTaker.sol:GenericPoolRaindexV6ArbOrderTaker"
        );
        retired[3] = retiredContract(
            "RouteProcessorRaindexV6ArbOrderTaker",
            routeProcessorArbOrderTakerCandidate().snapshot,
            "src/concrete/arb/RouteProcessorRaindexV6ArbOrderTaker.sol:RouteProcessorRaindexV6ArbOrderTaker"
        );
        retired[4] = retiredContract(
            "GenericPoolRaindexV6FlashBorrower",
            genericPoolFlashBorrowerCandidate().snapshot,
            "src/concrete/arb/GenericPoolRaindexV6FlashBorrower.sol:GenericPoolRaindexV6FlashBorrower"
        );
        return retired;
    }

    /// A record cut as `contractName` from `formerArtifactPath`, released under
    /// `template`'s suite key.
    /// @param contractName The former contract name.
    /// @param template The current candidate's snapshot it was renamed to.
    /// @param formerArtifactPath The artifact path the record was cut with.
    /// @return The retired contract.
    function retiredContract(string memory contractName, DeploySuite memory template, string memory formerArtifactPath)
        internal
        pure
        returns (RetiredContract memory)
    {
        template.artifactPath = formerArtifactPath;
        return RetiredContract({contractName: contractName, template: template});
    }

    /// @inheritdoc BuildScript
    /// @dev An unanchorable candidate's creation code is its declared source;
    /// every other is read from its artifact.
    function regenerateSnapshots() internal override {
        GeneratedContract[] memory contracts = checkedGeneratedContracts(declaration());
        for (uint256 i = 0; i < contracts.length; i++) {
            LibRainDeploySnapshot.writeSnapshot(
                vm,
                recordRoot(),
                LibRainDeploySnapshot.CANDIDATE,
                contracts[i].contractName,
                bytes(contracts[i].candidate.unanchorableReason).length == 0
                    ? vm.getCode(contracts[i].candidate.snapshot.artifactPath)
                    : sourceCreationCode(contracts[i].candidate.snapshot.suite),
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
