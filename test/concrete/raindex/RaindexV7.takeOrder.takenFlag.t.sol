// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {OrderV4, TakeOrdersConfigV6, TaskV3, IRaindexV7} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";
import {RecordingOrderTaker} from "test/util/concrete/RecordingOrderTaker.sol";

/// An order in a `takeOrders5` batch is taken only when its calculation yields a
/// strictly positive output at a non-negative ratio within the taker's
/// limit. Non-positive outputs and negative ratios are no-op skips
/// (`OrderZeroAmount`) rather than reverting the whole batch, and the
/// `onTakeOrders2` callback fires only when something was taken.
contract RaindexV7TakeOrderTakenFlagTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    address internal alice = address(uint160(uint256(keccak256("alice.rain.test"))));
    address internal bob = address(uint160(uint256(keccak256("bob.rain.test"))));

    function _addOrder(bytes memory expression) internal returns (OrderV4 memory) {
        return LibTestTakeOrder.addOrderWithExpression(
            vm, alice, expression, address(iToken0), bytes32(uint256(0x01)), address(iToken1), bytes32(uint256(0x01))
        );
    }

    /// Deposit `amount` of the output token (iToken1) into alice's output vault
    /// so an order's outputMax is not clamped to zero by an empty vault.
    function _depositOutput(uint256 amount) internal {
        vm.mockCall(
            address(iToken1),
            abi.encodeWithSelector(IERC20.transferFrom.selector, alice, address(iRaindex)),
            abi.encode(true)
        );
        vm.prank(alice);
        iRaindex.deposit5(
            address(iToken1),
            bytes32(uint256(0x01)),
            // amount is a whole-token test quantity, far below int256 max.
            // forge-lint: disable-next-line(unsafe-typecast)
            LibDecimalFloat.packLossless(int256(amount), 0),
            new TaskV3[](0)
        );
    }

    /// A negative outputMax is a no-op skip, not a `NegativeVaultBalanceChange`
    /// revert of the whole batch.
    function testNegativeOutputMaxSkips() external {
        // outputMax = -1, IORatio = 1.
        OrderV4 memory order = _addOrder("_ _:sub(0 1) 1;:;");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));

        vm.prank(bob);
        vm.expectEmit(true, true, true, true);
        emit IRaindexV7.OrderZeroAmount(bob, alice, LibOrder.hash(order));
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        assertTrue(totalTakerInput.isZero(), "totalTakerInput must be zero");
        assertTrue(totalTakerOutput.isZero(), "totalTakerOutput must be zero");
    }

    /// A negative IORatio (with a fillable output) is a no-op skip, not a
    /// `NegativeVaultBalanceChange` revert on the market-buy path.
    function testNegativeRatioSkips() external {
        _depositOutput(10);
        // outputMax = 1, IORatio = -1.
        OrderV4 memory order = _addOrder("_ _:1 sub(0 1);:;");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));

        vm.prank(bob);
        vm.expectEmit(true, true, true, true);
        emit IRaindexV7.OrderZeroAmount(bob, alice, LibOrder.hash(order));
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        assertTrue(totalTakerInput.isZero(), "totalTakerInput must be zero");
        assertTrue(totalTakerOutput.isZero(), "totalTakerOutput must be zero");
    }

    /// Regression: a zero outputMax still skips with `OrderZeroAmount`.
    function testZeroOutputMaxStillSkips() external {
        // outputMax = 0, IORatio = 1.
        OrderV4 memory order = _addOrder("_ _:0 1;:;");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));

        vm.prank(bob);
        vm.expectEmit(true, true, true, true);
        emit IRaindexV7.OrderZeroAmount(bob, alice, LibOrder.hash(order));
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        assertTrue(totalTakerInput.isZero(), "totalTakerInput must be zero");
        assertTrue(totalTakerOutput.isZero(), "totalTakerOutput must be zero");
    }

    /// `onTakeOrders2` must NOT fire when no order in the batch was taken, even
    /// when the taker supplied callback data.
    function testCallbackNotFiredWhenNothingTaken() external {
        RecordingOrderTaker taker = new RecordingOrderTaker(iRaindex);
        // A skipping order (zero outputMax) means nothing is taken.
        OrderV4 memory order = _addOrder("_ _:0 1;:;");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        config.data = hex"01"; // non-empty callback data
        config.minimumIO = LibDecimalFloat.packLossless(0, 0); // no minimum so zero-fill does not revert

        taker.take(config);

        assertTrue(!taker.called(), "onTakeOrders2 must not fire when nothing was taken");
    }
}
