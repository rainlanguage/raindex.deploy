// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibRaindexV7StartBlock} from "src/lib/deploy/LibRaindexV7StartBlock.sol";

/// @title NetworksJsonStartBlockFlareTest
/// @notice The flare startBlock in subgraph/networks.json is the RaindexV7
/// start block.
contract NetworksJsonStartBlockFlareTest is Test {
    function testNetworksJsonStartBlockFlare() external view {
        string memory json = vm.readFile("subgraph/networks.json");
        uint256 startBlock = vm.parseJsonUint(json, ".flare.Raindex.startBlock");
        assertEq(
            startBlock, LibRaindexV7StartBlock.RAINDEX_START_BLOCK_FLARE, "networks.json startBlock mismatch: flare"
        );
    }
}
