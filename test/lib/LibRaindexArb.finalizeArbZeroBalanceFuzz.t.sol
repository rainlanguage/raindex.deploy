// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibTestArb, ArbResult} from "test/util/lib/LibTestArb.sol";

/// @title LibRaindexArbFinalizeArbZeroBalanceFuzzTest
/// @notice Audit GAP-A12-1 / GAP-A12-4 (#2537): the zero-leftover path and fuzz
/// coverage for `finalizeArb`'s input-token sweep.
contract LibRaindexArbFinalizeArbZeroBalanceFuzzTest is Test {
    /// GAP-A12-1: when the arb leaves zero token balance (raindex pulls exactly
    /// the swapped amount), `finalizeArb`'s `if (balance > 0)` guards skip the
    /// `safeTransfer`s — the call succeeds and the sender receives nothing.
    function testFinalizeArbNoTransferWhenZeroBalance() external {
        // pull == swap (== raindex output == exchange input) => zero leftover.
        ArbResult memory result = LibTestArb.setupAndArb(vm, 100e18, 100e18, 100e18, 100e18, LibTestArb.noopTask(), 0);

        assertEq(result.inputToken.balanceOf(address(this)), 0, "no input profit to sender");
        assertEq(result.outputToken.balanceOf(address(this)), 0, "no output profit to sender");
        assertEq(result.inputToken.balanceOf(address(result.arb)), 0, "arb empty input");
        assertEq(result.outputToken.balanceOf(address(result.arb)), 0, "arb empty output");
    }

    /// GAP-A12-4: fuzz the swap and pull amounts. `finalizeArb` sweeps exactly
    /// the input-token profit (`swap - pull`) to the sender and leaves the arb
    /// empty, across the full range including the zero-leftover boundary.
    function testFinalizeArbSweepsInputProfitFuzz(uint256 swapAmount, uint256 pullAmount) external {
        swapAmount = bound(swapAmount, 1, 1e27);
        pullAmount = bound(pullAmount, 0, swapAmount);

        // raindex output == exchange input == swap so the arb swaps all of its
        // output and the exchange can fill it; raindex then pulls `pullAmount`.
        ArbResult memory result =
            LibTestArb.setupAndArb(vm, pullAmount, swapAmount, swapAmount, swapAmount, LibTestArb.noopTask(), 0);

        assertEq(result.inputToken.balanceOf(address(this)), swapAmount - pullAmount, "input profit swept");
        assertEq(result.inputToken.balanceOf(address(result.arb)), 0, "arb empty input");
        assertEq(result.outputToken.balanceOf(address(result.arb)), 0, "arb empty output");
        assertEq(result.inputToken.balanceOf(address(result.raindex)), pullAmount, "raindex pulled exactly");
    }
}
