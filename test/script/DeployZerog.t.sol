// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibRainDeploy} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {DeployHarness} from "test/util/concrete/DeployHarness.sol";
import {DeployZerogHarness} from "test/util/concrete/DeployZerogHarness.sol";

contract DeployZerogTest is Test {
    /// `DeployZerog` broadcasts to `zerog` only, and declares the same
    /// networks and suites as `Deploy`, which broadcasts to every network.
    function testDeployZerogBroadcastsToZerogOnly() external {
        DeployHarness all = new DeployHarness();
        DeployZerogHarness zerog = new DeployZerogHarness();

        string[] memory deployNetworks = zerog.externalDeployNetworks();
        assertEq(deployNetworks.length, 1);
        assertEq(deployNetworks[0], LibRainDeploy.ZEROG);

        assertEq(all.externalDeployNetworks(), all.externalNetworks());
        assertEq(zerog.externalNetworks(), all.externalNetworks());
        assertEq(zerog.externalSuitesHash(), all.externalSuitesHash());
    }
}
