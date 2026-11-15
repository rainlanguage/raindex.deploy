// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {Vm} from "forge-std-1.17.0/src/Test.sol";
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

/// @title RaindexV7TakeOrderBatchStateAdversarialTest
/// @notice Audit Protofire H01 (#2617): adversarial coverage of the per-owner
/// `stateOverlay` threading in `takeOrders5`. Each test pins one behaviour of
/// the fix with a discriminating total: the value differs from what the
/// pre-fix (or a mutated) contract would produce.
contract RaindexV7TakeOrderBatchStateAdversarialTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    address internal immutable iBob = address(uint160(uint256(keccak256("bob.rain.test"))));

    function owner(string memory tag) internal pure returns (address) {
        return address(uint160(uint256(keccak256(bytes(tag)))));
    }

    /// Allow every deposit/settlement transfer.
    function mockTransfers() internal {
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
    }

    function fund(address o) internal {
        vm.prank(o);
        iRaindex.deposit5(
            address(iToken1), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(100, 0), new TaskV3[](0)
        );
    }

    function addOrder(address o, bytes memory expr) internal returns (OrderV4 memory) {
        return LibTestTakeOrder.addOrderWithExpression(
            vm, o, expr, address(iToken0), bytes32(uint256(0x01)), address(iToken1), bytes32(uint256(0x01))
        );
    }

    function take(OrderV4[] memory ordersList) internal returns (Float totalTakerInput) {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](ordersList.length);
        for (uint256 i = 0; i < ordersList.length; i++) {
            orders[i] = TakeOrderConfigV5({
                order: ordersList[i], inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)
            });
        }
        vm.prank(iBob);
        (totalTakerInput,) = iRaindex.takeOrders5(LibTestTakeOrder.defaultTakeConfig(orders));
    }

    /// Threading works across DIFFERENT orders of the same owner, not just
    /// repeats: order A writes key 0; order B (same owner, different expression)
    /// reads key 0 and only fills while unset. In one batch B must observe A's
    /// write and skip. Pre-fix: B reads stale 0 and fills → total 2.
    function testThreadingAcrossDistinctSameOwnerOrders() external {
        mockTransfers();
        address o = owner("alice");
        fund(o);
        OrderV4[] memory list = new OrderV4[](2);
        list[0] = addOrder(o, ":set(0 1),_ _:1 1;:;");
        list[1] = addOrder(o, "used:get(0),_ _:if(used 0 1) 1;:;");
        assertTrue(take(list).eq(LibDecimalFloat.packLossless(1, 0)), "B must see A's calculate write");
    }

    /// An UNTAKEN order causes no state change, so its calculate `set` is not
    /// visible to later same-owner orders — it neither threads via an overlay
    /// nor commits to the store, because an untaken order runs no handleIO. A
    /// writes key 0 but offers 0 (untaken -> OrderZeroAmount); B reads key 0,
    /// sees it unset, and fills -> total 1. (The old overlay threaded A's write
    /// so B skipped -> total 0.)
    function testUntakenOrderWriteNotVisibleToLaterSameOwnerOrder() external {
        mockTransfers();
        address o = owner("bob.owner");
        fund(o);
        OrderV4[] memory list = new OrderV4[](2);
        list[0] = addOrder(o, ":set(0 1),_ _:0 1;:;");
        list[1] = addOrder(o, "used:get(0),_ _:if(used 0 1) 1;:;");
        assertTrue(
            take(list).eq(LibDecimalFloat.packLossless(1, 0)),
            "untaken order's write must not be visible to a later same-owner order"
        );
    }

    /// Per-owner scoping: owner B writes key 0; owner A (who never wrote key 0)
    /// reads it and must NOT see B's write — A fills. Interleaving an unrelated
    /// owner between A's two orders also must not leak. Batch [A1, Bwriter, A2]
    /// fills all three (total 3). A flat (un-scoped) overlay would leak B's
    /// write into A2 and drop it -> total 2.
    function testCrossOwnerNoLeakInterleaved() external {
        mockTransfers();
        address a = owner("ownerA");
        address b = owner("ownerB");
        fund(a);
        fund(b);
        OrderV4[] memory list = new OrderV4[](3);
        list[0] = addOrder(a, "_ _:1 1;:;");
        list[1] = addOrder(b, ":set(0 1),_ _:1 1;:;");
        list[2] = addOrder(a, "used:get(0),_ _:if(used 0 1) 1;:;");
        assertTrue(take(list).eq(LibDecimalFloat.packLossless(3, 0)), "owner A must not see owner B's key 0 write");
    }

    /// Cumulative counter threaded across repeats: each fill requires the stored
    /// counter < 2 and increments it. Repeated 5x in one batch fills exactly
    /// twice (n = 0,1), proving values (not just a boolean flag) thread
    /// correctly and that skipped repeats still advance the counter via `set`.
    /// Pre-fix: every repeat reads 0 and fills -> total 5.
    function testCumulativeCounterThreadsAcrossRepeats() external {
        mockTransfers();
        address o = owner("carol");
        fund(o);
        OrderV4 memory order = addOrder(o, "n:get(0),:set(0 add(n 1)),_ _:if(less-than(n 2) 1 0) 1;:;");
        OrderV4[] memory list = new OrderV4[](5);
        for (uint256 i = 0; i < 5; i++) {
            list[i] = order;
        }
        assertTrue(take(list).eq(LibDecimalFloat.packLossless(2, 0)), "counter must cap fills at 2 within the batch");
    }

    /// The fix does not change cross-transaction persistence: a "fill once"
    /// order filled in one `takeOrders5` is exhausted in a later one (the
    /// calculate write is still persisted by handleIO). tx1 fills 1, tx2 fills 0.
    function testCrossTransactionPersistenceUnchanged() external {
        mockTransfers();
        address o = owner("dave");
        fund(o);
        OrderV4 memory order = addOrder(o, "used:get(0),:set(0 1),_ _:if(used 0 1) 1;:;");
        OrderV4[] memory single = new OrderV4[](1);
        single[0] = order;
        assertTrue(take(single).eq(LibDecimalFloat.packLossless(1, 0)), "tx1 fills once");
        assertTrue(take(single).isZero(), "tx2 sees persisted state and skips");
    }

    /// Threaded writes are key-specific: A writes key 0; B reads key 1 (a
    /// different key) and must miss, filling normally. Total 2. Guards against
    /// an overlay that seeds values under the wrong key.
    function testThreadingIsKeySpecific() external {
        mockTransfers();
        address o = owner("erin");
        fund(o);
        OrderV4[] memory list = new OrderV4[](2);
        list[0] = addOrder(o, ":set(0 1),_ _:1 1;:;");
        list[1] = addOrder(o, "used:get(1),_ _:if(used 0 1) 1;:;");
        assertTrue(take(list).eq(LibDecimalFloat.packLossless(2, 0)), "read of an unset key must miss");
    }

    /// A dead order (never added -> OrderNotFound) interleaved in the batch is
    /// not evaluated and commits nothing, so it does not disturb same-owner
    /// threading through the store: [A fills+writes, DEAD, B reads] -> B sees A's
    /// committed write and skips -> total 1.
    function testDeadOrderInterleavedDoesNotBreakSameOwnerThreading() external {
        mockTransfers();
        address o = owner("frank");
        fund(o);
        OrderV4 memory a = addOrder(o, ":set(0 1),_ _:1 1;:;");
        OrderV4 memory b = addOrder(o, "used:get(0),_ _:if(used 0 1) 1;:;");
        // A never-added order (distinct nonce) with matching tokens is dead.
        OrderV4 memory dead = OrderV4({
            owner: a.owner,
            evaluable: a.evaluable,
            validInputs: a.validInputs,
            validOutputs: a.validOutputs,
            nonce: bytes32(uint256(0xdead))
        });
        OrderV4[] memory list = new OrderV4[](3);
        list[0] = a;
        list[1] = dead;
        list[2] = b;
        assertTrue(take(list).eq(LibDecimalFloat.packLossless(1, 0)), "dead order must not break same-owner threading");
    }

    /// Same no-state-change rule for an order untaken via OrderExceedsMaxRatio
    /// (distinct from the zero-amount path). A (ratio 1000 > maximumIORatio 1)
    /// writes key 0 but is untaken; B reads key 0, sees it unset, and fills ->
    /// total 1. (The old overlay recorded before the skip check, so B saw A's
    /// write and skipped -> total 0.)
    function testUntakenRatioOrderWriteNotVisibleToLaterSameOwnerOrder() external {
        mockTransfers();
        address o = owner("grace");
        fund(o);
        OrderV4 memory a = addOrder(o, ":set(0 1),_ _:1 1000;:;");
        OrderV4 memory b = addOrder(o, "used:get(0),_ _:if(used 0 1) 1;:;");
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](2);
        orders[0] =
            TakeOrderConfigV5({order: a, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        orders[1] =
            TakeOrderConfigV5({order: b, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        TakeOrdersConfigV6 memory config = TakeOrdersConfigV6({
            orders: orders,
            minimumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
            maximumIORatio: LibDecimalFloat.packLossless(1, 0),
            IOIsInput: true,
            data: ""
        });
        vm.prank(iBob);
        (Float total,) = iRaindex.takeOrders5(config);
        assertTrue(total.eq(LibDecimalFloat.packLossless(1, 0)), "ratio-untaken order's write must not be visible to B");
    }

    /// The strongest no-state-change check: an untaken order's write is not
    /// persisted even when a LATER same-owner order in the same batch fills (and
    /// runs handleIO). A is zero-amount-untaken and writes key 0; B (same owner)
    /// fills and writes key 1. B's handleIO must commit only B's OWN writes,
    /// never A's. In a later tx a reader of key 0 fills (A's write gone) while a
    /// reader of key 1 skips (B's write kept) -> total exactly 1.
    ///
    /// Under the old overlay A's write was seeded into B's calculate, the
    /// interpreter echoed it back into B's output kvs, and B's handleIO committed
    /// A's key 0 too -> the key-0 reader would also skip -> total 0.
    function testUntakenWriteNotPersistedByLaterFilledSibling() external {
        mockTransfers();
        address o = owner("judy");
        fund(o);
        OrderV4 memory a = addOrder(o, ":set(0 1),_ _:0 1;:;");
        OrderV4 memory b = addOrder(o, ":set(1 9),_ _:1 1;:;");
        OrderV4[] memory batch = new OrderV4[](2);
        batch[0] = a;
        batch[1] = b;
        // tx1: A untaken, B fills; B's handleIO must commit only B's own key 1.
        assertTrue(take(batch).eq(LibDecimalFloat.packLossless(1, 0)), "tx1: only B fills");
        // tx2: key-0 reader fills (A's write never persisted), key-1 reader skips
        // (B's write persisted) -> total 1.
        OrderV4 memory readKey0 = addOrder(o, "used:get(0),_ _:if(used 0 1) 1;:;");
        OrderV4 memory readKey1 = addOrder(o, "used:get(1),_ _:if(used 0 1) 1;:;");
        OrderV4[] memory readers = new OrderV4[](2);
        readers[0] = readKey0;
        readers[1] = readKey1;
        assertTrue(
            take(readers).eq(LibDecimalFloat.packLossless(1, 0)),
            "tx2: key-0 reader fills (A gone), key-1 reader skips (B kept)"
        );
    }

    /// Same persistence check for an order untaken via OrderExceedsMaxRatio: a
    /// ratio-untaken A (ratio 1000 > maximumIORatio 1) writing key 0, followed by
    /// a filling same-owner B writing key 1, must not leak key 0 into the store
    /// via B's handleIO. In a later tx the key-0 reader fills and the key-1
    /// reader skips -> total 1.
    function testUntakenRatioWriteNotPersistedByLaterFilledSibling() external {
        mockTransfers();
        address o = owner("mallory");
        fund(o);
        OrderV4 memory a = addOrder(o, ":set(0 1),_ _:1 1000;:;");
        OrderV4 memory b = addOrder(o, ":set(1 9),_ _:1 1;:;");
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](2);
        orders[0] =
            TakeOrderConfigV5({order: a, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        orders[1] =
            TakeOrderConfigV5({order: b, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        TakeOrdersConfigV6 memory config = TakeOrdersConfigV6({
            orders: orders,
            minimumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
            maximumIORatio: LibDecimalFloat.packLossless(1, 0),
            IOIsInput: true,
            data: ""
        });
        // tx1: A ratio-untaken, B fills; B's handleIO must commit only key 1.
        vm.prank(iBob);
        (Float total1,) = iRaindex.takeOrders5(config);
        assertTrue(total1.eq(LibDecimalFloat.packLossless(1, 0)), "tx1: only B fills");
        // tx2: key-0 reader fills (A gone), key-1 reader skips (B kept) -> 1.
        OrderV4 memory readKey0 = addOrder(o, "used:get(0),_ _:if(used 0 1) 1;:;");
        OrderV4 memory readKey1 = addOrder(o, "used:get(1),_ _:if(used 0 1) 1;:;");
        OrderV4[] memory readers = new OrderV4[](2);
        readers[0] = readKey0;
        readers[1] = readKey1;
        assertTrue(
            take(readers).eq(LibDecimalFloat.packLossless(1, 0)),
            "tx2: ratio-untaken key-0 reader fills (A gone), key-1 reader skips (B kept)"
        );
    }

    /// recordVaultInput / recordVaultOutput populate the per-order balance-DIFF
    /// cells that handle-IO reads as the amounts traded this fill: the input diff
    /// is context<3 4>() and the output diff is context<4 4>() (column 3/4, row
    /// CONTEXT_VAULT_IO_BALANCE_DIFF). A 1:1 fill of 1 must report diff 1 on both
    /// legs. The order's handle-IO ensures exactly that, so it fills only if both
    /// diffs are written correctly; a wrong/zero diff makes handle-IO revert.
    function testVaultBalanceDiffsWrittenToHandleIOContext() external {
        mockTransfers();
        address o = owner("nate");
        fund(o);
        OrderV4[] memory list = new OrderV4[](1);
        list[0] =
            addOrder(o, "_ _:1 1;:ensure(every(equal-to(context<3 4>() 1) equal-to(context<4 4>() 1)) \"diffs\");");
        assertTrue(
            take(list).eq(LibDecimalFloat.packLossless(1, 0)),
            "order fills only if both balance-diffs (input + output) reach handle-IO context"
        );
    }

    /// M01 collapsed two near-duplicate ContextV2 emissions into a single
    /// fully-populated emit per filled order. ContextV2 is the orderbook's
    /// contract-context notification consumed off-chain (the subgraph), so it
    /// must be emitted exactly once per fill — never dropped, never doubled.
    function testContextV2EmittedOncePerFilledOrder() external {
        mockTransfers();
        address o = owner("olive");
        fund(o);
        OrderV4[] memory list = new OrderV4[](1);
        list[0] = addOrder(o, "_ _:1 1;:;");
        vm.recordLogs();
        take(list);
        Vm.Log[] memory logs = vm.getRecordedLogs();
        bytes32 sig = keccak256("ContextV2(address,bytes32[][])");
        uint256 count;
        for (uint256 i = 0; i < logs.length; i++) {
            if (logs[i].topics.length > 0 && logs[i].topics[0] == sig) {
                count++;
            }
        }
        assertEq(count, 1, "exactly one ContextV2 emitted per filled order");
    }

    /// handleIO commits an order's calculate-phase `kvs` to the store BEFORE
    /// running that order's handle-IO eval, so handle-IO sees its own calculate
    /// `:set` within the same evaluation (the contract intent: "handle IO to see
    /// a consistent view on sets from calculate IO"). An order whose calculate
    /// sets key 0 to 5 and whose handle-IO requires get(0) == 5 fills only if
    /// that ordering holds; deferring the commit to after the handle-IO eval
    /// would make handle-IO read an unset 0 and revert.
    function testHandleIOSeesSameOrderCalculateSet() external {
        mockTransfers();
        address o = owner("frank");
        fund(o);
        OrderV4[] memory list = new OrderV4[](1);
        list[0] = addOrder(o, ":set(0 5),_ _:1 1;:ensure(equal-to(get(0) 5) \"hio sees calc\");");
        assertTrue(
            take(list).eq(LibDecimalFloat.packLossless(1, 0)),
            "order fills only if handle-IO sees its own calculate set"
        );
    }
}
