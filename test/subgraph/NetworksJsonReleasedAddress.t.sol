// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibRaindexV7NewestRelease} from "test/util/lib/LibRaindexV7NewestRelease.sol";

/// @title NetworksJsonReleasedAddressTest
/// @notice Every address in subgraph/networks.json is the newest released
/// RaindexV7.
contract NetworksJsonReleasedAddressTest is Test {
    function testNetworksJsonReleasedAddress() external view {
        address released = LibRaindexV7NewestRelease.suite().storedDeployedAddress;
        string memory json = vm.readFile("subgraph/networks.json");
        string[] memory networks = vm.parseJsonKeys(json, "$");
        for (uint256 i = 0; i < networks.length; i++) {
            string memory path = string.concat(".", networks[i], ".Raindex.address");
            address addr = vm.parseJsonAddress(json, path);
            assertEq(addr, released, string.concat("networks.json address mismatch: ", networks[i]));
        }
    }
}
