// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {
    DeployCandidate,
    DeployDependency,
    DeployMechanism,
    DeploySuite,
    RainDeploySuitesBase
} from "./RainDeploySuitesBase.sol";
import {LibRaindexDeploy} from "../lib/deploy/LibRaindexDeploy.sol";
import {LibReleasedSuites} from "../lib/LibReleasedSuites.sol";
import {
    DEPLOYED_ADDRESS as TOFU_DECIMALS_ADDRESS,
    RUNTIME_CODE as TOFU_DECIMALS_RUNTIME_CODE
} from "rain-tofu-erc20-decimals-deploy-0.1.4/src/generated/0_1_0/TOFUTokenDecimals.sol";
import {
    CREATION_CODE as RAINDEX_CREATION_CODE_CANDIDATE,
    RUNTIME_CODE as RAINDEX_RUNTIME_CODE_CANDIDATE
} from "../generated/candidate/RaindexV7.sol";
import {
    CREATION_CODE as SUB_PARSER_CREATION_CODE_CANDIDATE,
    RUNTIME_CODE as SUB_PARSER_RUNTIME_CODE_CANDIDATE
} from "../generated/candidate/RaindexV7SubParser.sol";
import {
    CREATION_CODE as ROUTE_PROCESSOR_CREATION_CODE_CANDIDATE,
    RUNTIME_CODE as ROUTE_PROCESSOR_RUNTIME_CODE_CANDIDATE
} from "../generated/candidate/RouteProcessor4.sol";
import {
    CREATION_CODE as GENERIC_POOL_ARB_OT_CREATION_CODE_CANDIDATE,
    RUNTIME_CODE as GENERIC_POOL_ARB_OT_RUNTIME_CODE_CANDIDATE
} from "../generated/candidate/GenericPoolRaindexV7ArbOrderTaker.sol";
import {
    CREATION_CODE as RP_ARB_OT_CREATION_CODE_CANDIDATE,
    RUNTIME_CODE as RP_ARB_OT_RUNTIME_CODE_CANDIDATE
} from "../generated/candidate/RouteProcessorRaindexV7ArbOrderTaker.sol";
import {
    CREATION_CODE as GENERIC_POOL_FB_CREATION_CODE_CANDIDATE,
    RUNTIME_CODE as GENERIC_POOL_FB_RUNTIME_CODE_CANDIDATE
} from "../generated/candidate/GenericPoolRaindexV7FlashBorrower.sol";

/// @title RaindexDeploySuites
/// @notice Everything this repo deploys, declared ONCE: the six hand-written
/// candidates below, and the released side read from the generated
/// `LibReleasedSuites`, which `script/Build.sol` emits from the frozen record.
///
/// The suite keys are the `Manual sol artifacts` dispatch choices and MUST
/// stay in step with `.github/workflows/manual-sol-artifacts.yaml`.
///
/// It lives in `src/` rather than `test/` because `.soldeerignore` excludes
/// `test/` from the published package, and the deployment process is part of
/// the product.
abstract contract RaindexDeploySuites is RainDeploySuitesBase {
    /// @inheritdoc RainDeploySuitesBase
    function releasedSuites() internal pure override returns (DeploySuite[] memory) {
        return LibReleasedSuites.releasedSuites();
    }

    /// @inheritdoc RainDeploySuitesBase
    function candidateSuites() internal pure override returns (DeployCandidate[] memory) {
        DeployCandidate[] memory candidates = new DeployCandidate[](6);
        candidates[0] = raindexCandidate();
        candidates[1] = subParserCandidate();
        candidates[2] = routeProcessorCandidate();
        candidates[3] = genericPoolArbOrderTakerCandidate();
        candidates[4] = routeProcessorArbOrderTakerCandidate();
        candidates[5] = genericPoolFlashBorrowerCandidate();
        return candidates;
    }

    /// The TOFU decimals reader, which `RaindexV7` and every arb read token
    /// decimals through.
    /// @return The dependency.
    function tofuDecimalsDependency() internal pure returns (DeployDependency memory) {
        return DeployDependency({deployedAddress: TOFU_DECIMALS_ADDRESS, runtimeCode: TOFU_DECIMALS_RUNTIME_CODE});
    }

    /// The candidate `RaindexV7`, the only raindex every arb accepts.
    /// @return The dependency.
    function raindexDependency() internal pure returns (DeployDependency memory) {
        return DeployDependency({
            deployedAddress: LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, runtimeCode: RAINDEX_RUNTIME_CODE_CANDIDATE
        });
    }

    /// The code that must already be on a network before an arb can be
    /// broadcast there: the raindex it takes orders from and the TOFU
    /// decimals reader.
    /// @return The dependencies.
    function arbDependencies() internal pure returns (DeployDependency[] memory) {
        DeployDependency[] memory deps = new DeployDependency[](2);
        deps[0] = raindexDependency();
        deps[1] = tofuDecimalsDependency();
        return deps;
    }

    /// `arbDependencies` plus the `RouteProcessor4` candidate the arb swaps
    /// through.
    /// @return The dependencies.
    function routeProcessorArbDependencies() internal pure returns (DeployDependency[] memory) {
        DeployDependency[] memory deps = new DeployDependency[](3);
        deps[0] = raindexDependency();
        deps[1] = tofuDecimalsDependency();
        deps[2] = DeployDependency({
            deployedAddress: LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS,
            runtimeCode: ROUTE_PROCESSOR_RUNTIME_CODE_CANDIDATE
        });
        return deps;
    }

    /// The code that must already be on a network before `RaindexV7` can be
    /// broadcast there: the TOFU decimals reader.
    /// @return The dependencies.
    function raindexDependencies() internal pure returns (DeployDependency[] memory) {
        DeployDependency[] memory deps = new DeployDependency[](1);
        deps[0] = tofuDecimalsDependency();
        return deps;
    }

    /// This repo's rolling `RaindexV7` candidate. Each candidate is a named
    /// function rather than an index into `candidateSuites`, because
    /// `script/Build.sol` emits snapshots from these candidates specifically,
    /// and because six full creation codes built in one frame do not fit the
    /// legacy codegen's stack.
    /// @return The candidate.
    function raindexCandidate() internal pure returns (DeployCandidate memory) {
        return DeployCandidate({
            snapshot: DeploySuite({
                suite: "raindex",
                creationCode: RAINDEX_CREATION_CODE_CANDIDATE,
                storedDeployedAddress: LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS,
                storedBytecodeHash: LibRaindexDeploy.RAINDEX_DEPLOYED_CODEHASH,
                storedRuntimeCode: RAINDEX_RUNTIME_CODE_CANDIDATE,
                artifactPath: "src/concrete/raindex/RaindexV7.sol:RaindexV7",
                dependencies: raindexDependencies(),
                mechanism: DeployMechanism.Zoltu,
                mechanismData: ""
            }),
            unanchorableReason: ""
        });
    }

    /// This repo's rolling `RaindexV7SubParser` candidate.
    /// @return The candidate.
    function subParserCandidate() internal pure returns (DeployCandidate memory) {
        return DeployCandidate({
            snapshot: DeploySuite({
                suite: "subparser",
                creationCode: SUB_PARSER_CREATION_CODE_CANDIDATE,
                storedDeployedAddress: LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS,
                storedBytecodeHash: LibRaindexDeploy.SUB_PARSER_DEPLOYED_CODEHASH,
                storedRuntimeCode: SUB_PARSER_RUNTIME_CODE_CANDIDATE,
                artifactPath: "src/concrete/parser/RaindexV7SubParser.sol:RaindexV7SubParser",
                dependencies: new DeployDependency[](0),
                mechanism: DeployMechanism.Zoltu,
                mechanismData: ""
            }),
            unanchorableReason: ""
        });
    }

    /// This repo's rolling `RouteProcessor4` candidate. There is no local
    /// source for RouteProcessor4 — its creation code is the pinned
    /// `ROUTE_PROCESSOR_4_CREATION_CODE` constant — so it declares an
    /// `unanchorableReason`.
    /// @return The candidate.
    function routeProcessorCandidate() internal pure returns (DeployCandidate memory) {
        return DeployCandidate({
            snapshot: DeploySuite({
                suite: "route-processor",
                creationCode: ROUTE_PROCESSOR_CREATION_CODE_CANDIDATE,
                storedDeployedAddress: LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS,
                storedBytecodeHash: LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_CODEHASH,
                storedRuntimeCode: ROUTE_PROCESSOR_RUNTIME_CODE_CANDIDATE,
                artifactPath: "src/lib/deploy/LibRouteProcessor4CreationCode.sol:RouteProcessor4",
                dependencies: new DeployDependency[](0),
                mechanism: DeployMechanism.Zoltu,
                mechanismData: ""
            }),
            unanchorableReason: "A vendored third-party deployment, pinned as its creation code; no local source compiles to it."
        });
    }

    /// This repo's rolling `GenericPoolRaindexV7ArbOrderTaker` candidate.
    /// @return The candidate.
    function genericPoolArbOrderTakerCandidate() internal pure returns (DeployCandidate memory) {
        return DeployCandidate({
            snapshot: DeploySuite({
                suite: "arb-generic-pool-order-taker",
                creationCode: GENERIC_POOL_ARB_OT_CREATION_CODE_CANDIDATE,
                storedDeployedAddress: LibRaindexDeploy.GENERIC_POOL_ARB_ORDER_TAKER_DEPLOYED_ADDRESS,
                storedBytecodeHash: LibRaindexDeploy.GENERIC_POOL_ARB_ORDER_TAKER_DEPLOYED_CODEHASH,
                storedRuntimeCode: GENERIC_POOL_ARB_OT_RUNTIME_CODE_CANDIDATE,
                artifactPath: "src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol:GenericPoolRaindexV7ArbOrderTaker",
                dependencies: arbDependencies(),
                mechanism: DeployMechanism.Zoltu,
                mechanismData: ""
            }),
            unanchorableReason: ""
        });
    }

    /// This repo's rolling `RouteProcessorRaindexV7ArbOrderTaker` candidate.
    /// @return The candidate.
    function routeProcessorArbOrderTakerCandidate() internal pure returns (DeployCandidate memory) {
        return DeployCandidate({
            snapshot: DeploySuite({
                suite: "arb-route-processor-order-taker",
                creationCode: RP_ARB_OT_CREATION_CODE_CANDIDATE,
                storedDeployedAddress: LibRaindexDeploy.ROUTE_PROCESSOR_ARB_ORDER_TAKER_DEPLOYED_ADDRESS,
                storedBytecodeHash: LibRaindexDeploy.ROUTE_PROCESSOR_ARB_ORDER_TAKER_DEPLOYED_CODEHASH,
                storedRuntimeCode: RP_ARB_OT_RUNTIME_CODE_CANDIDATE,
                artifactPath: "src/concrete/arb/RouteProcessorRaindexV7ArbOrderTaker.sol:RouteProcessorRaindexV7ArbOrderTaker",
                dependencies: routeProcessorArbDependencies(),
                mechanism: DeployMechanism.Zoltu,
                mechanismData: ""
            }),
            unanchorableReason: ""
        });
    }

    /// This repo's rolling `GenericPoolRaindexV7FlashBorrower` candidate.
    /// @return The candidate.
    function genericPoolFlashBorrowerCandidate() internal pure returns (DeployCandidate memory) {
        return DeployCandidate({
            snapshot: DeploySuite({
                suite: "arb-generic-pool-flash-borrower",
                creationCode: GENERIC_POOL_FB_CREATION_CODE_CANDIDATE,
                storedDeployedAddress: LibRaindexDeploy.GENERIC_POOL_FLASH_BORROWER_DEPLOYED_ADDRESS,
                storedBytecodeHash: LibRaindexDeploy.GENERIC_POOL_FLASH_BORROWER_DEPLOYED_CODEHASH,
                storedRuntimeCode: GENERIC_POOL_FB_RUNTIME_CODE_CANDIDATE,
                artifactPath: "src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol:GenericPoolRaindexV7FlashBorrower",
                dependencies: arbDependencies(),
                mechanism: DeployMechanism.Zoltu,
                mechanismData: ""
            }),
            unanchorableReason: ""
        });
    }
}
