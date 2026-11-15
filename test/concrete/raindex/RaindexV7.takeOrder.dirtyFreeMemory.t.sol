// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibRaindexDeploy} from "src/lib/deploy/LibRaindexDeploy.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {
    OrderV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// @title RaindexV7TakeOrderDirtyFreeMemoryTest
/// @notice `takeOrders5` settles exactly the taken orders when the memory past
/// the free memory pointer is nonzero on entry.
contract RaindexV7TakeOrderDirtyFreeMemoryTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    address internal owner = address(uint160(uint256(keccak256("owner.rain.test"))));
    address internal bob = address(uint160(uint256(keccak256("bob.rain.test"))));

    bytes32 internal constant OUTPUT_VAULT_ID = bytes32(uint256(0x01));

    MockToken internal token0;
    MockToken internal token1;

    constructor() {
        vm.etch(
            LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS,
            vm.getDeployedCode("RaindexV7DirtyFreeMemoryHarness.sol:RaindexV7DirtyFreeMemoryHarness")
        );
    }

    function setUp() public {
        token0 = new MockToken("Token Zero", "TK0", 18);
        token1 = new MockToken("Token One", "TK1", 18);
    }

    function _deposit(address who, MockToken token, bytes32 vaultId, uint256 amount) internal {
        token.mint(who, amount * 1e18);
        vm.startPrank(who);
        token.approve(address(iRaindex), amount * 1e18);
        // forge-lint: disable-next-line(unsafe-typecast)
        iRaindex.deposit5(address(token), vaultId, LibDecimalFloat.packLossless(int256(amount), 0), new TaskV3[](0));
        vm.stopPrank();
    }

    function testTakeOrdersDirtyFreeMemorySettlesOnlyTakenOrders() external {
        _deposit(owner, token1, OUTPUT_VAULT_ID, 10);
        OrderV4 memory taken = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _:3 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );
        OrderV4 memory zeroAmount = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _:0 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );

        token0.mint(bob, 3e18);
        vm.prank(bob);
        token0.approve(address(iRaindex), 3e18);

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](2);
        orders[0] = TakeOrderConfigV5(taken, 0, 0, new SignedContextV2[](0));
        orders[1] = TakeOrderConfigV5(zeroAmount, 0, 0, new SignedContextV2[](0));
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(orders);
        config.maximumIO = LibDecimalFloat.packLossless(10, 0);

        vm.prank(bob);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        assertTrue(totalTakerInput.eq(LibDecimalFloat.packLossless(3, 0)), "taker input");
        assertTrue(totalTakerOutput.eq(LibDecimalFloat.packLossless(3, 0)), "taker output");
        assertEq(token0.balanceOf(owner), 3e18, "owner wallet");
        assertEq(token0.balanceOf(bob), 0, "taker token0");
        assertEq(token1.balanceOf(bob), 3e18, "taker token1");
        assertEq(token0.balanceOf(address(iRaindex)), 0, "orderbook token0");
    }
}
