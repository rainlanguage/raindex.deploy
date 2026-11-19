// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";

/// @title RaindexV7FreshTest
/// @notice Same external setup as `RaindexV7ExternalRealTest` but overlays the
/// FRESHLY COMPILED `RaindexV7` runtime bytecode at the deployed address rather
/// than the committed `src/generated/candidate/RaindexV7.sol` runtime code that
/// `LibEtchRaindex` etches. The committed candidate code is regenerated at the last
/// `Build.sol` run, so tests that run against it do not observe edits to
/// `src/concrete/raindex/RaindexV7.sol`. Overlaying the `RaindexV7` artifact runtime code
/// makes the live source the code under test.
abstract contract RaindexV7FreshTest is RaindexV7ExternalRealTest {
    constructor() {
        // Overlay the freshly compiled runtime over the pointer-etched runtime.
        // `RaindexV7` has no constructor or immutables, so its runtime code is a
        // standalone, fully functional deployment.
        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, vm.getDeployedCode("RaindexV7.sol:RaindexV7"));
    }
}
