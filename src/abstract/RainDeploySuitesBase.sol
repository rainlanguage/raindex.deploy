// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

// Re-exports the deploy-suite declaration from the pinned `rain-deploy`
// package under this repo's own `src/abstract/` path: the generated libs
// under `src/lib/` import `../abstract/RainDeploySuitesBase.sol` — the
// writers emit that relative path, and relative imports never go through
// remappings — so this file makes the path resolve, and holds the one local
// spelling of the package path.
import {
    DeployCandidate,
    DeployDependency,
    DeployMechanism,
    DeploySuite,
    RainDeploySuitesBase
} from "rain-deploy-0.1.33/src/abstract/RainDeploySuitesBase.sol";
