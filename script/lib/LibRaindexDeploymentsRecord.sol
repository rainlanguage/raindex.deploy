// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {Vm} from "forge-std-1.17.0/src/Vm.sol";
import {DeploySuite} from "rain-deploy-0.1.38/src/abstract/RainDeploySuitesBase.sol";
import {LibRainDeploy} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {LibRaindexLegacyDeployments, RaindexDeployment} from "../../src/lib/LibRaindexLegacyDeployments.sol";

// Where the released raindexes' first blocks are recorded, one entry per
// (address, network). `script/build-start-blocks.sh` adds the newly released
// address on every network at each release.
string constant RELEASED_START_BLOCKS_JSON = "deployments/released-start-blocks.json";

/// A released raindex with no recorded first block on a supported network.
error NoReleasedStartBlock(address raindex, string network);

/// One released raindex's first block with code on one network. Fields are in
/// key order, which is how `vm.parseJson` decodes an object.
struct ReleasedStartBlock {
    address deployedAddress;
    string network;
    uint256 startBlock;
}

/// @title LibRaindexDeploymentsRecord
/// @notice Every raindex deployment, composed from the records that already
/// exist rather than listed again: the legacy table for deployments no release
/// records, the released-suites libs for released addresses (on every
/// supported network), and the released start-block record for their blocks.
library LibRaindexDeploymentsRecord {
    function releasedStartBlocks(Vm vm) internal view returns (ReleasedStartBlock[] memory) {
        return abi.decode(vm.parseJson(vm.readFile(RELEASED_START_BLOCKS_JSON)), (ReleasedStartBlock[]));
    }

    /// The recorded first block of `addr` on `network`, or reverts naming the
    /// missing entry: a released raindex with no block is a record to complete,
    /// never a row to drop or guess.
    function startBlockOf(ReleasedStartBlock[] memory blocks, address addr, string memory network)
        internal
        pure
        returns (uint256)
    {
        for (uint256 i = 0; i < blocks.length; i++) {
            if (blocks[i].deployedAddress == addr && keccak256(bytes(blocks[i].network)) == keccak256(bytes(network))) {
                return blocks[i].startBlock;
            }
        }
        revert NoReleasedStartBlock(addr, network);
    }

    /// Whether a release of `addr` among `suites` is declared on `network`. A
    /// suite with no networks of its own is declared on every supported one.
    function declaredOn(DeploySuite[] memory suites, address addr, string memory network) internal pure returns (bool) {
        for (uint256 i = 0; i < suites.length; i++) {
            if (suites[i].storedDeployedAddress != addr) continue;
            string[] memory nets = suites[i].networks;
            if (nets.length == 0) return true;
            for (uint256 j = 0; j < nets.length; j++) {
                if (keccak256(bytes(nets[j])) == keccak256(bytes(network))) return true;
            }
        }
        return false;
    }

    /// Every distinct released address of `suites`, at `version`, on each
    /// supported network a release of it is declared on.
    function released(
        DeploySuite[] memory suites,
        uint256 version,
        string[] memory networks,
        ReleasedStartBlock[] memory blocks
    ) internal pure returns (RaindexDeployment[] memory out) {
        address[] memory addrs = new address[](suites.length);
        uint256 n = 0;
        for (uint256 i = 0; i < suites.length; i++) {
            bool seen = false;
            for (uint256 j = 0; j < n; j++) {
                if (addrs[j] == suites[i].storedDeployedAddress) seen = true;
            }
            if (!seen) addrs[n++] = suites[i].storedDeployedAddress;
        }
        out = new RaindexDeployment[](n * networks.length);
        uint256 o = 0;
        for (uint256 i = 0; i < n; i++) {
            for (uint256 k = 0; k < networks.length; k++) {
                if (!declaredOn(suites, addrs[i], networks[k])) continue;
                out[o++] = RaindexDeployment({
                    network: networks[k],
                    deployedAddress: addrs[i],
                    interfaceVersion: version,
                    blockNumber: startBlockOf(blocks, addrs[i], networks[k])
                });
            }
        }
        assembly ("memory-safe") {
            mstore(out, o)
        }
    }

    /// The legacy table, then every released RaindexV6, then every released
    /// RaindexV7. The released suites carry the networks each release is
    /// declared on (`RaindexDeploySuites.narrowReleasedNetworks`).
    function deployments(Vm vm, DeploySuite[] memory v6Suites, DeploySuite[] memory v7Suites)
        internal
        view
        returns (RaindexDeployment[] memory all)
    {
        RaindexDeployment[] memory legacy = LibRaindexLegacyDeployments.deployments();
        ReleasedStartBlock[] memory blocks = releasedStartBlocks(vm);
        string[] memory networks = LibRainDeploy.supportedNetworks();
        RaindexDeployment[] memory v6 = released(v6Suites, 6, networks, blocks);
        RaindexDeployment[] memory v7 = released(v7Suites, 7, networks, blocks);
        all = new RaindexDeployment[](legacy.length + v6.length + v7.length);
        uint256 o = 0;
        for (uint256 i = 0; i < legacy.length; i++) {
            all[o++] = legacy[i];
        }
        for (uint256 i = 0; i < v6.length; i++) {
            all[o++] = v6[i];
        }
        for (uint256 i = 0; i < v7.length; i++) {
            all[o++] = v7[i];
        }
    }
}
