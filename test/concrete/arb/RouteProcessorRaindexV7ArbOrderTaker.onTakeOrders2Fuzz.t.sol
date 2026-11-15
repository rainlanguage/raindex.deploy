// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {IRouteProcessor} from "src/interface/IRouteProcessor.sol";
import {RouteProcessorRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/RouteProcessorRaindexV7ArbOrderTaker.sol";
import {Float} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockRouteProcessor} from "test/util/concrete/MockRouteProcessor.sol";
import {LibRainDeploy} from "rain-deploy-0.1.32/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";

/// @title RouteProcessorRaindexV7ArbOrderTakerOnTakeOrders2FuzzTest
/// @notice Audit A07-3 (#2534): `onTakeOrders2` had only concrete coverage
/// (the arb6 cycle, a zero-amount direct call, and one lossy-rounding case).
/// This fuzzes the callback directly across both tokens' decimals, the Float
/// amounts, and the route bytes. Whatever amounts come in, the callback must
/// forward to the RouteProcessor: the input token's fixed-decimal amount
/// (truncated down — precision loss is acceptable for the input side), the
/// output token's fixed-decimal amount rounded UP on any loss (so the swap
/// never under-delivers what the order needs), `address(this)` as recipient,
/// and the decoded route. The max approval it grants must also be revoked
/// afterwards.
contract RouteProcessorRaindexV7ArbOrderTakerOnTakeOrders2FuzzTest is Test {
    address internal constant ROUTE_PROCESSOR = LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS;

    // Held in storage (not locals) to keep the fuzz entrypoint within stack
    // limits without viaIR. Re-created per run because decimals are fuzzed.
    MockToken internal iInputToken;
    MockToken internal iOutputToken;
    RouteProcessorRaindexV7ArbOrderTaker internal iArb;

    /// Packed to keep the fuzz entrypoint within stack limits without viaIR.
    struct FuzzParams {
        uint8 inputDecimals;
        uint8 outputDecimals;
        int256 inputCoefficient;
        int256 inputExponent;
        int256 outputCoefficient;
        int256 outputExponent;
    }

    /// `onTakeOrders2` resolves token decimals via the deployed TOFU contract,
    /// so it must exist before the callback runs.
    function setUp() public {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);
    }

    /// Mirror the contract's conversions in a separate frame: the input side is
    /// allowed to lose precision (truncates down); the output side rounds up on
    /// any loss.
    function expectedAmounts(Float inputAmountSent, uint8 inputDecimals, Float totalOutputAmount, uint8 outputDecimals)
        internal
        pure
        returns (uint256 expectedInputAmount, uint256 expectedOutputAmount)
    {
        (expectedInputAmount,) = LibDecimalFloat.toFixedDecimalLossy(inputAmountSent, inputDecimals);
        bool lossless;
        (expectedOutputAmount, lossless) = LibDecimalFloat.toFixedDecimalLossy(totalOutputAmount, outputDecimals);
        if (!lossless) {
            expectedOutputAmount++;
        }
    }

    /// forge-config: default.fuzz.runs = 200
    function testOnTakeOrders2ForwardsConvertedAmounts(FuzzParams memory f, bytes memory route) external {
        f.inputDecimals = uint8(bound(f.inputDecimals, 0, 18));
        f.outputDecimals = uint8(bound(f.outputDecimals, 0, 18));

        iInputToken = new MockToken("IN", "IN", f.inputDecimals);
        iOutputToken = new MockToken("OUT", "OUT", f.outputDecimals);
        {
            MockRouteProcessor mockRp = new MockRouteProcessor();
            vm.etch(ROUTE_PROCESSOR, address(mockRp).code);
        }
        iArb = new RouteProcessorRaindexV7ArbOrderTaker();

        // Non-negative amounts in a range that keeps the fixed-decimal
        // conversion inside uint256. Small values and negative exponents
        // truncate toward zero, exercising the lossy branches.
        Float inputAmountSent =
            LibDecimalFloat.packLossless(bound(f.inputCoefficient, 0, 1e9), bound(f.inputExponent, -18, 18));
        Float totalOutputAmount =
            LibDecimalFloat.packLossless(bound(f.outputCoefficient, 0, 1e9), bound(f.outputExponent, -18, 18));

        (uint256 expectedInputAmount, uint256 expectedOutputAmount) =
            expectedAmounts(inputAmountSent, f.inputDecimals, totalOutputAmount, f.outputDecimals);

        // The mock route processor pulls exactly `amountIn` input token from the
        // arb, so the arb must hold it.
        iInputToken.mint(address(iArb), expectedInputAmount);

        vm.expectCall(
            ROUTE_PROCESSOR,
            abi.encodeWithSelector(
                IRouteProcessor.processRoute.selector,
                address(iInputToken),
                expectedInputAmount,
                address(iOutputToken),
                expectedOutputAmount,
                address(iArb),
                route
            )
        );

        // takeOrdersData is the abi-encoded route bytes.
        iArb.onTakeOrders2(
            address(iInputToken), address(iOutputToken), inputAmountSent, totalOutputAmount, abi.encode(route)
        );

        assertEq(iInputToken.allowance(address(iArb), ROUTE_PROCESSOR), 0, "approval revoked after swap");
    }
}
