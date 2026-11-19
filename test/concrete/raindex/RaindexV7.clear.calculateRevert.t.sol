// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7FreshTakeOrderTest} from "test/util/abstract/RaindexV7FreshTakeOrderTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {OrderV4, ClearConfigV2, SignedContextV2} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";

/// A clear cannot drop one side, so a reverting calculate eval reverts
/// `clear4` with the eval's own revert data.
contract RaindexV7ClearCalculateRevertTest is RaindexV7FreshTakeOrderTest {
    address internal alice = address(uint160(uint256(keccak256("alice.rain.test"))));
    address internal bob = address(uint160(uint256(keccak256("bob.rain.test"))));

    function checkClearReverts(bytes memory aliceExpression, bytes memory bobExpression, bytes memory err) internal {
        OrderV4 memory aliceOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, alice, aliceExpression, address(iToken0), bytes32(uint256(1)), address(iToken1), bytes32(uint256(1))
        );
        OrderV4 memory bobOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, bob, bobExpression, address(iToken1), bytes32(uint256(1)), address(iToken0), bytes32(uint256(1))
        );

        vm.expectRevert(err);
        iRaindex.clear4(
            aliceOrder, bobOrder, ClearConfigV2(0, 0, 0, 0, 0, 0), new SignedContextV2[](0), new SignedContextV2[](0)
        );
    }

    function testClearAliceCalculateRevertReverts() external {
        checkClearReverts(":ensure(0 \"alice calc\"),_ _:1 1;:;", "_ _:1 1;:;", "alice calc");
    }

    function testClearBobCalculateRevertReverts() external {
        checkClearReverts("_ _:1 1;:;", ":ensure(0 \"bob calc\"),_ _:1 1;:;", "bob calc");
    }
}
