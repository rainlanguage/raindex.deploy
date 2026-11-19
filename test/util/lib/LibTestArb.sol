// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.19;

import {Vm} from "forge-std-1.17.0/src/Vm.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {
    IRaindexV7,
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    OrderV4,
    IOV2,
    EvaluableV4,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.40/test/lib/deploy/LibTestInterpreterDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {GenericPoolRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";
import {RealisticOrderTakerMockRaindex} from "test/util/concrete/RealisticOrderTakerMockRaindex.sol";

/// @dev Return value from `setupAndArb`.
struct ArbResult {
    GenericPoolRaindexV7ArbOrderTaker arb;
    MockToken inputToken;
    MockToken outputToken;
    RealisticOrderTakerMockRaindex raindex;
    MockExchange exchange;
}

/// @dev Return value from `setup`. Caller keeps their own exchange reference.
struct OrderTakerSetup {
    GenericPoolRaindexV7ArbOrderTaker arb;
    MockToken inputToken;
    MockToken outputToken;
    IRaindexV7 raindex;
    TakeOrdersConfigV6 takeOrdersConfig;
}

library LibTestArb {
    /// Deploy Zoltu + TOFU prerequisites for arb tests.
    function deployPrereqs(Vm vm) internal {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);
    }

    /// Build a no-op TaskV3 (empty bytecode, real deploy constants).
    function noopTask() internal pure returns (TaskV3 memory) {
        return TaskV3({
            evaluable: EvaluableV4(
                IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), hex""
            ),
            signedContext: new SignedContextV2[](0)
        });
    }

    /// Set up a standard arb scenario and execute arb6.
    ///
    /// @param vm The Vm cheatcode handle.
    /// @param raindexPullAmount How many inputTokens the mock Raindex pulls from arb.
    /// @param raindexOutputAmount How many outputTokens the mock Raindex has to send.
    /// @param exchangeInputAmount How many inputTokens the exchange has.
    /// @param swapAmount How many outputTokens the arb swaps at the exchange.
    /// @param task The post-arb task to run.
    /// @param ethValue ETH to send with arb6.
    function setupAndArb(
        Vm vm,
        uint256 raindexPullAmount,
        uint256 raindexOutputAmount,
        uint256 exchangeInputAmount,
        uint256 swapAmount,
        TaskV3 memory task,
        uint256 ethValue
    ) internal returns (ArbResult memory) {
        deployPrereqs(vm);

        MockToken inputToken = new MockToken("Input", "IN", 18);
        MockToken outputToken = new MockToken("Output", "OUT", 18);

        // arb6 only trusts the canonical raindex deployment, so etch the mock's
        // runtime code (immutables included) at that address.
        vm.etch(
            LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS,
            address(new RealisticOrderTakerMockRaindex(raindexPullAmount)).code
        );
        RealisticOrderTakerMockRaindex raindex =
            RealisticOrderTakerMockRaindex(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
        MockExchange exchange = new MockExchange();

        outputToken.mint(address(raindex), raindexOutputAmount);
        inputToken.mint(address(exchange), exchangeInputAmount);

        GenericPoolRaindexV7ArbOrderTaker arb = new GenericPoolRaindexV7ArbOrderTaker();

        bytes memory exchangeData =
            abi.encodeCall(MockExchange.swap, (IERC20(address(outputToken)), IERC20(address(inputToken)), swapAmount));

        TakeOrdersConfigV6 memory takeOrdersConfig;
        {
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

            takeOrdersConfig = TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(0, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: abi.encode(address(exchange), address(exchange), exchangeData)
            });
        }

        arb.arb6{value: ethValue}(IRaindexV7(address(raindex)), takeOrdersConfig, task);

        return
            ArbResult({
                arb: arb, inputToken: inputToken, outputToken: outputToken, raindex: raindex, exchange: exchange
            });
    }

    /// Set up an order-taker arb scenario without executing it.
    /// The caller provides their own exchange. Returns everything needed
    /// to call arb6 directly.
    ///
    /// @param vm The Vm cheatcode handle.
    /// @param exchange The exchange contract address.
    /// @param amount Token amount for the swap (18 decimals). Used as
    /// raindexPullAmount, raindexOutputAmount, exchangeInputAmount, and swapAmount.
    function setup(Vm vm, address exchange, uint256 amount) internal returns (OrderTakerSetup memory) {
        deployPrereqs(vm);

        MockToken inputToken = new MockToken("Input", "IN", 18);
        MockToken outputToken = new MockToken("Output", "OUT", 18);

        // arb6 only trusts the canonical raindex deployment, so etch the mock's
        // runtime code (immutables included) at that address.
        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(new RealisticOrderTakerMockRaindex(amount)).code);
        RealisticOrderTakerMockRaindex raindex =
            RealisticOrderTakerMockRaindex(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);

        outputToken.mint(address(raindex), amount);
        inputToken.mint(exchange, amount);

        GenericPoolRaindexV7ArbOrderTaker arb = new GenericPoolRaindexV7ArbOrderTaker();

        bytes memory exchangeData =
            abi.encodeCall(MockExchange.swap, (IERC20(address(outputToken)), IERC20(address(inputToken)), amount));

        TakeOrdersConfigV6 memory takeOrdersConfig;
        {
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

            takeOrdersConfig = TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(0, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: abi.encode(exchange, exchange, exchangeData)
            });
        }

        return OrderTakerSetup({
            arb: arb,
            inputToken: inputToken,
            outputToken: outputToken,
            raindex: IRaindexV7(address(raindex)),
            takeOrdersConfig: takeOrdersConfig
        });
    }
}
