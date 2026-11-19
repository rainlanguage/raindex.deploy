// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibRaindexV7StartBlock} from "src/lib/deploy/LibRaindexV7StartBlock.sol";

/// @title NetworksJsonStartBlockArbitrumTest
/// @notice The arbitrum-one startBlock in subgraph/networks.json is the RaindexV7
/// start block.
contract NetworksJsonStartBlockArbitrumTest is Test {
    function testNetworksJsonStartBlockArbitrum() external view {
        string memory json = vm.readFile("subgraph/networks.json");
        uint256 startBlock = vm.parseJsonUint(json, ".arbitrum-one.Raindex.startBlock");
        assertEq(
            startBlock,
            LibRaindexV7StartBlock.RAINDEX_START_BLOCK_ARBITRUM,
            "networks.json startBlock mismatch: arbitrum-one"
        );
    }
}
