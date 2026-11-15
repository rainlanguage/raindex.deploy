// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {OrderV4, TakeOrdersConfigV6, IRaindexV7} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {Strings} from "@openzeppelin-contracts-5.7.0/utils/Strings.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";

/// The calculations context column (`calculated-max-output`,
/// `calculated-io-ratio`) is populated from the calculate eval's outputs, so it
/// only holds meaningful values during handle IO. During calculate itself the
/// column is seeded with zeros, so reading those words there yields 0 rather
/// than reverting with an out-of-bounds bounds-check panic.
contract RaindexV7TakeOrderCalculationsContextTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;
    using Strings for address;

    address internal alice = address(uint160(uint256(keccak256("alice.rain.test"))));
    address internal bob = address(uint160(uint256(keccak256("bob.rain.test"))));

    function _addOrder(bytes memory expression) internal returns (OrderV4 memory) {
        return LibTestTakeOrder.addOrderWithExpression(
            vm, alice, expression, address(iToken0), bytes32(uint256(0x01)), address(iToken1), bytes32(uint256(0x01))
        );
    }

    /// An order whose calculate source reads `calculated-max-output()` and
    /// `calculated-io-ratio()` evaluates without reverting; both read 0 because
    /// the calculations are not yet known during calculate. The order then
    /// no-op skips on its empty output vault rather than the whole batch
    /// reverting on a bounds-check panic.
    function testCalculatedWordsReadZeroDuringCalculate() external {
        bytes memory expression = bytes(
            string.concat(
                "using-words-from ",
                address(iSubParser).toHexString(),
                "\n_ _:1e0 1e0,",
                ":ensure(equal-to(calculated-max-output() 0) \"calc max output zero\"),",
                ":ensure(equal-to(calculated-io-ratio() 0) \"calc io ratio zero\");",
                ":;"
            )
        );
        OrderV4 memory order = _addOrder(expression);
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));

        // The calculate eval (with its in-band `ensure`s on the calculations
        // words) must run to completion. The empty output vault then clamps the
        // fill to zero, so the order is a zero-amount skip.
        vm.prank(bob);
        vm.expectEmit(true, true, true, true);
        emit IRaindexV7.OrderZeroAmount(bob, alice, LibOrder.hash(order));
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        assertTrue(totalTakerInput.isZero(), "totalTakerInput must be zero");
        assertTrue(totalTakerOutput.isZero(), "totalTakerOutput must be zero");
    }
}
