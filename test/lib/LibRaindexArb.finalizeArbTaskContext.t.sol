// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {EvaluableV4, SignedContextV2, TaskV3} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.39/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.39/src/interface/IInterpreterStoreV3.sol";
import {IParserV2} from "rainlang-0.2.39/src/interface/IParserV2.sol";
import {
    LibTestInterpreterDeploy,
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS,
    TEST_COMPILER_ADDRESS
} from "rainlang-0.2.39/test/lib/deploy/LibTestInterpreterDeploy.sol";
import {LibTestArb, ArbResult} from "test/util/lib/LibTestArb.sol";

/// @title LibRaindexArbFinalizeArbTaskContextTest
/// @notice Verifies that finalizeArb passes correct Float-encoded context to
/// the post-arb task: context<1 0>=input balance, context<1 1>=output balance,
/// context<1 2>=gas balance. Column 0 is the calling context added by LibContext.
contract LibRaindexArbFinalizeArbTaskContextTest is Test {
    function testFinalizeArbTaskContextValues() external {
        LibTestInterpreterDeploy.etchTestRainlang(vm);

        IParserV2 parser = IParserV2(TEST_COMPILER_ADDRESS);

        // Task expression: ensure context values match expected Floats.
        // 20e18 input profit with 18 decimals → Float(20).
        // 0 output profit → Float(0).
        // 0 gas → Float(0).
        bytes memory taskBytecode = parser.parse2(
            bytes(
                string.concat(
                    ":ensure(equal-to(context<1 0>() 20) \"input\"),",
                    ":ensure(equal-to(context<1 1>() 0) \"output\"),",
                    ":ensure(equal-to(context<1 2>() 0) \"gas\");"
                )
            )
        );

        TaskV3 memory task = TaskV3({
            evaluable: EvaluableV4(
                IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), taskBytecode
            ),
            signedContext: new SignedContextV2[](0)
        });

        // Raindex has 100e18 output, pulls 80e18 input. Exchange has 100e18 input.
        // 20e18 input profit, 0 output profit, 0 gas.
        // Task expression will revert if context values are wrong.
        ArbResult memory result = LibTestArb.setupAndArb(vm, 80e18, 100e18, 100e18, 100e18, task, 0);

        // Sanity check balances.
        assertEq(result.inputToken.balanceOf(address(this)), 20e18, "sender inputToken profit");
        assertEq(result.inputToken.balanceOf(address(result.arb)), 0, "arb inputToken");
        assertEq(result.outputToken.balanceOf(address(result.arb)), 0, "arb outputToken");
    }

    /// Companion to the above: the output (context<1 1>) and gas (context<1 2>)
    /// columns must carry the correct NON-ZERO Floats, not just zero. The
    /// sibling test only ever sees zero output/gas, so a mutation zeroing either
    /// column would survive it. Here input profit is zero while output profit is
    /// 20 and gas is 1 ether, pinning all three columns at distinct values.
    function testFinalizeArbContextNonZeroOutputAndGas() external {
        LibTestInterpreterDeploy.etchTestRainlang(vm);

        IParserV2 parser = IParserV2(TEST_COMPILER_ADDRESS);

        // 0 input profit → Float(0); 20e18 output profit (18 dp) → Float(20);
        // 1 ether gas swept (packed at -18) → Float(1).
        bytes memory taskBytecode = parser.parse2(
            bytes(
                string.concat(
                    ":ensure(equal-to(context<1 0>() 0) \"input\"),",
                    ":ensure(equal-to(context<1 1>() 20) \"output\"),",
                    ":ensure(equal-to(context<1 2>() 1) \"gas\");"
                )
            )
        );

        TaskV3 memory task = TaskV3({
            evaluable: EvaluableV4(
                IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), taskBytecode
            ),
            signedContext: new SignedContextV2[](0)
        });

        // Raindex has 100e18 output, pulls 80e18 input. Exchange has 80e18 input.
        // Arb swaps 80e18 output → 80e18 input, all pulled by raindex (0 input
        // profit), leaving 20e18 output profit; 1 ether is returned and swept.
        // Task expression reverts if any context value is wrong.
        ArbResult memory result = LibTestArb.setupAndArb(vm, 80e18, 100e18, 80e18, 80e18, task, 1 ether);

        // Sanity check balances.
        assertEq(result.inputToken.balanceOf(address(this)), 0, "no input profit");
        assertEq(result.outputToken.balanceOf(address(this)), 20e18, "sender outputToken profit");
        assertEq(result.inputToken.balanceOf(address(result.arb)), 0, "arb inputToken");
        assertEq(result.outputToken.balanceOf(address(result.arb)), 0, "arb outputToken");
    }

    /// Needed to receive the swept gas in the non-zero gas scenario.
    receive() external payable {}
}
