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
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.35/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.35/test/lib/deploy/LibTestInterpreterDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockRouteProcessor} from "test/util/concrete/MockRouteProcessor.sol";
import {RealisticOrderTakerMockRaindex} from "test/util/concrete/RealisticOrderTakerMockRaindex.sol";

/// arb6 with RouteProcessor MUST correctly convert Float amounts to
/// fixed-point using the token's decimals, not hardcoded 18.
contract RouteProcessorRaindexV7ArbOrderTakerNonStandardDecimalsTest is Test {
    function testRouteProcessorArb5SixDecimalTokens() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        // 6-decimal tokens (like USDC/USDT).
        MockToken inputToken = new MockToken("USDC", "USDC", 6);
        MockToken outputToken = new MockToken("USDT", "USDT", 6);

        // Raindex will send 100 USDT (100e6) to the taker, then pull 100 USDC.
        // arb6 only trusts the canonical raindex deployment, so etch the mock's
        // runtime code (immutables included) at that address.
        RealisticOrderTakerMockRaindex mockRaindex = new RealisticOrderTakerMockRaindex(100e6);
        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(mockRaindex).code);
        RealisticOrderTakerMockRaindex raindex =
            RealisticOrderTakerMockRaindex(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
        MockRouteProcessor mockRp = new MockRouteProcessor();
        vm.etch(LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS, address(mockRp).code);
        address routeProcessor = LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS;

        // Raindex has outputToken to send to taker.
        outputToken.mint(address(raindex), 100e6);
        // RouteProcessor has inputToken to give back after swap.
        inputToken.mint(routeProcessor, 100e6);

        RouteProcessorRaindexV7ArbOrderTaker arb = new RouteProcessorRaindexV7ArbOrderTaker();

        IOV2[] memory validInputs = new IOV2[](1);
        validInputs[0] = IOV2(address(inputToken), bytes32(0));
        IOV2[] memory validOutputs = new IOV2[](1);
        validOutputs[0] = IOV2(address(outputToken), bytes32(0));

        OrderV4 memory order = OrderV4({
            owner: address(0x1234),
            evaluable: EvaluableV4(
                IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), hex""
            ),
            validInputs: validInputs,
            validOutputs: validOutputs,
            nonce: bytes32(0)
        });

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        orders[0] = TakeOrderConfigV5(order, 0, 0, new SignedContextV2[](0));

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
                evaluable: EvaluableV4(
                    IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), hex""
                ),
                signedContext: new SignedContextV2[](0)
            })
        );

        // Arb contract has no remaining tokens.
        assertEq(inputToken.balanceOf(address(arb)), 0, "arb inputToken");
        assertEq(outputToken.balanceOf(address(arb)), 0, "arb outputToken");
        // Raindex swapped outputToken for inputToken.
        assertEq(inputToken.balanceOf(address(raindex)), 100e6, "Raindex inputToken");
        assertEq(outputToken.balanceOf(address(raindex)), 0, "Raindex outputToken");
        // RouteProcessor sent all its inputToken.
        assertEq(inputToken.balanceOf(routeProcessor), 0, "RP inputToken");
        assertEq(outputToken.balanceOf(routeProcessor), 0, "RP outputToken");
        // Test contract received profit via finalizeArb.
        assertEq(outputToken.balanceOf(address(this)), 100e6, "test outputToken");
    }
}
