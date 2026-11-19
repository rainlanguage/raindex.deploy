// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

import {RouteProcessorRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/RouteProcessorRaindexV7ArbOrderTaker.sol";
import {
    IRaindexV7,
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    OrderV4,
    IOV2,
    EvaluableV4,
    SignedContextV2,
    TaskV3,
    Float
} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.39/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.39/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.36/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockRouteProcessor} from "test/util/concrete/MockRouteProcessor.sol";
import {RealisticOrderTakerMockRaindex} from "test/util/concrete/RealisticOrderTakerMockRaindex.sol";

contract RouteProcessorRaindexV7ArbOrderTakerOnTakeOrders2Test is Test {
    /// arb6 with RouteProcessor completes a full order-taker cycle:
    /// takeOrders, onTakeOrders2 callback via mock route processor, finalize.
    function testRouteProcessorArb5() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        MockToken inputToken = new MockToken("Input", "IN", 18);
        MockToken outputToken = new MockToken("Output", "OUT", 18);

        // arb6 only trusts the canonical raindex deployment, so etch the mock's
        // runtime code (immutables included) at that address.
        RealisticOrderTakerMockRaindex mockRaindex = new RealisticOrderTakerMockRaindex(100e18);
        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(mockRaindex).code);
        RealisticOrderTakerMockRaindex raindex =
            RealisticOrderTakerMockRaindex(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
        MockRouteProcessor mockRp = new MockRouteProcessor();
        vm.etch(LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS, address(mockRp).code);
        address routeProcessor = LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS;

        // Raindex has outputToken to send to taker.
        outputToken.mint(address(raindex), 100e18);
        // RouteProcessor has inputToken to give back after swap.
        inputToken.mint(routeProcessor, 100e18);

        RouteProcessorRaindexV7ArbOrderTaker arb = new RouteProcessorRaindexV7ArbOrderTaker();

        IOV2[] memory validInputs = new IOV2[](1);
        validInputs[0] = IOV2(address(inputToken), bytes32(0));
        IOV2[] memory validOutputs = new IOV2[](1);
        validOutputs[0] = IOV2(address(outputToken), bytes32(0));

        OrderV4 memory order = OrderV4({
            owner: address(0x1234),
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            validInputs: validInputs,
            validOutputs: validOutputs,
            nonce: bytes32(0)
        });

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        orders[0] = TakeOrderConfigV5(order, 0, 0, new SignedContextV2[](0));

        // Route bytes are ignored by MockRouteProcessor.
        bytes memory takeOrdersData = abi.encode(hex"");

        arb.arb6(
            IRaindexV7(address(raindex)),
            TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(100, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: takeOrdersData
            }),
            TaskV3({
                evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
                signedContext: new SignedContextV2[](0)
            })
        );

        // Arb contract has no remaining tokens.
        assertEq(inputToken.balanceOf(address(arb)), 0, "arb inputToken");
        assertEq(outputToken.balanceOf(address(arb)), 0, "arb outputToken");
        // Raindex started with outputToken, swapped for inputToken via arb.
        assertEq(inputToken.balanceOf(address(raindex)), 100e18, "Raindex inputToken");
        assertEq(outputToken.balanceOf(address(raindex)), 0, "Raindex outputToken");
        // RouteProcessor started with inputToken, received nothing (amountIn=0).
        assertEq(inputToken.balanceOf(routeProcessor), 0, "RP inputToken");
        assertEq(outputToken.balanceOf(routeProcessor), 0, "RP outputToken");
        // Test contract received profit (outputToken) via finalizeArb.
        assertEq(outputToken.balanceOf(address(this)), 100e18, "test outputToken");
    }
}
