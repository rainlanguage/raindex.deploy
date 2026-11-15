// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    OrderV4,
    IOV2,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    EvaluableV4,
    SignedContextV2
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {TokenMismatch} from "../../../src/error/ErrRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// @title RaindexV7TakeOrderTokenMismatchTest
/// @notice A test harness for testing the RaindexV7 takeOrder function.
/// Focuses on the token mismatch case.
contract RaindexV7TakeOrderTokenMismatchTest is RaindexV7ExternalRealTest {
    /// It is only possible to get a token mismatch when there are at least two
    /// orders. This is because `takeOrders` is interactive so we assume that
    /// the caller's desired input and output tokens match the first order they
    /// pass in.
    /// Test a mismatch in the input tokens.
    /// forge-config: default.fuzz.runs = 10
    function testTokenMismatchInputs(
        OrderV4 memory a,
        uint256 aInputIOIndex,
        uint256 aOutputIOIndex,
        OrderV4 memory b,
        uint256 bInputIOIndex,
        uint256 bOutputIOIndex,
        uint256 maxTakerInput18,
        uint256 maxIORatio
    ) external {
        vm.assume(a.validInputs.length > 0);
        aInputIOIndex = bound(aInputIOIndex, 0, a.validInputs.length - 1);
        vm.assume(b.validInputs.length > 0);
        bInputIOIndex = bound(bInputIOIndex, 0, b.validInputs.length - 1);
        vm.assume(a.validOutputs.length > 0);
        aOutputIOIndex = bound(aOutputIOIndex, 0, a.validOutputs.length - 1);
        vm.assume(b.validOutputs.length > 0);
        bOutputIOIndex = bound(bOutputIOIndex, 0, b.validOutputs.length - 1);
        maxTakerInput18 = bound(maxTakerInput18, 1, uint256(int256(type(int224).max)));
        maxIORatio = bound(maxIORatio, 0, uint256(int256(type(int224).max)));

        vm.assume(a.validInputs[aInputIOIndex].token != a.validOutputs[aOutputIOIndex].token);

        // Mismatch on inputs across orders taken.
        vm.assume(a.validInputs[aInputIOIndex].token != b.validInputs[bInputIOIndex].token);
        // Line up outputs so we don't trigger that code path.
        b.validOutputs[bOutputIOIndex].token = a.validOutputs[aOutputIOIndex].token;

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](2);
        orders[0] = TakeOrderConfigV5(a, aInputIOIndex, aOutputIOIndex, new SignedContextV2[](0));
        orders[1] = TakeOrderConfigV5(b, bInputIOIndex, bOutputIOIndex, new SignedContextV2[](0));
        TakeOrdersConfigV6 memory config = TakeOrdersConfigV6({
            minimumIO: Float.wrap(0),
            maximumIO: LibDecimalFloat.fromFixedDecimalLosslessPacked(maxTakerInput18, 18),
            maximumIORatio: LibDecimalFloat.fromFixedDecimalLosslessPacked(maxIORatio, 18),
            IOIsInput: true,
            orders: orders,
            data: ""
        });
        vm.expectRevert(abi.encodeWithSelector(TokenMismatch.selector));
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);
        (totalTakerInput, totalTakerOutput);
    }

    /// Test a mismatch in the output tokens.
    /// forge-config: default.fuzz.runs = 10
    function testTokenDecimalMismatchOutputs(
        OrderV4 memory a,
        uint256 aInputIOIndex,
        uint256 aOutputIOIndex,
        OrderV4 memory b,
        uint256 bInputIOIndex,
        uint256 bOutputIOIndex,
        uint256 maxTakerInput18,
        uint256 maxIORatio18
    ) external {
        vm.assume(a.validInputs.length > 0);
        aInputIOIndex = bound(aInputIOIndex, 0, a.validInputs.length - 1);
        vm.assume(b.validInputs.length > 0);
        bInputIOIndex = bound(bInputIOIndex, 0, b.validInputs.length - 1);
        vm.assume(a.validOutputs.length > 0);
        aOutputIOIndex = bound(aOutputIOIndex, 0, a.validOutputs.length - 1);
        vm.assume(b.validOutputs.length > 0);
        bOutputIOIndex = bound(bOutputIOIndex, 0, b.validOutputs.length - 1);
        maxTakerInput18 = bound(maxTakerInput18, 1, uint256(int256(type(int224).max)));
        maxIORatio18 = bound(maxIORatio18, 0, uint256(int256(type(int224).max)));

        vm.assume(a.validOutputs[aOutputIOIndex].token != a.validInputs[aInputIOIndex].token);

        // Mismatch on outputs across orders taken.
        vm.assume(a.validOutputs[aOutputIOIndex].token != b.validOutputs[bOutputIOIndex].token);
        // Line up inputs so we don't trigger that code path.
        b.validInputs[bInputIOIndex].token = a.validInputs[aInputIOIndex].token;

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](2);
        orders[0] = TakeOrderConfigV5(a, aInputIOIndex, aOutputIOIndex, new SignedContextV2[](0));
        orders[1] = TakeOrderConfigV5(b, bInputIOIndex, bOutputIOIndex, new SignedContextV2[](0));
        TakeOrdersConfigV6 memory config = TakeOrdersConfigV6({
            minimumIO: Float.wrap(0),
            maximumIO: LibDecimalFloat.fromFixedDecimalLosslessPacked(maxTakerInput18, 18),
            maximumIORatio: LibDecimalFloat.fromFixedDecimalLosslessPacked(maxIORatio18, 18),
            IOIsInput: true,
            orders: orders,
            data: ""
        });
        vm.expectRevert(abi.encodeWithSelector(TokenMismatch.selector));
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);
        (totalTakerInput, totalTakerOutput);
    }
}
