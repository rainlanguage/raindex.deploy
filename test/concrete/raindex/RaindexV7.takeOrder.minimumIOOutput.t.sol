// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {OrderV4, TakeOrdersConfigV6, TaskV3} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {MinimumIO} from "../../../src/error/ErrRaindexV7.sol";

/// @title RaindexV7TakeOrderMinimumIOOutputTest
/// @notice Audit A08-8 (#2535): the `minimumIO` guard compares
/// `config.IOIsInput ? totalTakerInput : totalTakerOutput` against
/// `config.minimumIO`. The pre-existing minimumIO test only exercises the
/// `IOIsInput == true` (totalTakerInput) branch because `defaultTakeConfig`
/// sets `IOIsInput = true`. These tests pin the `IOIsInput == false`
/// (totalTakerOutput) branch, which a swapped-branch bug would leave green
/// under every other test.
///
/// To make the branch observable the order uses ratio 0.5, so the taker's
/// output (token0 paid) is strictly less than its input (token1 received).
/// With `maximumIO` capping the take below the order's capacity,
/// `totalTakerOutput` lands exactly on the literal `maximumIO` value (it
/// passes through `min(orderMaxInput, remainingTakerIO)` unchanged), so the
/// revert's reported `actualIO` is an exact literal rather than a product of
/// Float arithmetic.
contract RaindexV7TakeOrderMinimumIOOutputTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    address internal immutable iAlice = address(uint160(uint256(keccak256("alice.rain.test"))));
    address internal immutable iBob = address(uint160(uint256(keccak256("bob.rain.test"))));

    /// Deposit 10 token1 into alice's output vault and add an order that sells
    /// token1 for token0 at ratio 0.5, then return the live order.
    function addFundedOrder() internal returns (OrderV4 memory) {
        vm.mockCall(
            address(iToken1),
            abi.encodeWithSelector(IERC20.transferFrom.selector, iAlice, address(iRaindex)),
            abi.encode(true)
        );
        vm.prank(iAlice);
        iRaindex.deposit5(
            address(iToken1), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(10, 0), new TaskV3[](0)
        );

        // outputMax (token1) = 10, IORatio = 0.5. The order can give far more
        // than the taker's capped `maximumIO`, so the take is bounded by
        // `maximumIO`, not by the order or the vault.
        return LibTestTakeOrder.addOrderWithExpression(
            vm,
            iAlice,
            "_ _:10 0.5;:;",
            address(iToken0),
            bytes32(uint256(0x01)),
            address(iToken1),
            bytes32(uint256(0x01))
        );
    }

    /// With `IOIsInput == false`, `minimumIO` is checked against the taker's
    /// OUTPUT. A `minimumIO` of 0.75 sits between the output (0.5, capped by
    /// `maximumIO`) and the input (1.0), so the take must revert: the relevant
    /// quantity is the 0.5 output. If the branch wrongly compared the 1.0
    /// input instead, 1.0 >= 0.75 and the take would fill — so the revert
    /// alone proves the output branch is taken, and the reported `actualIO`
    /// pins it to 0.5.
    function testTakeOrderMinimumIOOutputBranchRevert() external {
        OrderV4 memory order = addFundedOrder();

        TakeOrdersConfigV6 memory takeConfig = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        takeConfig.IOIsInput = false;
        // Cap the take so totalTakerOutput == maximumIO == 0.5 exactly.
        takeConfig.maximumIO = LibDecimalFloat.packLossless(5, -1);
        // Above the 0.5 output, but below the 1.0 input.
        takeConfig.minimumIO = LibDecimalFloat.packLossless(75, -2);

        // The revert happens before any token is pulled or pushed, so any
        // transfer here would be a bug.
        vm.mockCallRevert(
            address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector), "unexpected iToken0.transfer"
        );
        vm.mockCallRevert(
            address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), "unexpected iToken0.transferFrom"
        );

        vm.prank(iBob);
        vm.expectRevert(
            abi.encodeWithSelector(
                MinimumIO.selector, LibDecimalFloat.packLossless(75, -2), LibDecimalFloat.packLossless(5, -1)
            )
        );
        iRaindex.takeOrders5(takeConfig);
    }

    /// Positive control for the same `IOIsInput == false` path: with
    /// `minimumIO == 0` the identical take fills. The taker receives 1.0
    /// token1 (input) and pays 0.5 token0 (output, capped by `maximumIO`),
    /// confirming the order is fillable and that the revert above is purely
    /// the minimumIO-vs-output comparison rather than any other failure.
    function testTakeOrderMinimumIOOutputBranchFillsAtZeroMinimum() external {
        OrderV4 memory order = addFundedOrder();

        TakeOrdersConfigV6 memory takeConfig = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        takeConfig.IOIsInput = false;
        takeConfig.maximumIO = LibDecimalFloat.packLossless(5, -1);
        takeConfig.minimumIO = LibDecimalFloat.packLossless(0, 0);

        // Taker receives 1.0 token1 (its input == order output).
        vm.mockCall(
            address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector, iBob, uint256(1e18)), abi.encode(true)
        );
        vm.expectCall(address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector, iBob, uint256(1e18)), 1);
        // Taker pays 0.5 token0 (its output == order input), capped by maximumIO.
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, iBob, address(iRaindex), uint256(5e17)),
            abi.encode(true)
        );
        vm.expectCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, iBob, address(iRaindex), uint256(5e17)),
            1
        );

        vm.prank(iBob);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(takeConfig);

        assertTrue(totalTakerInput.eq(LibDecimalFloat.packLossless(1, 0)), "taker input == 1.0 token1");
        assertTrue(totalTakerOutput.eq(LibDecimalFloat.packLossless(5, -1)), "taker output == 0.5 token0");
    }
}
