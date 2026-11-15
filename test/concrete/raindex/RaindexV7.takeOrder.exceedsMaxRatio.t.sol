// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {OrderV4, TakeOrdersConfigV6, TaskV3, IRaindexV7} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";

/// When an order's IORatio exceeds the taker's maximumIORatio, the order
/// is skipped and OrderExceedsMaxRatio is emitted.
contract RaindexV7TakeOrderExceedsMaxRatioTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    function testTakeOrderExceedsMaxRatio() external {
        address alice = address(uint160(uint256(keccak256("alice.rain.test"))));
        address bob = address(uint160(uint256(keccak256("bob.rain.test"))));

        // Deposit so the order can fill.
        vm.mockCall(
            address(iToken1),
            abi.encodeWithSelector(IERC20.transferFrom.selector, alice, address(iRaindex)),
            abi.encode(true)
        );
        vm.prank(alice);
        iRaindex.deposit5(
            address(iToken1), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(10, 0), new TaskV3[](0)
        );

        // Order with outputMax=1 and IORatio=100.
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm,
            alice,
            "_ _:1 100;:;",
            address(iToken0),
            bytes32(uint256(0x01)),
            address(iToken1),
            bytes32(uint256(0x01))
        );

        // maximumIORatio = 1, but order ratio is 100 -> skip.
        TakeOrdersConfigV6 memory takeConfig = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        takeConfig.maximumIORatio = LibDecimalFloat.packLossless(1, 0);

        vm.prank(bob);
        vm.expectEmit(true, true, true, true);
        emit IRaindexV7.OrderExceedsMaxRatio(bob, alice, LibOrder.hash(order));
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(takeConfig);

        assertTrue(totalTakerInput.isZero(), "totalTakerInput must be zero");
        assertTrue(totalTakerOutput.isZero(), "totalTakerOutput must be zero");
    }
}
