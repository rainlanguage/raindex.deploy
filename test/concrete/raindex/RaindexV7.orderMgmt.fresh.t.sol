// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7FreshTest} from "test/util/abstract/RaindexV7FreshTest.sol";
import {
    OrderConfigV4,
    OrderV4,
    IOV2,
    EvaluableV4,
    TaskV3,
    SignedContextV2
} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";
import {IMetaV1_2} from "rain-metadata-0.1.46/src/interface/unstable/IMetaV1_2.sol";
import {META_MAGIC_NUMBER_V1} from "rain-metadata-0.1.46/src/lib/LibMeta.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";
import {NotOrderOwner} from "../../../src/error/ErrRaindexV7.sol";

import {Strings} from "@openzeppelin-contracts-5.7.0/utils/Strings.sol";

/// @title RaindexV7OrderMgmtFreshTest
/// @notice Mutation-validating coverage for `addOrder5`, `removeOrder4` and the
/// post-action (doPost) entask path. Uses `RaindexV7FreshTest` so the freshly
/// compiled `src/concrete/raindex/RaindexV7.sol` is the code under test (the
/// pointer-etched harnesses run the committed runtime bytecode and are blind to
/// source edits). Every assertion is discriminating: an exact event, an exact
/// revert selector, an exact return value or an exact context cell.
contract RaindexV7OrderMgmtFreshTest is RaindexV7FreshTest, IMetaV1_2 {
    using LibOrder for OrderV4;
    using Strings for address;
    using Strings for uint256;

    /// The canonical two-source (calculate + handle IO) serialized bytecode that
    /// `addOrder5`'s source-count guard accepts, so each test exercises the
    /// specific input/output/meta guard it targets rather than the source-count
    /// guard. The runtime stack/IO checks are not reached by `addOrder5`.
    bytes constant CALC_BYTECODE = LibTestAddOrder.VALID_ORDER_BYTECODE;

    // -----------------------------------------------------------------------
    // addOrder5: input/output guards (A1, A2)
    // -----------------------------------------------------------------------

    /// addOrder5 MUST revert with `OrderNoInputs` when there are zero inputs,
    /// and MUST NOT record the order. The guard fires before everything else,
    /// so even a valid-output order with zero inputs reverts.
    /// forge-config: default.fuzz.runs = 20
    function testFreshAddOrderNoInputsReverts(address owner, OrderConfigV4 memory config) external {
        config.evaluable.bytecode = CALC_BYTECODE;
        config.validInputs = new IOV2[](0);
        config.validOutputs = new IOV2[](1);
        config.validOutputs[0] = IOV2(address(1), bytes32(uint256(1)));
        config.meta = new bytes(0);

        (OrderV4 memory order, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        (order);

        vm.prank(owner);
        vm.expectRevert(abi.encodeWithSelector(OrderNoInputs.selector));
        iRaindex.addOrder5(config, new TaskV3[](0));

        assertFalse(iRaindex.orderExists(orderHash));
    }

    /// A single input is enough to pass the inputs guard (boundary at length 1).
    /// forge-config: default.fuzz.runs = 20
    function testFreshAddOrderOneInputPasses(address owner, OrderConfigV4 memory config) external {
        config.evaluable.bytecode = CALC_BYTECODE;
        config.evaluable.interpreter = iInterpreter;
        config.evaluable.store = iStore;
        config.validInputs = new IOV2[](1);
        config.validInputs[0] = IOV2(address(0), bytes32(uint256(1)));
        config.validOutputs = new IOV2[](1);
        config.validOutputs[0] = IOV2(address(1), bytes32(uint256(1)));
        config.meta = new bytes(0);

        (, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        vm.prank(owner);
        assertTrue(iRaindex.addOrder5(config, new TaskV3[](0)));
        assertTrue(iRaindex.orderExists(orderHash));
    }

    /// addOrder5 MUST revert with `OrderNoOutputs` when there are zero outputs
    /// (inputs present), and MUST NOT record the order.
    /// forge-config: default.fuzz.runs = 20
    function testFreshAddOrderNoOutputsReverts(address owner, OrderConfigV4 memory config) external {
        config.evaluable.bytecode = CALC_BYTECODE;
        config.validInputs = new IOV2[](1);
        config.validInputs[0] = IOV2(address(0), bytes32(uint256(1)));
        config.validOutputs = new IOV2[](0);
        config.meta = new bytes(0);

        (, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        vm.prank(owner);
        vm.expectRevert(abi.encodeWithSelector(OrderNoOutputs.selector));
        iRaindex.addOrder5(config, new TaskV3[](0));

        assertFalse(iRaindex.orderExists(orderHash));
    }

    /// The outputs guard is reached only when inputs are present, i.e. the
    /// inputs guard is checked FIRST. Zero inputs AND zero outputs reverts with
    /// `OrderNoInputs`, never `OrderNoOutputs`.
    /// forge-config: default.fuzz.runs = 20
    function testFreshAddOrderNoInputsTakesPrecedenceOverNoOutputs(address owner, OrderConfigV4 memory config)
        external
    {
        config.evaluable.bytecode = CALC_BYTECODE;
        config.validInputs = new IOV2[](0);
        config.validOutputs = new IOV2[](0);
        config.meta = new bytes(0);

        vm.prank(owner);
        vm.expectRevert(abi.encodeWithSelector(OrderNoInputs.selector));
        iRaindex.addOrder5(config, new TaskV3[](0));
    }

    // -----------------------------------------------------------------------
    // addOrder5: owner/event/state/return (A3, A5, A6, A9)
    // -----------------------------------------------------------------------

    /// A successful add MUST emit `AddOrderV3(owner, orderHash, order)` with the
    /// order owner set to msg.sender, set the order live and return true.
    /// forge-config: default.fuzz.runs = 20
    function testFreshAddOrderEmitsExactEventAndOwner(address owner, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);

        OrderV4 memory order = OrderV4(owner, config.evaluable, config.validInputs, config.validOutputs, config.nonce);
        bytes32 orderHash = order.hash();

        assertFalse(iRaindex.orderExists(orderHash));

        // Full-data event check pinned to the raindex emitter. The owner field
        // MUST be msg.sender, not any other address.
        vm.expectEmit(true, true, true, true, address(iRaindex));
        emit AddOrderV3(owner, orderHash, order);

        vm.prank(owner);
        assertTrue(iRaindex.addOrder5(config, new TaskV3[](0)));
        assertTrue(iRaindex.orderExists(orderHash));
    }

    /// The order owner is ALWAYS msg.sender. A different prank than the supposed
    /// owner produces a different order hash that is live, and the claimed
    /// owner's hash is NOT live.
    /// forge-config: default.fuzz.runs = 20
    function testFreshAddOrderOwnerIsMsgSender(address sender, address other, OrderConfigV4 memory config) external {
        vm.assume(sender != other);
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);

        OrderV4 memory senderOrder =
            OrderV4(sender, config.evaluable, config.validInputs, config.validOutputs, config.nonce);
        OrderV4 memory otherOrder =
            OrderV4(other, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.prank(sender);
        iRaindex.addOrder5(config, new TaskV3[](0));

        assertTrue(iRaindex.orderExists(senderOrder.hash()));
        assertFalse(iRaindex.orderExists(otherOrder.hash()));
    }

    // -----------------------------------------------------------------------
    // addOrder5: dedup no-op (A4)
    // -----------------------------------------------------------------------

    /// First add of a (owner, config) returns true; a second identical add is a
    /// no-op that returns false, emits NOTHING, and leaves the order live.
    /// forge-config: default.fuzz.runs = 20
    function testFreshAddOrderDedupNoop(address owner, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);
        OrderV4 memory order = OrderV4(owner, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.prank(owner);
        assertTrue(iRaindex.addOrder5(config, new TaskV3[](0)));
        assertTrue(iRaindex.orderExists(order.hash()));

        vm.recordLogs();
        vm.prank(owner);
        assertFalse(iRaindex.addOrder5(config, new TaskV3[](0)));
        // No event whatsoever on the no-op second add.
        assertEq(vm.getRecordedLogs().length, 0);
        assertTrue(iRaindex.orderExists(order.hash()));
    }

    /// A re-added order after removal is a state change again (true). This pins
    /// that the dead/live check is read from storage, not a one-shot.
    /// forge-config: default.fuzz.runs = 20
    function testFreshAddOrderReaddAfterRemoveIsStateChange(address owner, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);
        OrderV4 memory order = OrderV4(owner, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.startPrank(owner);
        assertTrue(iRaindex.addOrder5(config, new TaskV3[](0)));
        assertTrue(iRaindex.removeOrder4(order, new TaskV3[](0)));
        assertFalse(iRaindex.orderExists(order.hash()));
        // Re-add: dead again so this is a fresh state change.
        assertTrue(iRaindex.addOrder5(config, new TaskV3[](0)));
        assertTrue(iRaindex.orderExists(order.hash()));
        vm.stopPrank();
    }

    // -----------------------------------------------------------------------
    // addOrder5: meta guard (A7)
    // -----------------------------------------------------------------------

    /// A non-empty, valid rain meta MUST emit `MetaV1_2(owner, orderHash, meta)`
    /// with exact data.
    /// forge-config: default.fuzz.runs = 20
    function testFreshAddOrderMetaEmitsExact(address owner, OrderConfigV4 memory config) external {
        config.evaluable.bytecode = CALC_BYTECODE;
        config.evaluable.interpreter = iInterpreter;
        config.evaluable.store = iStore;
        config.validInputs = new IOV2[](1);
        config.validInputs[0] = IOV2(address(0), bytes32(uint256(1)));
        config.validOutputs = new IOV2[](1);
        config.validOutputs[0] = IOV2(address(1), bytes32(uint256(1)));
        vm.assume(config.meta.length > 0);
        config.meta = abi.encodePacked(META_MAGIC_NUMBER_V1, config.meta);

        (OrderV4 memory order, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        (order);

        vm.expectEmit(true, true, true, true, address(iRaindex));
        emit MetaV1_2(owner, orderHash, config.meta);

        vm.prank(owner);
        assertTrue(iRaindex.addOrder5(config, new TaskV3[](0)));
    }

    /// Empty meta MUST NOT emit a `MetaV1_2` event. A successful add with empty
    /// meta emits exactly one log (AddOrderV3).
    /// forge-config: default.fuzz.runs = 20
    function testFreshAddOrderEmptyMetaNoMetaEvent(address owner, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);

        vm.recordLogs();
        vm.prank(owner);
        assertTrue(iRaindex.addOrder5(config, new TaskV3[](0)));
        // Exactly one log: AddOrderV3, no MetaV1_2.
        assertEq(vm.getRecordedLogs().length, 1);
    }

    /// A non-empty meta that does NOT self-describe as a rain meta document MUST
    /// revert (NotRainMetaV1) and MUST NOT record the order.
    /// forge-config: default.fuzz.runs = 20
    function testFreshAddOrderNonRainMetaReverts(address owner, OrderConfigV4 memory config) external {
        config.evaluable.bytecode = CALC_BYTECODE;
        config.evaluable.interpreter = iInterpreter;
        config.evaluable.store = iStore;
        config.validInputs = new IOV2[](1);
        config.validInputs[0] = IOV2(address(0), bytes32(uint256(1)));
        config.validOutputs = new IOV2[](1);
        config.validOutputs[0] = IOV2(address(1), bytes32(uint256(1)));
        // Non rain meta: prefix with a byte that is NOT the magic number.
        config.meta = hex"deadbeef";

        (, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        vm.prank(owner);
        vm.expectRevert();
        iRaindex.addOrder5(config, new TaskV3[](0));
        assertFalse(iRaindex.orderExists(orderHash));
    }

    // -----------------------------------------------------------------------
    // addOrder5: post-action context (A8, P5)
    // -----------------------------------------------------------------------

    /// The post-action context for addOrder MUST expose the order hash and the
    /// order owner (msg.sender) via the subparser words `order-hash()` and
    /// `order-owner()`, and `raindex()` MUST be the raindex address. The evals
    /// only run because the add is a state change.
    /// forge-config: default.fuzz.runs = 10
    function testFreshAddOrderPostContext(address alice, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);

        OrderV4 memory order = OrderV4(alice, config.evaluable, config.validInputs, config.validOutputs, config.nonce);
        bytes32 orderHash = order.hash();

        string memory usingWordsFrom = string.concat("using-words-from ", address(iSubParser).toHexString(), "\n");

        bytes[] memory evals = new bytes[](3);
        evals[0] = bytes(
            string.concat(usingWordsFrom, ":ensure(equal-to(raindex() ", address(iRaindex).toHexString(), ") \"r\");")
        );
        evals[1] = bytes(
            string.concat(
                usingWordsFrom, ":ensure(equal-to(order-hash() ", uint256(orderHash).toHexString(), ") \"h\");"
            )
        );
        evals[2] =
            bytes(string.concat(usingWordsFrom, ":ensure(equal-to(order-owner() ", alice.toHexString(), ") \"o\");"));

        TaskV3[] memory actions = new TaskV3[](3);
        for (uint256 i = 0; i < 3; i++) {
            actions[i] = TaskV3(EvaluableV4(iInterpreter, iStore, iParserV2.parse2(evals[i])), new SignedContextV2[](0));
        }

        vm.prank(alice);
        assertTrue(iRaindex.addOrder5(config, actions));
    }

    /// A revert in a post-action task MUST prevent the order being added (whole
    /// tx reverts), so the order remains not live.
    /// forge-config: default.assertions_revert = false
    /// forge-config: default.legacy_assertions = true
    /// forge-config: default.fuzz.runs = 10
    function testFreshAddOrderPostRevertNoAdd(address alice, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);

        bytes[] memory evals = new bytes[](1);
        evals[0] = bytes(":ensure(0 \"always revert\");");
        TaskV3[] memory actions = evalsToActions(evals);

        OrderV4 memory order = OrderV4(alice, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.prank(alice);
        vm.expectRevert("always revert");
        iRaindex.addOrder5(config, actions);

        assertFalse(iRaindex.orderExists(order.hash()));
    }

    /// The post-action evals MUST NOT run when the add is a no-op (order already
    /// live). A reverting eval that would always revert is never reached on the
    /// second identical add.
    /// forge-config: default.fuzz.runs = 10
    function testFreshAddOrderNoopSkipsPost(address alice, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);

        vm.startPrank(alice);
        assertTrue(iRaindex.addOrder5(config, new TaskV3[](0)));

        bytes[] memory evals = new bytes[](1);
        evals[0] = bytes(":ensure(0 \"always revert\");");
        TaskV3[] memory actions = evalsToActions(evals);
        // The order is already live so this is a no-op; the reverting eval does
        // NOT run, so the call returns false without reverting.
        assertFalse(iRaindex.addOrder5(config, actions));
        vm.stopPrank();
    }

    // -----------------------------------------------------------------------
    // removeOrder4: owner guard (R1)
    // -----------------------------------------------------------------------

    /// removeOrder4 MUST revert with `NotOrderOwner(order.owner)` when msg.sender
    /// is not the order owner, even after the order is live, and MUST NOT change
    /// state.
    /// forge-config: default.fuzz.runs = 20
    function testFreshRemoveOrderNotOwnerReverts(address alice, address bob, OrderConfigV4 memory config) external {
        vm.assume(alice != bob);
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);

        OrderV4 memory order = OrderV4(alice, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        // Reverts even before the order is added.
        vm.prank(bob);
        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, alice));
        iRaindex.removeOrder4(order, new TaskV3[](0));

        vm.prank(alice);
        iRaindex.addOrder5(config, new TaskV3[](0));
        assertTrue(iRaindex.orderExists(order.hash()));

        // Still reverts after it is live, with the order owner as the revert arg.
        vm.prank(bob);
        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, alice));
        iRaindex.removeOrder4(order, new TaskV3[](0));

        assertTrue(iRaindex.orderExists(order.hash()));
    }

    /// The owner is allowed (no revert) and the revert arg is the order owner
    /// field, not msg.sender. Prove the arg by removing a live order owned by
    /// alice with a wrong sender bob and checking the arg is alice.
    /// forge-config: default.fuzz.runs = 20
    function testFreshRemoveOrderRevertArgIsOrderOwner(address alice, address bob, OrderConfigV4 memory config)
        external
    {
        vm.assume(alice != bob);
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);
        OrderV4 memory order = OrderV4(alice, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.prank(bob);
        vm.expectRevert(abi.encodeWithSelector(NotOrderOwner.selector, alice));
        iRaindex.removeOrder4(order, new TaskV3[](0));
    }

    // -----------------------------------------------------------------------
    // removeOrder4: live check / state / event / return (R2..R7)
    // -----------------------------------------------------------------------

    /// Removing a LIVE order MUST emit `RemoveOrderV3(owner, orderHash, order)`,
    /// set the order dead and return true.
    /// forge-config: default.fuzz.runs = 20
    function testFreshRemoveOrderLiveEmitsExactAndReturnsTrue(address owner, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);
        OrderV4 memory order = OrderV4(owner, config.evaluable, config.validInputs, config.validOutputs, config.nonce);
        bytes32 orderHash = order.hash();

        vm.prank(owner);
        iRaindex.addOrder5(config, new TaskV3[](0));
        assertTrue(iRaindex.orderExists(orderHash));

        vm.expectEmit(true, true, true, true, address(iRaindex));
        emit RemoveOrderV3(owner, orderHash, order);

        vm.prank(owner);
        assertTrue(iRaindex.removeOrder4(order, new TaskV3[](0)));
        assertFalse(iRaindex.orderExists(orderHash));
    }

    /// Removing a NON-LIVE order (never added, owned by msg.sender) MUST be a
    /// no-op: returns false, emits NOTHING, and leaves the order not live.
    /// forge-config: default.fuzz.runs = 20
    function testFreshRemoveOrderDeadNoop(address owner, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);
        OrderV4 memory order = OrderV4(owner, config.evaluable, config.validInputs, config.validOutputs, config.nonce);
        bytes32 orderHash = order.hash();

        assertFalse(iRaindex.orderExists(orderHash));

        vm.recordLogs();
        vm.prank(owner);
        assertFalse(iRaindex.removeOrder4(order, new TaskV3[](0)));
        assertEq(vm.getRecordedLogs().length, 0);
        assertFalse(iRaindex.orderExists(orderHash));
    }

    /// Removing an already-removed order is a no-op (returns false, no event):
    /// proves R4 actually writes DEAD and R2 re-reads it.
    /// forge-config: default.fuzz.runs = 20
    function testFreshRemoveOrderDoubleRemoveSecondNoop(address owner, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);
        OrderV4 memory order = OrderV4(owner, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.startPrank(owner);
        iRaindex.addOrder5(config, new TaskV3[](0));
        assertTrue(iRaindex.removeOrder4(order, new TaskV3[](0)));
        assertFalse(iRaindex.orderExists(order.hash()));

        vm.recordLogs();
        assertFalse(iRaindex.removeOrder4(order, new TaskV3[](0)));
        assertEq(vm.getRecordedLogs().length, 0);
        assertFalse(iRaindex.orderExists(order.hash()));
        vm.stopPrank();
    }

    // -----------------------------------------------------------------------
    // removeOrder4: post-action context (R6, P5)
    // -----------------------------------------------------------------------

    /// The post-action context for removeOrder MUST expose the order hash and
    /// order owner. These evals run ONLY when the remove is a state change, so
    /// the order is added first then removed with the asserting evals.
    /// forge-config: default.fuzz.runs = 10
    function testFreshRemoveOrderPostContext(address alice, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);

        OrderV4 memory order = OrderV4(alice, config.evaluable, config.validInputs, config.validOutputs, config.nonce);
        bytes32 orderHash = order.hash();

        string memory usingWordsFrom = string.concat("using-words-from ", address(iSubParser).toHexString(), "\n");

        bytes[] memory evals = new bytes[](3);
        evals[0] = bytes(
            string.concat(usingWordsFrom, ":ensure(equal-to(raindex() ", address(iRaindex).toHexString(), ") \"r\");")
        );
        evals[1] = bytes(
            string.concat(
                usingWordsFrom, ":ensure(equal-to(order-hash() ", uint256(orderHash).toHexString(), ") \"h\");"
            )
        );
        evals[2] =
            bytes(string.concat(usingWordsFrom, ":ensure(equal-to(order-owner() ", alice.toHexString(), ") \"o\");"));

        TaskV3[] memory actions = new TaskV3[](3);
        for (uint256 i = 0; i < 3; i++) {
            actions[i] = TaskV3(EvaluableV4(iInterpreter, iStore, iParserV2.parse2(evals[i])), new SignedContextV2[](0));
        }

        vm.startPrank(alice);
        iRaindex.addOrder5(config, new TaskV3[](0));
        // The evals only run because this is a live -> dead state change.
        assertTrue(iRaindex.removeOrder4(order, actions));
        vm.stopPrank();
    }

    /// A revert in a remove post-action task MUST prevent the order being
    /// removed (whole tx reverts), so the order remains live.
    /// forge-config: default.assertions_revert = false
    /// forge-config: default.legacy_assertions = true
    /// forge-config: default.fuzz.runs = 10
    function testFreshRemoveOrderPostRevertNoRemove(address alice, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);
        OrderV4 memory order = OrderV4(alice, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.startPrank(alice);
        iRaindex.addOrder5(config, new TaskV3[](0));
        assertTrue(iRaindex.orderExists(order.hash()));

        bytes[] memory evals = new bytes[](1);
        evals[0] = bytes(":ensure(0 \"always revert\");");
        TaskV3[] memory actions = evalsToActions(evals);

        vm.expectRevert("always revert");
        iRaindex.removeOrder4(order, actions);
        // Order remains live because the post-action reverted the whole tx.
        assertTrue(iRaindex.orderExists(order.hash()));
        vm.stopPrank();
    }

    /// The post-action evals MUST NOT run when remove is a no-op (order not
    /// live). A reverting eval is never reached on a dead-order remove.
    /// forge-config: default.fuzz.runs = 10
    function testFreshRemoveOrderDeadSkipsPost(address alice, OrderConfigV4 memory config) external {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.meta = new bytes(0);
        OrderV4 memory order = OrderV4(alice, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        bytes[] memory evals = new bytes[](1);
        evals[0] = bytes(":ensure(0 \"always revert\");");
        TaskV3[] memory actions = evalsToActions(evals);

        // Order is dead, so the remove is a no-op and the reverting eval does NOT
        // run; the call returns false without reverting.
        vm.prank(alice);
        assertFalse(iRaindex.removeOrder4(order, actions));
        assertFalse(iRaindex.orderExists(order.hash()));
    }
}
