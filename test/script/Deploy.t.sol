// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibRainDeploy} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {ZerogOnlySuite} from "script/Deploy.sol";
import {DeployHarness} from "test/util/concrete/DeployHarness.sol";

contract DeployTest is Test {
    /// `Deploy` refuses a suite that only `zerog` requires, candidate or
    /// release, on every other network, and allows it on `zerog`.
    function testPreDeployCheckRefusesZerogOnlySuitesElsewhere() external {
        DeployHarness deploy = new DeployHarness();
        string[3] memory suites =
            ["route-processor-six", "arb-route-processor-six-order-taker", "route-processor-six@0_1_19"];
        string[] memory networks = LibRainDeploy.supportedNetworks();
        for (uint256 i = 0; i < suites.length; i++) {
            for (uint256 j = 0; j < networks.length; j++) {
                if (keccak256(bytes(networks[j])) == keccak256(bytes(LibRainDeploy.ZEROG))) {
                    deploy.externalPreDeployCheck(suites[i], networks[j]);
                } else {
                    vm.expectRevert(abi.encodeWithSelector(ZerogOnlySuite.selector, suites[i], networks[j]));
                    deploy.externalPreDeployCheck(suites[i], networks[j]);
                }
            }
        }
    }

    /// `Deploy` allows every other suite on every network.
    function testPreDeployCheckAllowsOtherSuites() external {
        DeployHarness deploy = new DeployHarness();
        string[4] memory suites =
            ["raindex", "route-processor", "arb-route-processor-order-taker", "arb-route-processor-order-taker@0_1_15"];
        string[] memory networks = LibRainDeploy.supportedNetworks();
        for (uint256 i = 0; i < suites.length; i++) {
            for (uint256 j = 0; j < networks.length; j++) {
                deploy.externalPreDeployCheck(suites[i], networks[j]);
            }
        }
    }
}
