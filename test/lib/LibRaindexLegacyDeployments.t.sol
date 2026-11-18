// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {DeploySuite} from "rain-deploy-0.1.33/src/abstract/RainDeploySuitesBase.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {LibRaindexLegacyDeployments, RaindexDeployment} from "../../src/lib/LibRaindexLegacyDeployments.sol";
import {LibReleasedSuites} from "../../src/lib/LibReleasedSuites.sol";
import {LibRaindexV6Released} from "../../src/lib/LibRaindexV6Released.sol";
import {LibRaindexV7Released} from "../../src/lib/LibRaindexV7Released.sol";
import {LibRaindexV7StartBlock} from "../../src/lib/deploy/LibRaindexV7StartBlock.sol";
import {
    LibRaindexDeploymentsRecord,
    ReleasedStartBlock,
    NoReleasedStartBlock
} from "../../script/lib/LibRaindexDeploymentsRecord.sol";

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

    function same(string memory a, string memory b) internal pure returns (bool) {
        return keccak256(bytes(a)) == keccak256(bytes(b));
    }

    /// The row for `addr` on `network`, or a zero row when there is none.
    function rowFor(RaindexDeployment[] memory records, string memory network, address addr)
        internal
        pure
        returns (RaindexDeployment memory)
    {
        for (uint256 i = 0; i < records.length; i++) {
            if (same(records[i].network, network) && records[i].deployedAddress == addr) {
                return records[i];
            }
        }
        RaindexDeployment memory none;
        return none;
    }

    function testLegacyOrderedByNetworkThenBlock() external pure {
        RaindexDeployment[] memory records = LibRaindexLegacyDeployments.deployments();
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

    function testLegacyNonZeroAddressBlockAndVersionInRange() external pure {
        RaindexDeployment[] memory records = LibRaindexLegacyDeployments.deployments();
        for (uint256 i = 0; i < records.length; i++) {
            assertNotEq(records[i].deployedAddress, address(0), string.concat("zero address at ", vm.toString(i)));
            assertNotEq(records[i].blockNumber, 0, string.concat("zero block at ", vm.toString(i)));
            assertGe(records[i].interfaceVersion, 3, string.concat("version below 3 at ", vm.toString(i)));
            assertLe(records[i].interfaceVersion, 7, string.concat("version above 7 at ", vm.toString(i)));
        }
    }

    /// The legacy table holds only what no release records: a released
    /// address listed here would be a second source of truth for it.
    function testLegacyExcludesReleasedAddresses() external pure {
        RaindexDeployment[] memory records = LibRaindexLegacyDeployments.deployments();
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

    /// Composed: no (network, address) pair appears twice.
    function testDeploymentsNetworkAddressPairsUnique() external view {
        RaindexDeployment[] memory records = LibRaindexDeploymentsRecord.deployments(vm);
        for (uint256 i = 0; i < records.length; i++) {
            for (uint256 j = i + 1; j < records.length; j++) {
                assertFalse(
                    same(records[i].network, records[j].network)
                        && records[i].deployedAddress == records[j].deployedAddress,
                    string.concat("duplicate pair at ", vm.toString(i), " and ", vm.toString(j))
                );
            }
        }
    }

    /// Composed: every released raindex is present, at its version and with a
    /// non-zero recorded block, on every network this repo deploys to. A
    /// release whose blocks were not recorded fails here, naming the gap.
    function testDeploymentsIncludeEveryReleasedRaindex() external view {
        RaindexDeployment[] memory records = LibRaindexDeploymentsRecord.deployments(vm);
        string[] memory networks = LibRainDeploy.supportedNetworks();
        DeploySuite[] memory v6 = LibRaindexV6Released.releasedSuites();
        DeploySuite[] memory v7 = LibRaindexV7Released.releasedSuites();
        for (uint256 n = 0; n < networks.length; n++) {
            for (uint256 i = 0; i < v6.length; i++) {
                RaindexDeployment memory r = rowFor(records, networks[n], v6[i].storedDeployedAddress);
                assertEq(r.interfaceVersion, 6, string.concat("released ", v6[i].suite, " on ", networks[n]));
                assertNotEq(r.blockNumber, 0, string.concat("released ", v6[i].suite, " block on ", networks[n]));
            }
            for (uint256 i = 0; i < v7.length; i++) {
                RaindexDeployment memory r = rowFor(records, networks[n], v7[i].storedDeployedAddress);
                assertEq(r.interfaceVersion, 7, string.concat("released ", v7[i].suite, " on ", networks[n]));
                assertNotEq(r.blockNumber, 0, string.concat("released ", v7[i].suite, " block on ", networks[n]));
            }
        }
    }

    /// `LibRaindexV7StartBlock` and the released start-block record are written
    /// by the same release run; they must agree on the newest released V7.
    function testV7StartBlockConstantsAgreeWithTheRecord() external view {
        ReleasedStartBlock[] memory blocks = LibRaindexDeploymentsRecord.releasedStartBlocks(vm);
        DeploySuite[] memory v7 = LibRaindexV7Released.releasedSuites();
        address newest = v7[v7.length - 1].storedDeployedAddress;
        assertEq(
            LibRaindexDeploymentsRecord.startBlockOf(blocks, newest, LibRainDeploy.ARBITRUM_ONE),
            LibRaindexV7StartBlock.RAINDEX_START_BLOCK_ARBITRUM
        );
        assertEq(
            LibRaindexDeploymentsRecord.startBlockOf(blocks, newest, LibRainDeploy.BASE),
            LibRaindexV7StartBlock.RAINDEX_START_BLOCK_BASE
        );
        assertEq(
            LibRaindexDeploymentsRecord.startBlockOf(blocks, newest, LibRainDeploy.FLARE),
            LibRaindexV7StartBlock.RAINDEX_START_BLOCK_FLARE
        );
        assertEq(
            LibRaindexDeploymentsRecord.startBlockOf(blocks, newest, LibRainDeploy.POLYGON),
            LibRaindexV7StartBlock.RAINDEX_START_BLOCK_POLYGON
        );
    }

    /// A released raindex with no recorded block is an error naming it, never
    /// a row with a guessed or zero block.
    function testMissingReleasedStartBlockReverts() external {
        ReleasedStartBlock[] memory blocks = new ReleasedStartBlock[](1);
        blocks[0] = ReleasedStartBlock({deployedAddress: address(0xA), network: "base", startBlock: 7});
        assertEq(this.externalStartBlockOf(blocks, address(0xA), "base"), 7);
        vm.expectRevert(abi.encodeWithSelector(NoReleasedStartBlock.selector, address(0xA), "flare"));
        this.externalStartBlockOf(blocks, address(0xA), "flare");
        vm.expectRevert(abi.encodeWithSelector(NoReleasedStartBlock.selector, address(0xB), "base"));
        this.externalStartBlockOf(blocks, address(0xB), "base");
    }

    function externalStartBlockOf(ReleasedStartBlock[] memory blocks, address addr, string memory network)
        external
        pure
        returns (uint256)
    {
        return LibRaindexDeploymentsRecord.startBlockOf(blocks, addr, network);
    }
}
