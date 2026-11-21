// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

import {GenericPoolRaindexV7FlashBorrower} from "../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";
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
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";
import {RealisticFlashLendingMockRaindex} from "test/util/concrete/RealisticFlashLendingMockRaindex.sol";

contract RaindexV7FlashBorrowerRealTokenTransfersTest is Test {
    /// arb5 completes a full flash loan cycle with real ERC20 transfers:
    /// flash loan, exchange, take orders, repayment, and finalize.
    function testArb4RealTokenTransfers() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        MockToken inputToken = new MockToken("Input", "IN", 18);
        MockToken outputToken = new MockToken("Output", "OUT", 18);

        RealisticFlashLendingMockRaindex mockRaindex = new RealisticFlashLendingMockRaindex();
        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(mockRaindex).code);
        RealisticFlashLendingMockRaindex raindex =
            RealisticFlashLendingMockRaindex(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
        MockExchange exchange = new MockExchange();

        outputToken.mint(address(raindex), 1000e18);
        inputToken.mint(address(exchange), 100e18);

        GenericPoolRaindexV7FlashBorrower arb = new GenericPoolRaindexV7FlashBorrower();

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

        bytes memory exchangeData = abi.encode(
            address(exchange),
            address(exchange),
            abi.encodeCall(MockExchange.swap, (IERC20(address(outputToken)), IERC20(address(inputToken)), 100e18))
        );

        arb.arb5(
            IRaindexV7(address(raindex)),
            TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(100, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: ""
            }),
            exchangeData,
            TaskV3({
                evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
                signedContext: new SignedContextV2[](0)
            })
        );

        // Raindex received all inputToken from arb.
        assertEq(inputToken.balanceOf(address(raindex)), 100e18);
        // Exchange received outputToken from arb (100e18 of the 1000e18 Raindex had).
        assertEq(outputToken.balanceOf(address(exchange)), 100e18);
        // Arb contract has no remaining tokens.
        assertEq(inputToken.balanceOf(address(arb)), 0);
        assertEq(outputToken.balanceOf(address(arb)), 0);
    }
}
