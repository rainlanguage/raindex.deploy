// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Script} from "forge-std-1.17.0/src/Script.sol";
import {LibRaindexSubParser} from "../src/lib/LibRaindexSubParser.sol";

/// @title Raindex subparser Authoring Meta
/// @notice A script that writes the raw authoring meta out to file so it can be
/// wrapped in CBOR and emitted on metaboard.
contract BuildAuthoringMeta is Script {
    function run() external {
        vm.writeFileBinary("meta/RaindexV7SubParserAuthoringMeta.rain.meta", LibRaindexSubParser.authoringMetaV2());
    }
}
