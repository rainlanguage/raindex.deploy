// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {DeploySuite} from "rain-deploy-0.1.33/src/abstract/RainDeploySuitesBase.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {LibBlockSearch} from "rain-deploy-0.1.33/src/lib/LibBlockSearch.sol";
import {LibRaindexV7StartBlock} from "src/lib/deploy/LibRaindexV7StartBlock.sol";
import {LibRaindexV7NewestRelease} from "test/util/lib/LibRaindexV7NewestRelease.sol";

/// @title LibRaindexV7StartBlockIsStartBlockPolygonTest
/// @notice The newest released RaindexV7 has code at the Polygon start block but
/// not the block before.
contract LibRaindexV7StartBlockIsStartBlockPolygonTest is Test {
    function testIsStartBlockPolygon() external {
        DeploySuite memory released = LibRaindexV7NewestRelease.suite();
        vm.createSelectFork(LibRainDeploy.POLYGON);
        assertTrue(
            LibBlockSearch.isStartBlock(
                vm,
                released.storedDeployedAddress,
                released.storedBytecodeHash,
                LibRaindexV7StartBlock.RAINDEX_START_BLOCK_POLYGON
            ),
            "not start block: Polygon"
        );
    }
}
