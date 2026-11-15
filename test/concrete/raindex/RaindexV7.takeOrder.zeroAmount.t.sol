// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Vm} from "forge-std-1.17.0/src/Test.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {OrderV4, TakeOrdersConfigV6, TaskV3} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";

/// @title RaindexV7TakeOrderZeroAmountTest
/// @notice Audit A08-4 (#2535): when an order's `outputMax` evaluates to zero
/// the order is skipped (not reverted) and `OrderZeroAmount` is emitted. This
/// is distinct from `OrderExceedsMaxRatio` (ratio too high) and `OrderNotFound`
/// (dead order): here the order is live and within ratio, but has nothing to
/// give.
contract RaindexV7TakeOrderZeroAmountTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    /// A live order whose evaluation yields `outputMax == 0` is skipped with an
    /// `OrderZeroAmount` event, no tokens move, and the call returns zero/zero
    /// rather than reverting.
    function testTakeOrderZeroAmount() external {
        address alice = address(uint160(uint256(keccak256("alice.rain.test"))));
        address bob = address(uint160(uint256(keccak256("bob.rain.test"))));

        // Deposit so the only reason the order can't fill is the zero output max.
        vm.mockCall(
            address(iToken1),
            abi.encodeWithSelector(IERC20.transferFrom.selector, alice, address(iRaindex)),
            abi.encode(true)
        );
        vm.prank(alice);
        iRaindex.deposit5(
            address(iToken1), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(10, 0), new TaskV3[](0)
        );

        // Order with outputMax=0 and a perfectly fine IORatio=1.
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm, alice, "_ _:0 1;:;", address(iToken0), bytes32(uint256(0x01)), address(iToken1), bytes32(uint256(0x01))
        );

        // maximumIORatio is generous so the skip is purely due to the zero
        // output max, not a ratio rejection.
        TakeOrdersConfigV6 memory takeConfig = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));

        // No token should move out of either vault token.
        vm.mockCallRevert(
            address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector), "unexpected iToken0.transfer"
        );
        vm.mockCallRevert(
            address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector), "unexpected iToken1.transfer"
        );

        vm.prank(bob);
        vm.recordLogs();
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(takeConfig);

        assertTrue(totalTakerInput.isZero(), "totalTakerInput must be zero");
        assertTrue(totalTakerOutput.isZero(), "totalTakerOutput must be zero");

        // The skip path emits exactly one event (OrderZeroAmount) and does not
        // emit a TakeOrderV4, confirming no fill occurred. Decoding the single
        // recorded log directly (rather than vm.expectEmit) avoids counting the
        // expectEmit helper's own synthetic log.
        Vm.Log[] memory logs = vm.getRecordedLogs();
        assertEq(logs.length, 1, "only OrderZeroAmount emitted");
        assertEq(logs[0].topics.length, 1, "OrderZeroAmount is unindexed");
        assertEq(
            logs[0].topics[0],
            bytes32(uint256(keccak256("OrderZeroAmount(address,address,bytes32)"))),
            "event is OrderZeroAmount"
        );
        (address sender, address owner, bytes32 orderHash) = abi.decode(logs[0].data, (address, address, bytes32));
        assertEq(sender, bob, "OrderZeroAmount sender");
        assertEq(owner, alice, "OrderZeroAmount owner");
        assertEq(orderHash, LibOrder.hash(order), "OrderZeroAmount orderHash");
    }
}
