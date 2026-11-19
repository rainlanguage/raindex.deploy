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
import {LibRainDeploy} from "rain-deploy-0.1.37/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";
import {RealisticFlashLendingMockRaindex} from "test/util/concrete/RealisticFlashLendingMockRaindex.sol";

contract RaindexV7FlashBorrowerOrders0SelectionTest is Test {
    MockToken internal inputToken;
    MockToken internal outputToken;
    MockToken internal decoyInputToken;
    MockToken internal decoyOutputToken;
    address internal raindex;
    MockExchange internal exchange;
    GenericPoolRaindexV7FlashBorrower internal arb;

    function setUp() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        inputToken = new MockToken("Input", "IN", 18);
        outputToken = new MockToken("Output", "OUT", 18);
        // Decoy tokens belonging ONLY to the SECOND order; raindex and the
        // exchange hold zero of them so any attempt to flash-loan/swap them
        // reverts.
        decoyInputToken = new MockToken("DecoyIn", "DIN", 18);
        decoyOutputToken = new MockToken("DecoyOut", "DOUT", 18);

        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(new RealisticFlashLendingMockRaindex()).code);
        raindex = LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS;
        exchange = new MockExchange();

        outputToken.mint(raindex, 1000e18);
        inputToken.mint(address(exchange), 100e18);

        arb = new GenericPoolRaindexV7FlashBorrower();
    }

    /// Build a single order with the given input/output token at IO index 0.
    function buildOrder(address inTok, address outTok) internal pure returns (OrderV4 memory order) {
        IOV2[] memory validInputs = new IOV2[](1);
        validInputs[0] = IOV2(inTok, bytes32(0));
        IOV2[] memory validOutputs = new IOV2[](1);
        validOutputs[0] = IOV2(outTok, bytes32(0));
        order = OrderV4({
            owner: address(0x1234),
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            validInputs: validInputs,
            validOutputs: validOutputs,
            nonce: bytes32(0)
        });
    }

    /// arb5 reads `ordersOutputToken`/`ordersInputToken` from `orders[0]`
    /// specifically (the FIRST order), NOT a later order. With a two-order
    /// config whose SECOND order references tokens neither the raindex nor the
    /// exchange holds, the full arb cycle still completes using ONLY the FIRST
    /// order's tokens (outputToken loaned/repaid, inputToken swapped in and
    /// handed to raindex), and the second order's decoy tokens are never
    /// touched. If arb5 read `orders[1]` instead it would try to flash-loan the
    /// decoy output token (raindex holds none) and revert.
    function testArb4SelectsFirstOrderTokens() external {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](2);
        orders[0] =
            TakeOrderConfigV5(buildOrder(address(inputToken), address(outputToken)), 0, 0, new SignedContextV2[](0));
        orders[1] = TakeOrderConfigV5(
            buildOrder(address(decoyInputToken), address(decoyOutputToken)), 0, 0, new SignedContextV2[](0)
        );

        arb.arb5(
            IRaindexV7(raindex),
            TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(100, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: ""
            }),
            abi.encode(
                address(exchange),
                address(exchange),
                abi.encodeCall(MockExchange.swap, (IERC20(address(outputToken)), IERC20(address(inputToken)), 100e18))
            ),
            TaskV3({
                evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
                signedContext: new SignedContextV2[](0)
            })
        );

        // First order's tokens drove the whole cycle.
        assertEq(inputToken.balanceOf(raindex), 100e18, "raindex received first order input");
        assertEq(outputToken.balanceOf(address(exchange)), 100e18, "exchange received first order output");
        // Second order's decoy tokens were never involved.
        assertEq(decoyInputToken.balanceOf(raindex), 0, "decoy input untouched at raindex");
        assertEq(decoyOutputToken.balanceOf(address(exchange)), 0, "decoy output untouched at exchange");
        assertEq(decoyInputToken.balanceOf(address(arb)), 0, "decoy input untouched at arb");
        assertEq(decoyOutputToken.balanceOf(address(arb)), 0, "decoy output untouched at arb");
    }
}
