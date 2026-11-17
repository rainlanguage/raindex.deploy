// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {DeploySuite} from "rain-deploy-0.1.25/src/abstract/RainDeploySuitesBase.sol";
import {
    LibRaindexLegacyDeployments,
    RaindexLegacyDeployment
} from "../../src/lib/LibRaindexLegacyDeployments.sol";
import {LibReleasedSuites} from "../../src/lib/LibReleasedSuites.sol";

contract LibRaindexLegacyDeploymentsTest is Test {
    function networkOrder(string memory a, string memory b) internal pure returns (int256) {
        bytes memory left = bytes(a);
        bytes memory right = bytes(b);
        uint256 length = left.length < right.length ? left.length : right.length;
        for (uint256 i = 0; i < length; i++) {
            if (left[i] != right[i]) {
                return uint8(left[i]) < uint8(right[i]) ? -1 : int256(1);
            }
        }
        if (left.length == right.length) {
            return 0;
        }
        return left.length < right.length ? -1 : int256(1);
    }

    /// No (network, deployedAddress) pair is recorded twice.
    function testLegacyDeploymentsNetworkAddressPairsUnique() external pure {
        RaindexLegacyDeployment[] memory records = LibRaindexLegacyDeployments.deployments();
        for (uint256 i = 0; i < records.length; i++) {
            for (uint256 j = i + 1; j < records.length; j++) {
                assertFalse(
                    keccak256(bytes(records[i].network)) == keccak256(bytes(records[j].network))
                        && records[i].deployedAddress == records[j].deployedAddress,
                    string.concat("duplicate pair at ", vm.toString(i), " and ", vm.toString(j))
                );
            }
        }
    }

    /// Records are ordered by network, then by block within a network.
    function testLegacyDeploymentsOrderedByNetworkThenBlock() external pure {
        RaindexLegacyDeployment[] memory records = LibRaindexLegacyDeployments.deployments();
        for (uint256 i = 1; i < records.length; i++) {
            int256 order = networkOrder(records[i - 1].network, records[i].network);
            assertLe(order, 0, string.concat("network out of order at ", vm.toString(i)));
            if (order == 0) {
                assertLe(
                    records[i - 1].blockNumber,
                    records[i].blockNumber,
                    string.concat("block out of order at ", vm.toString(i))
                );
            }
        }
    }

    /// Every record has a nonzero address and a nonzero block.
    function testLegacyDeploymentsNonZeroAddressAndBlock() external pure {
        RaindexLegacyDeployment[] memory records = LibRaindexLegacyDeployments.deployments();
        for (uint256 i = 0; i < records.length; i++) {
            assertNotEq(records[i].deployedAddress, address(0), string.concat("zero address at ", vm.toString(i)));
            assertNotEq(records[i].blockNumber, 0, string.concat("zero block at ", vm.toString(i)));
        }
    }

    /// No legacy address is the deployed address of any released suite.
    function testLegacyDeploymentsExcludeReleasedAddresses() external pure {
        RaindexLegacyDeployment[] memory records = LibRaindexLegacyDeployments.deployments();
        DeploySuite[] memory released = LibReleasedSuites.releasedSuites();
        for (uint256 i = 0; i < records.length; i++) {
            for (uint256 j = 0; j < released.length; j++) {
                assertNotEq(
                    records[i].deployedAddress,
                    released[j].storedDeployedAddress,
                    string.concat("legacy ", vm.toString(i), " is released ", released[j].suite)
                );
            }
        }
    }
}
