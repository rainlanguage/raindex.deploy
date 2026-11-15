// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {OrderV4, TakeOrdersConfigV6, TaskV3} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {MinimumIO} from "../../../src/error/ErrRaindexV7.sol";

/// When the total taker IO is less than the configured minimumIO,
/// takeOrders5 must revert with MinimumIO(minimumIO, actualIO).
contract RaindexV7TakeOrderMinimumIOTest is RaindexV7ExternalRealTest {
    function testTakeOrderMinimumIORevert() external {
        address alice = address(uint160(uint256(keccak256("alice.rain.test"))));
        address bob = address(uint160(uint256(keccak256("bob.rain.test"))));

        // Deposit 1 token into alice's output vault so the order can fill.
        vm.mockCall(
            address(iToken1),
            abi.encodeWithSelector(IERC20.transferFrom.selector, alice, address(iRaindex)),
            abi.encode(true)
        );
        vm.prank(alice);
        iRaindex.deposit5(address(iToken1), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(1, 0), new TaskV3[](0));

        // Order outputs 1e-18 at ratio 1.
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm,
            alice,
            "_ _:1e-18 1;:;",
            address(iToken0),
            bytes32(uint256(0x01)),
            address(iToken1),
            bytes32(uint256(0x01))
        );

        // Take with minimumIO = 1, but order only provides 1e-18.
        TakeOrdersConfigV6 memory takeConfig = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        takeConfig.minimumIO = LibDecimalFloat.packLossless(1, 0);

        vm.prank(bob);
        vm.expectRevert(
            abi.encodeWithSelector(
                MinimumIO.selector, LibDecimalFloat.packLossless(1, 0), LibDecimalFloat.packLossless(1, -18)
            )
        );
        iRaindex.takeOrders5(takeConfig);
    }
}
