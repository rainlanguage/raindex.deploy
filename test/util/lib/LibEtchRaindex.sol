// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Vm} from "forge-std-1.17.0/src/Vm.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {RUNTIME_CODE as RAINDEX_RUNTIME_CODE} from "../../../src/generated/candidate/RaindexV7.sol";
import {RUNTIME_CODE as SUB_PARSER_RUNTIME_CODE} from "../../../src/generated/candidate/RaindexV7SubParser.sol";
import {RUNTIME_CODE as ROUTE_PROCESSOR_RUNTIME_CODE} from "../../../src/generated/candidate/RouteProcessor4.sol";

/// @title LibEtchRaindex
/// @notice Etches the runtime bytecode of the raindex, sub parser, and
/// route processor at their expected deterministic addresses.
library LibEtchRaindex {
    /// @notice Etches the runtime bytecode of the raindex, sub parser, and
    /// route processor at their expected deterministic addresses. Skips any
    /// contract whose codehash already matches.
    /// @param vm The Forge `Vm` cheatcode interface.
    function etchRaindex(Vm vm) internal {
        if (LibRaindexDeploy.RAINDEX_DEPLOYED_CODEHASH != LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS.codehash) {
            vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, RAINDEX_RUNTIME_CODE);
        }
        if (LibRaindexDeploy.SUB_PARSER_DEPLOYED_CODEHASH != LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS.codehash) {
            vm.etch(LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS, SUB_PARSER_RUNTIME_CODE);
        }
        if (
            LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_CODEHASH
                != LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS.codehash
        ) {
            vm.etch(LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS, ROUTE_PROCESSOR_RUNTIME_CODE);
        }
    }
}
