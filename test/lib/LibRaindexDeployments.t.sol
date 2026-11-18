// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {DeploySuite} from "rain-deploy-0.1.33/src/abstract/RainDeploySuitesBase.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {LibRaindexDeployments, RaindexDeployment} from "../../src/lib/LibRaindexDeployments.sol";
import {LibRaindexV6Released} from "../../src/lib/LibRaindexV6Released.sol";
import {LibRaindexV7Released} from "../../src/lib/LibRaindexV7Released.sol";

contract LibRaindexDeploymentsTest is Test {
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

    function testDeploymentsNetworkAddressPairsUnique() external pure {
        RaindexDeployment[] memory records = LibRaindexDeployments.deployments();
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

    function testDeploymentsOrderedByNetworkThenBlock() external pure {
        RaindexDeployment[] memory records = LibRaindexDeployments.deployments();
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

    function testDeploymentsNonZeroAddressAndBlock() external pure {
        RaindexDeployment[] memory records = LibRaindexDeployments.deployments();
        for (uint256 i = 0; i < records.length; i++) {
            assertNotEq(records[i].deployedAddress, address(0), string.concat("zero address at ", vm.toString(i)));
            assertNotEq(records[i].blockNumber, 0, string.concat("zero block at ", vm.toString(i)));
        }
    }

    function testDeploymentsInterfaceVersionInRange() external pure {
        RaindexDeployment[] memory records = LibRaindexDeployments.deployments();
        for (uint256 i = 0; i < records.length; i++) {
            assertGe(records[i].interfaceVersion, 3, string.concat("version below 3 at ", vm.toString(i)));
            assertLe(records[i].interfaceVersion, 7, string.concat("version above 7 at ", vm.toString(i)));
        }
    }

    /// The row for `addr` on `network`, or a zero row when there is none.
    function rowFor(RaindexDeployment[] memory records, string memory network, address addr)
        internal
        pure
        returns (RaindexDeployment memory)
    {
        for (uint256 i = 0; i < records.length; i++) {
            if (keccak256(bytes(records[i].network)) == keccak256(bytes(network)) && records[i].deployedAddress == addr)
            {
                return records[i];
            }
        }
        RaindexDeployment memory none;
        return none;
    }

    /// Every released raindex is recorded, at its version, on every network
    /// this repo deploys to.
    function testDeploymentsIncludeEveryReleasedRaindex() external pure {
        RaindexDeployment[] memory records = LibRaindexDeployments.deployments();
        string[] memory networks = LibRainDeploy.supportedNetworks();
        DeploySuite[] memory v6 = LibRaindexV6Released.releasedSuites();
        DeploySuite[] memory v7 = LibRaindexV7Released.releasedSuites();
        for (uint256 n = 0; n < networks.length; n++) {
            for (uint256 i = 0; i < v6.length; i++) {
                RaindexDeployment memory r = rowFor(records, networks[n], v6[i].storedDeployedAddress);
                assertEq(r.interfaceVersion, 6, string.concat("released ", v6[i].suite, " on ", networks[n]));
            }
            for (uint256 i = 0; i < v7.length; i++) {
                RaindexDeployment memory r = rowFor(records, networks[n], v7[i].storedDeployedAddress);
                assertEq(r.interfaceVersion, 7, string.concat("released ", v7[i].suite, " on ", networks[n]));
            }
        }
    }
}
