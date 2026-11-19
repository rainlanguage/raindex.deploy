// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";

/// @title RaindexV7FreshTakeOrderTest
/// @notice Base for take-order tests that must observe mutations to
/// `src/concrete/raindex/RaindexV7.sol`. `RaindexV7ExternalRealTest` etches the
/// COMMITTED `src/generated/candidate/RaindexV7.sol` runtime bytecode (via
/// `LibEtchRaindex`) at `RAINDEX_DEPLOYED_ADDRESS`, which is blind to source
/// edits. This base re-etches that address with the FRESH-COMPILED `RaindexV7`
/// artifact runtime code, so a mutation to the take/skip loop is live in
/// what the tests execute. Orders are still added/parsed against the real etched
/// interpreter/store/parser, exactly as in the committed suite.
abstract contract RaindexV7FreshTakeOrderTest is RaindexV7ExternalRealTest {
    constructor() {
        // Re-etch the raindex address with freshly compiled runtime code so
        // source mutations to RaindexV7 are observed by the tests below.
        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, vm.getDeployedCode("RaindexV7.sol:RaindexV7"));
    }
}
