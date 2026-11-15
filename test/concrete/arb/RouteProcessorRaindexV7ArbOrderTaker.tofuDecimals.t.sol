// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";

import {RouteProcessorRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/RouteProcessorRaindexV7ArbOrderTaker.sol";
import {Float} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockRouteProcessor} from "test/util/concrete/MockRouteProcessor.sol";

contract RouteProcessorRaindexV7ArbOrderTakerTOFUDecimalsTest is Test {
    /// onTakeOrders2 resolves the input/output token decimals through the
    /// protocol's TOFU helper rather than a raw IERC20Metadata.decimals() read.
    /// So a token whose reported decimals changes after its first (pinned) read
    /// is rejected with TokenDecimalsReadFailure instead of being silently used
    /// with the new value: tokenA's decimals are pinned at 18, then reported as
    /// 6, and onTakeOrders2 reverts. A raw decimals() read would read 6 and
    /// complete (processRoute moves nothing at zero amounts).
    function testOnTakeOrders2RevertsOnInconsistentInputDecimals() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        MockToken tokenA = new MockToken("A", "A", 18);
        MockToken tokenB = new MockToken("B", "B", 18);
        MockRouteProcessor mockRp = new MockRouteProcessor();
        vm.etch(LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS, address(mockRp).code);

        RouteProcessorRaindexV7ArbOrderTaker arb = new RouteProcessorRaindexV7ArbOrderTaker();

        // Pin tokenA's TOFU decimals at its real 18, then have it report 6.
        LibTOFUTokenDecimals.safeDecimalsForToken(address(tokenA));
        vm.mockCall(address(tokenA), abi.encodeWithSignature("decimals()"), abi.encode(uint8(6)));

        bytes memory route = abi.encode(hex"");
        vm.expectRevert();
        arb.onTakeOrders2(address(tokenA), address(tokenB), Float.wrap(0), Float.wrap(0), route);
    }
}
