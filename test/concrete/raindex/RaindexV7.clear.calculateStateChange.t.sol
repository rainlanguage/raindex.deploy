// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {OrderIOCalculationV4} from "src/struct/OrderIOCalculationV4.sol";
import {ClearStateChangeV2} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {OrderV4} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";
import {StateNamespace} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {IRaindexV7ClearStateChangeHarness} from "test/util/interface/IRaindexV7ClearStateChangeHarness.sol";

/// @title RaindexV7ClearStateChangeTest
/// @notice Pure-math coverage of `calculateClearStateChange` /
/// `calculateClearStateAlice` against fresh-compiled source. The interface
/// NatSpec is the oracle: each order's INPUT is its OUTPUT * its IO ratio, and
/// each order's OUTPUT is the smaller of its own max output and the
/// counterparty-derived cap. The difference (output - counterparty input) is the
/// bounty; a ratio mismatch that makes inputs exceed available outputs reverts
/// (NegativeBounty) at the clear4 layer.
contract RaindexV7ClearStateChangeTest is Test {
    using LibDecimalFloat for Float;

    IRaindexV7ClearStateChangeHarness internal harness;

    function setUp() external {
        // Etch runtime code so the large RaindexV7-derived harness isn't subject
        // to the EIP-170 creation size limit.
        harness = IRaindexV7ClearStateChangeHarness(
            payable(address(uint160(uint256(keccak256("clear.state.change.harness")))))
        );
        vm.etch(
            address(harness),
            vm.getDeployedCode("RaindexV7ClearStateChangeHarness.sol:RaindexV7ClearStateChangeHarness")
        );
    }

    /// Build a minimal OrderIOCalculationV4 carrying only the fields
    /// `calculateClearStateAlice` reads: outputMax and IORatio.
    function calc(Float outputMax, Float ioRatio) internal pure returns (OrderIOCalculationV4 memory c) {
        OrderV4 memory order;
        c.order = order;
        c.outputMax = outputMax;
        c.IORatio = ioRatio;
        c.context = new bytes32[][](0);
        c.namespace = StateNamespace.wrap(0);
        c.kvs = new bytes32[](0);
    }

    function f(int256 sig, int256 exp) internal pure returns (Float) {
        return LibDecimalFloat.packLossless(sig, exp);
    }

    // ---------------------------------------------------------------------
    // calculateClearStateAlice — UNCAPPED path (aliceInput <= bob.outputMax)
    // ---------------------------------------------------------------------

    /// Uncapped: alice outputs her full max (2), input = 2 * 0.5 = 1, which is
    /// <= bob's outputMax (10) so no cap. Pins B1 (input = outputMax*ratio) and
    /// B2 (output = outputMax) on the no-cap branch with EXACT values.
    function testAliceUncappedExactValues() external view {
        // aliceOutputMax=2, aliceIORatio=0.5 -> aliceInput=1; bobOutputMax=10.
        (Float aliceInput, Float aliceOutput) =
            harness.exposedCalculateClearStateAlice(calc(f(2, 0), f(5, -1)), calc(f(10, 0), f(1, 0)));
        assertTrue(aliceInput.eq(f(1, 0)), "uncapped input = outputMax * ratio = 1");
        assertTrue(aliceOutput.eq(f(2, 0)), "uncapped output = outputMax = 2");
    }

    /// Uncapped with ratio > 1: aliceOutputMax=3, ratio=2 -> input=6, bob's
    /// outputMax=6 -> NOT greater than 6, boundary equal -> uncapped. Output is
    /// the full 3. This pins the cap predicate is a STRICT gt at the boundary
    /// (== is uncapped) and that input is the product, not a sum.
    function testAliceUncappedBoundaryEqual() external view {
        (Float aliceInput, Float aliceOutput) =
            harness.exposedCalculateClearStateAlice(calc(f(3, 0), f(2, 0)), calc(f(6, 0), f(1, 0)));
        // aliceInput = 3 * 2 = 6 == bob.outputMax 6, so NOT capped.
        assertTrue(aliceInput.eq(f(6, 0)), "boundary: input stays 6 (== not > bob max)");
        assertTrue(aliceOutput.eq(f(3, 0)), "boundary: output stays full outputMax 3");
    }

    // ---------------------------------------------------------------------
    // calculateClearStateAlice — CAPPED path (aliceInput > bob.outputMax)
    // ---------------------------------------------------------------------

    /// Capped: aliceOutputMax=10, ratio=2 -> input would be 20, but bob's
    /// outputMax is only 4, and 20 > 4 so input caps to 4 and output recomputes
    /// to input/ratio = 4/2 = 2. Pins B3 (cap predicate), B4 (input := bob max),
    /// B5 (output := input/ratio) with exact values.
    function testAliceCappedExactValues() external view {
        (Float aliceInput, Float aliceOutput) =
            harness.exposedCalculateClearStateAlice(calc(f(10, 0), f(2, 0)), calc(f(4, 0), f(1, 0)));
        assertTrue(aliceInput.eq(f(4, 0)), "capped input := bob.outputMax = 4");
        assertTrue(aliceOutput.eq(f(2, 0)), "capped output := input / ratio = 4/2 = 2");
    }

    /// Capped with a fractional ratio: aliceOutputMax=8, ratio=0.5 would give
    /// input=4, but bob's outputMax is 3 (< 4) so input caps to 3 and output
    /// recomputes to 3 / 0.5 = 6 (NOT the original 8). Pins that the recompute
    /// uses DIVISION by the ratio (not multiply) and the cap value flows through.
    function testAliceCappedFractionalRatio() external view {
        (Float aliceInput, Float aliceOutput) =
            harness.exposedCalculateClearStateAlice(calc(f(8, 0), f(5, -1)), calc(f(3, 0), f(1, 0)));
        assertTrue(aliceInput.eq(f(3, 0)), "capped input := bob.outputMax = 3");
        assertTrue(aliceOutput.eq(f(6, 0)), "capped output := 3 / 0.5 = 6");
    }

    // ---------------------------------------------------------------------
    // calculateClearStateChange — wiring of alice/bob legs
    // ---------------------------------------------------------------------

    /// Full state change with a spread: alice (outputMax=1, ratio=0.5) and bob
    /// (outputMax=1, ratio=0.5). Neither input (0.5) exceeds the counterparty's
    /// outputMax (1) so both legs are uncapped: each outputs 1, inputs 0.5. This
    /// pins the leg wiring in calculateClearStateChange — alice's leg uses
    /// (aliceCalc, bobCalc) and bob's leg flips to (bobCalc, aliceCalc) — by
    /// asserting every one of the four fields independently.
    function testClearStateChangeUncappedBothLegs() external view {
        ClearStateChangeV2 memory c =
            harness.exposedCalculateClearStateChange(calc(f(1, 0), f(5, -1)), calc(f(1, 0), f(5, -1)));
        assertTrue(c.aliceOutput.eq(f(1, 0)), "aliceOutput = 1");
        assertTrue(c.bobOutput.eq(f(1, 0)), "bobOutput = 1");
        assertTrue(c.aliceInput.eq(f(5, -1)), "aliceInput = 0.5");
        assertTrue(c.bobInput.eq(f(5, -1)), "bobInput = 0.5");
    }

    /// Full state change where the legs are ASYMMETRIC, so swapping the leg
    /// arguments would change the result. Alice: outputMax=10, ratio=2 (input
    /// would be 20). Bob: outputMax=3, ratio=1 (input would be 3). Alice's leg:
    /// counterparty (bob) outputMax=3, 20>3 -> aliceInput=3, aliceOutput=3/2=1.5.
    /// Bob's leg: counterparty (alice) outputMax=10, bob input 3 <= 10 -> uncapped
    /// -> bobInput=3, bobOutput=3. The four fields are all distinct, so a mutation
    /// that mis-wires the legs (e.g. passes alice twice, or fails to flip) lands
    /// on different numbers and is caught.
    function testClearStateChangeAsymmetricLegs() external view {
        ClearStateChangeV2 memory c =
            harness.exposedCalculateClearStateChange(calc(f(10, 0), f(2, 0)), calc(f(3, 0), f(1, 0)));
        assertTrue(c.aliceOutput.eq(f(15, -1)), "aliceOutput = 3/2 = 1.5");
        assertTrue(c.aliceInput.eq(f(3, 0)), "aliceInput capped to bob.outputMax = 3");
        assertTrue(c.bobOutput.eq(f(3, 0)), "bobOutput = bob.outputMax = 3 (uncapped)");
        assertTrue(c.bobInput.eq(f(3, 0)), "bobInput = bobOutputMax*ratio = 3*1 = 3");
    }
}
