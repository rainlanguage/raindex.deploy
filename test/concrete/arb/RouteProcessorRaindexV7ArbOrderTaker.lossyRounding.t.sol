// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {IRouteProcessor} from "src/interface/IRouteProcessor.sol";

import {RouteProcessorRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/RouteProcessorRaindexV7ArbOrderTaker.sol";
import {Float} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.32/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockRouteProcessor} from "test/util/concrete/MockRouteProcessor.sol";

/// When toFixedDecimalLossy produces a lossy conversion for the output
/// amount, onTakeOrders2 MUST increment the output amount by 1 (round up).
contract RouteProcessorRaindexV7ArbOrderTakerLossyRoundingTest is Test {
    MockToken internal iInputToken;
    MockToken internal iOutputToken;
    RouteProcessorRaindexV7ArbOrderTaker internal iArb;

    function setUp() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        iInputToken = new MockToken("IN", "IN", 18);
        iOutputToken = new MockToken("OUT", "OUT", 6);

        MockRouteProcessor mockRp = new MockRouteProcessor();
        vm.etch(LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS, address(mockRp).code);

        iArb = new RouteProcessorRaindexV7ArbOrderTaker();
    }

    function testOnTakeOrders2LossyOutputRoundsUp() external {
        // Give the arb contract input tokens for the swap.
        iInputToken.mint(address(iArb), 1e18);
        // Give the route processor output tokens to send back.
        iOutputToken.mint(LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS, 1);

        // 1e-7 at 6 decimals = 0.1 of smallest unit → lossy, rounds to 0,
        // then incremented to 1.
        Float lossyOutputAmount = LibDecimalFloat.packLossless(1, -7);
        {
            (uint256 rawOutput, bool lossless) = LibDecimalFloat.toFixedDecimalLossy(lossyOutputAmount, 6);
            assertFalse(lossless, "test setup: conversion must be lossy");
            assertEq(rawOutput, 0, "test setup: raw output must be 0");
        }

        Float inputAmountSent = LibDecimalFloat.packLossless(1, 0);
        bytes memory route = hex"aa";

        // Expect processRoute called with amountOutMin = 1 (0 + 1 from rounding).
        vm.expectCall(
            LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS,
            abi.encodeWithSelector(
                IRouteProcessor.processRoute.selector,
                address(iInputToken),
                uint256(1e18),
                address(iOutputToken),
                uint256(1),
                address(iArb),
                route
            )
        );

        iArb.onTakeOrders2(
            address(iInputToken), address(iOutputToken), inputAmountSent, lossyOutputAmount, abi.encode(route)
        );

        // Approval revoked after call.
        assertEq(
            iInputToken.allowance(address(iArb), LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS),
            0,
            "approval must be revoked"
        );
    }
}
