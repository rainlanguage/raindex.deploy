// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibRaindexV7NewestRelease} from "test/util/lib/LibRaindexV7NewestRelease.sol";

/// @title SubgraphYamlReleasedAddressTest
/// @notice The data source address in subgraph/subgraph.yaml is the newest
/// released RaindexV7.
contract SubgraphYamlReleasedAddressTest is Test {
    function testSubgraphYamlReleasedAddress() external {
        string[] memory inputs = new string[](3);
        inputs[0] = "yq";
        inputs[1] = ".dataSources[0].source.address";
        inputs[2] = "subgraph/subgraph.yaml";
        bytes memory result = vm.ffi(inputs);
        // yq prints the 20-byte address this assertion compares.
        // forge-lint: disable-next-line(unsafe-typecast)
        address addr = address(bytes20(result));
        assertEq(addr, LibRaindexV7NewestRelease.suite().storedDeployedAddress, "subgraph.yaml address mismatch");
    }
}
