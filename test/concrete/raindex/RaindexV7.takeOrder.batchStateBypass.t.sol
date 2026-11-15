// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {
    OrderV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// @title RaindexV7TakeOrderBatchStateBypassTest
/// @notice Audit Protofire H01 (#2617): a stateful per-order cap enforced in an
/// order's calculate phase holds WITHIN a single `takeOrders5` batch, not only
/// across transactions. Every order's calculate entrypoint evaluates with an
/// empty `stateOverlay`, and `handleIO` runs per order inside the loop,
/// committing that order's calculate-phase `kvs` to the store before the next
/// order calculates, so a later evaluation of the same order sees the earlier
/// one's writes.
///
/// This suite verifies the cap with a "fill once" order: the calculate phase
/// reads a store flag, marks it via `set`, and offers output only while the flag
/// is unset. Repeating the order three times in a single `takeOrders5` fills
/// exactly once.
contract RaindexV7TakeOrderBatchStateBypassTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    /// A "fill once" order: outputMax is 1 only while store key 0 is unset; the
    /// calculate phase reads the flag (before setting it) and `set`s it. The
    /// per-order `handleIO` commits that `set` before the next order calculates,
    /// so the second and third repeats read the flag as already set.
    function testH01RepeatedOrderCalculatePhaseCapHolds() external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        address bob = address(uint160(uint256(keccak256("bob.rain.test"))));

        // Fund the order's output vault generously so the vault balance is never
        // the binding constraint — only the stateful cap should limit fills.
        vm.mockCall(
            address(iToken1),
            abi.encodeWithSelector(IERC20.transferFrom.selector, owner, address(iRaindex)),
            abi.encode(true)
        );
        vm.prank(owner);
        iRaindex.deposit5(
            address(iToken1), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(10, 0), new TaskV3[](0)
        );

        // calculate: read flag, set flag, offer 1 only while flag is unset (ratio 1).
        // handle: no-op.
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm,
            owner,
            "used:get(0),:set(0 1),_ _:if(used 0 1) 1;:;",
            address(iToken0),
            bytes32(uint256(0x01)),
            address(iToken1),
            bytes32(uint256(0x01))
        );

        // The taker submits the SAME order three times in one batch.
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](3);
        for (uint256 i = 0; i < 3; i++) {
            orders[i] = TakeOrderConfigV5({
                order: order, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)
            });
        }
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(orders);

        // Allow the settlement transfers (taker receives token1, pays token0).
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector, bob), abi.encode(true));
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, bob, address(iRaindex)),
            abi.encode(true)
        );

        vm.prank(bob);
        (Float totalTakerInput,) = iRaindex.takeOrders5(config);

        // The calculate-phase cap fills at most once (total output 1): the first
        // repeat sets the flag, per-order handleIO commits it before the next
        // order calculates, and the second and third repeats read it as set and
        // offer 0.
        assertTrue(
            totalTakerInput.eq(LibDecimalFloat.packLossless(1, 0)),
            "stateful calculate-phase cap must hold within a single takeOrders5 batch"
        );
    }

    /// Intra-batch state threading is scoped per owner namespace: two orders with
    /// DIFFERENT owners, both using store key 0 as a "fill once" flag, each fill
    /// independently. Owner B's calculate read of key 0 does not see owner A's
    /// calculate write, so the total is 2.
    function testH01CrossOwnerStateIsolation() external {
        address ownerA = address(uint160(uint256(keccak256("ownerA.rain.test"))));
        address ownerB = address(uint160(uint256(keccak256("ownerB.rain.test"))));
        address bob = address(uint160(uint256(keccak256("bob.rain.test"))));

        // Fund both owners' output vaults.
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
        vm.prank(ownerA);
        iRaindex.deposit5(
            address(iToken1), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(10, 0), new TaskV3[](0)
        );
        vm.prank(ownerB);
        iRaindex.deposit5(
            address(iToken1), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(10, 0), new TaskV3[](0)
        );

        OrderV4 memory orderA = LibTestTakeOrder.addOrderWithExpression(
            vm,
            ownerA,
            "used:get(0),:set(0 1),_ _:if(used 0 1) 1;:;",
            address(iToken0),
            bytes32(uint256(0x01)),
            address(iToken1),
            bytes32(uint256(0x01))
        );
        OrderV4 memory orderB = LibTestTakeOrder.addOrderWithExpression(
            vm,
            ownerB,
            "used:get(0),:set(0 1),_ _:if(used 0 1) 1;:;",
            address(iToken0),
            bytes32(uint256(0x01)),
            address(iToken1),
            bytes32(uint256(0x01))
        );

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](2);
        orders[0] = TakeOrderConfigV5({
            order: orderA, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)
        });
        orders[1] = TakeOrderConfigV5({
            order: orderB, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)
        });
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(orders);

        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector, bob), abi.encode(true));
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, bob, address(iRaindex)),
            abi.encode(true)
        );

        vm.prank(bob);
        (Float totalTakerInput,) = iRaindex.takeOrders5(config);

        assertTrue(
            totalTakerInput.eq(LibDecimalFloat.packLossless(2, 0)),
            "different owners' calculate state must be isolated within a batch"
        );
    }
}
