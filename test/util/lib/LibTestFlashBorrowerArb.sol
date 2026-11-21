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
    SignedContextV2
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.40/test/lib/deploy/LibTestInterpreterDeploy.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {GenericPoolRaindexV7FlashBorrower} from "../../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";
import {RealisticFlashLendingMockRaindex} from "test/util/concrete/RealisticFlashLendingMockRaindex.sol";

/// @dev Return value from `setup`. Caller keeps their own exchange reference.
struct FlashBorrowerSetup {
    GenericPoolRaindexV7FlashBorrower arb;
    MockToken inputToken;
    MockToken outputToken;
    IRaindexV7 raindex;
    TakeOrdersConfigV6 takeOrdersConfig;
    bytes exchangeData;
}

library LibTestFlashBorrowerArb {
    /// Set up a flash-borrower arb scenario without executing it.
    /// The caller provides their own exchange contract. Returns everything
    /// needed to call arb5 directly.
    ///
    /// @param vm The Vm cheatcode handle.
    /// @param exchange The exchange contract address.
    /// @param amount Token amount for the swap (18 decimals). Used as
    /// exchangeInputAmount, swapAmount, and minimumIO (flash loan size).
    /// The raindex receives 10x amount of outputToken.
    function setup(Vm vm, address exchange, uint256 amount) internal returns (FlashBorrowerSetup memory) {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        MockToken inputToken = new MockToken("Input", "IN", 18);
        MockToken outputToken = new MockToken("Output", "OUT", 18);

        RealisticFlashLendingMockRaindex mockRaindex = new RealisticFlashLendingMockRaindex();
        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(mockRaindex).code);
        IRaindexV7 raindex = IRaindexV7(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);

        outputToken.mint(address(raindex), 10 * amount);
        inputToken.mint(exchange, amount);

        GenericPoolRaindexV7FlashBorrower arb = new GenericPoolRaindexV7FlashBorrower();

        bytes memory exchangeData = abi.encode(
            exchange,
            exchange,
            abi.encodeCall(MockExchange.swap, (IERC20(address(outputToken)), IERC20(address(inputToken)), amount))
        );

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
                // Test amounts are always small enough for int256.
                // forge-lint: disable-next-line(unsafe-typecast)
                minimumIO: LibDecimalFloat.packLossless(int256(amount), -18),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: ""
            });
        }

        return FlashBorrowerSetup({
            arb: arb,
            inputToken: inputToken,
            outputToken: outputToken,
            raindex: raindex,
            takeOrdersConfig: takeOrdersConfig,
            exchangeData: exchangeData
        });
    }
}
