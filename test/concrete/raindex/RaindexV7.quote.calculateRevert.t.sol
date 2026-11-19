// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7FreshTakeOrderTest} from "test/util/abstract/RaindexV7FreshTakeOrderTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {OrderV4, QuoteV3, SignedContextV2} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";

/// `quote3` surfaces a reverting calculate eval as the eval's own revert data.
contract RaindexV7QuoteCalculateRevertTest is RaindexV7FreshTakeOrderTest {
    function testQuoteCalculateRevertReverts() external {
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm,
            address(uint160(uint256(keccak256("alice.rain.test")))),
            ":ensure(0 \"calc\"),_ _:1 1;:;",
            address(iToken0),
            bytes32(uint256(1)),
            address(iToken1),
            bytes32(uint256(1))
        );

        vm.expectRevert(bytes("calc"));
        iRaindex.quote3(QuoteV3(order, 0, 0, new SignedContextV2[](0)));
    }
}
