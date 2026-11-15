// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

import {GenericPoolRaindexV7ArbOrderTaker} from "../../src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol";
import {
    IRaindexV7,
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    OrderV4,
    IOV2,
    EvaluableV4,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.35/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.32/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";
import {RealisticOrderTakerMockRaindex} from "test/util/concrete/RealisticOrderTakerMockRaindex.sol";

/// arb6 reads the order's input token from `validInputs[inputIOIndex]` and the
/// output token from `validOutputs[outputIOIndex]`. A single order with two IOs
/// per side (index 0 = decoy, index 1 = real) and indices set to 1 must run the
/// full cycle on the index-1 tokens.
///
/// The scenario leaves BOTH a real-input leftover (raindex pulls less than the
/// swap produced) and a real-output leftover (the exchange swaps less than the
/// raindex sent), so finalize must sweep both real tokens. If arb6 read the
/// decoy input (index 0) the raindex's pull reverts on a missing allowance; if
/// arb6 read the decoy output (index 0) finalize sweeps the zero-balance decoy
/// and strands the real-output leftover in the arb.
contract RaindexV7ArbOrderTakerIoIndexSelectionTest is Test {
    receive() external payable {}

    function noopTask() internal pure returns (TaskV3 memory) {
        return TaskV3({
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            signedContext: new SignedContextV2[](0)
        });
    }

    function testArb5SelectsTokensByConfiguredIoIndices() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        MockToken inputToken = new MockToken("Input", "IN", 18);
        MockToken outputToken = new MockToken("Output", "OUT", 18);
        // Decoys placed at IO index 0; the real tokens sit at index 1.
        MockToken decoyInput = new MockToken("DecoyIn", "DIN", 18);
        MockToken decoyOutput = new MockToken("DecoyOut", "DOUT", 18);

        // Raindex sends 100e18 output and pulls 80e18 input. arb6 only trusts the
        // canonical raindex deployment, so etch the mock's runtime code
        // (immutables included) at that address.
        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(new RealisticOrderTakerMockRaindex(80e18)).code);
        RealisticOrderTakerMockRaindex raindex =
            RealisticOrderTakerMockRaindex(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
        MockExchange exchange = new MockExchange();

        outputToken.mint(address(raindex), 100e18);
        // Exchange can only fill an 80e18 swap, so 20e18 real output is left in
        // the arb to be finalized.
        inputToken.mint(address(exchange), 80e18);

        GenericPoolRaindexV7ArbOrderTaker arb = new GenericPoolRaindexV7ArbOrderTaker();

        IOV2[] memory validInputs = new IOV2[](2);
        validInputs[0] = IOV2(address(decoyInput), bytes32(0));
        validInputs[1] = IOV2(address(inputToken), bytes32(0));
        IOV2[] memory validOutputs = new IOV2[](2);
        validOutputs[0] = IOV2(address(decoyOutput), bytes32(0));
        validOutputs[1] = IOV2(address(outputToken), bytes32(0));

        OrderV4 memory order = OrderV4({
            owner: address(0x1234),
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            validInputs: validInputs,
            validOutputs: validOutputs,
            nonce: bytes32(0)
        });

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        // inputIOIndex 1, outputIOIndex 1: the REAL tokens.
        orders[0] = TakeOrderConfigV5(order, 1, 1, new SignedContextV2[](0));

        bytes memory exchangeData =
            abi.encodeCall(MockExchange.swap, (IERC20(address(outputToken)), IERC20(address(inputToken)), 80e18));

        arb.arb6(
            IRaindexV7(address(raindex)),
            TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(0, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: abi.encode(address(exchange), address(exchange), exchangeData)
            }),
            noopTask()
        );

        // The cycle ran on the index-1 (real) tokens: raindex pulled the real
        // input, the exchange swapped 80e18 of the real output, and finalize
        // swept the 20e18 real-output leftover to the sender.
        assertEq(inputToken.balanceOf(address(raindex)), 80e18, "raindex pulled real input");
        assertEq(outputToken.balanceOf(address(exchange)), 80e18, "exchange swapped real output");
        assertEq(outputToken.balanceOf(address(this)), 20e18, "real output leftover swept to sender");
        assertEq(inputToken.balanceOf(address(this)), 0, "no real input leftover");
        // The arb is left empty of the real output (it was finalized, not
        // stranded by reading a decoy index).
        assertEq(outputToken.balanceOf(address(arb)), 0, "arb empty of real output");

        // The decoys (index 0) were never touched: no balances moved, no
        // allowance was granted to the raindex on the decoy input.
        assertEq(decoyInput.allowance(address(arb), address(raindex)), 0, "decoy input never approved to raindex");
        assertEq(decoyInput.balanceOf(address(arb)), 0, "no decoy input held");
        assertEq(decoyOutput.balanceOf(address(this)), 0, "no decoy output swept");
    }

    /// arb6 reads the input token from `validInputs[inputIOIndex]` and the output
    /// token from `validOutputs[outputIOIndex]` using the side-SPECIFIC index.
    /// The sibling test sets both indices to 1, so reading the input via
    /// `outputIOIndex` (or the output via `inputIOIndex`) selects the same slot
    /// and is indistinguishable. Here the indices are DISTINCT (inputIOIndex 1,
    /// outputIOIndex 0) with the real input at validInputs[1] and the real output
    /// at validOutputs[0]; the opposite slot on each side holds a decoy. Reading
    /// the input via outputIOIndex picks decoyInput (raindex pull reverts on the
    /// missing allowance/balance); reading the output via inputIOIndex picks
    /// decoyOutput (finalize sweeps the zero-balance decoy and strands the real
    /// output leftover in the arb).
    function testArb5SelectsTokensByDistinctIoIndices() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        MockToken inputToken = new MockToken("Input", "IN", 18);
        MockToken outputToken = new MockToken("Output", "OUT", 18);
        // Decoy input sits at validInputs[0]; decoy output at validOutputs[1].
        MockToken decoyInput = new MockToken("DecoyIn", "DIN", 18);
        MockToken decoyOutput = new MockToken("DecoyOut", "DOUT", 18);

        // Raindex sends 100e18 output and pulls 80e18 input.
        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(new RealisticOrderTakerMockRaindex(80e18)).code);
        RealisticOrderTakerMockRaindex raindex =
            RealisticOrderTakerMockRaindex(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
        MockExchange exchange = new MockExchange();

        outputToken.mint(address(raindex), 100e18);
        // Exchange fills an 80e18 swap, leaving a 20e18 real-output leftover.
        inputToken.mint(address(exchange), 80e18);

        GenericPoolRaindexV7ArbOrderTaker arb = new GenericPoolRaindexV7ArbOrderTaker();

        // Real input at index 1, real output at index 0: the two sides use
        // DIFFERENT indices.
        IOV2[] memory validInputs = new IOV2[](2);
        validInputs[0] = IOV2(address(decoyInput), bytes32(0));
        validInputs[1] = IOV2(address(inputToken), bytes32(0));
        IOV2[] memory validOutputs = new IOV2[](2);
        validOutputs[0] = IOV2(address(outputToken), bytes32(0));
        validOutputs[1] = IOV2(address(decoyOutput), bytes32(0));

        OrderV4 memory order = OrderV4({
            owner: address(0x1234),
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            validInputs: validInputs,
            validOutputs: validOutputs,
            nonce: bytes32(0)
        });

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        // inputIOIndex 1 (real input at validInputs[1]); outputIOIndex 0 (real
        // output at validOutputs[0]).
        orders[0] = TakeOrderConfigV5(order, 1, 0, new SignedContextV2[](0));

        bytes memory exchangeData =
            abi.encodeCall(MockExchange.swap, (IERC20(address(outputToken)), IERC20(address(inputToken)), 80e18));

        arb.arb6(
            IRaindexV7(address(raindex)),
            TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(0, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: abi.encode(address(exchange), address(exchange), exchangeData)
            }),
            noopTask()
        );

        // The cycle ran on the side-specific tokens: input via inputIOIndex 1,
        // output via outputIOIndex 0.
        assertEq(inputToken.balanceOf(address(raindex)), 80e18, "raindex pulled real input (inputIOIndex)");
        assertEq(outputToken.balanceOf(address(exchange)), 80e18, "exchange swapped real output (outputIOIndex)");
        assertEq(outputToken.balanceOf(address(this)), 20e18, "real output leftover swept to sender");
        assertEq(inputToken.balanceOf(address(this)), 0, "no real input leftover");
        assertEq(outputToken.balanceOf(address(arb)), 0, "arb empty of real output");

        // The decoys (the OPPOSITE index on each side) were never touched.
        assertEq(decoyInput.allowance(address(arb), address(raindex)), 0, "decoy input never approved to raindex");
        assertEq(decoyInput.balanceOf(address(arb)), 0, "no decoy input held");
        assertEq(decoyOutput.balanceOf(address(this)), 0, "no decoy output swept");
        assertEq(decoyOutput.balanceOf(address(arb)), 0, "no decoy output held");
    }
}
