// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeCast} from "@openzeppelin-contracts-5.7.0/utils/math/SafeCast.sol";
import {RaindexV7FreshTakeOrderTest} from "test/util/abstract/RaindexV7FreshTakeOrderTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {
    IRaindexV7,
    OrderV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    TaskV3,
    SignedContextV2
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";
import {LibNamespace} from "rainlang-0.2.35/src/lib/ns/LibNamespace.sol";
import {StateNamespace} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";
import {Vm} from "forge-std-1.17.0/src/Test.sol";

/// A `takeOrders5` order skipped for any reason, at any position in the batch,
/// never runs its handle-io and never moves its vaults, while every taken order
/// in the same batch still runs its handle-io.
contract RaindexV7TakeOrderSkippedOrderTest is RaindexV7FreshTakeOrderTest {
    using LibDecimalFloat for Float;

    uint8 internal constant TAKEN = 0;
    uint8 internal constant CALCULATE_REVERT = 1;
    uint8 internal constant SHORT_STACK = 2;
    uint8 internal constant EXCEEDS_MAX_RATIO = 3;
    uint8 internal constant ZERO_AMOUNT = 4;
    uint8 internal constant NOT_FOUND = 5;
    uint8 internal constant KIND_COUNT = 6;

    address internal alice = address(uint160(uint256(keccak256("alice.rain.test"))));
    address internal bob = address(uint160(uint256(keccak256("bob.rain.test"))));

    function vaultId(uint256 i) internal pure returns (bytes32) {
        return bytes32(i + 1);
    }

    function takenKey(uint256 i) internal pure returns (bytes32) {
        return bytes32(0x100 + i);
    }

    function skippedKey(uint256 i) internal pure returns (bytes32) {
        return bytes32(0x200 + i);
    }

    function aliceStore(bytes32 key) internal view returns (bytes32) {
        return
            iStore.get(
                LibNamespace.qualifyNamespace(StateNamespace.wrap(uint256(uint160(alice))), address(iRaindex)), key
            );
    }

    function setHandle(bytes32 key) internal pure returns (bytes memory) {
        return bytes(string.concat(":set(", vm.toString(key), " 0x01);"));
    }

    function expression(uint8 kind, uint256 i, bool ensureHandle) internal pure returns (bytes memory) {
        if (kind == TAKEN) {
            return bytes.concat("_ _:1 1;", setHandle(takenKey(i)));
        }
        bytes memory handle = ensureHandle ? bytes(":ensure(0 \"handle ran\");") : setHandle(skippedKey(i));
        if (kind == CALCULATE_REVERT) {
            return bytes.concat(":ensure(0 \"calc\"),_ _:1 1;", handle);
        } else if (kind == SHORT_STACK) {
            return bytes.concat("_:1;", handle);
        } else if (kind == EXCEEDS_MAX_RATIO) {
            return bytes.concat("_ _:1 2;", handle);
        } else if (kind == ZERO_AMOUNT) {
            return bytes.concat("_ _:0 1;", handle);
        } else {
            return bytes.concat("_ _:1 1;", handle);
        }
    }

    function skipEvent(uint8 kind) internal pure returns (bytes32) {
        if (kind == CALCULATE_REVERT || kind == SHORT_STACK) {
            return IRaindexV7.OrderCalculateReverted.selector;
        } else if (kind == EXCEEDS_MAX_RATIO) {
            return IRaindexV7.OrderExceedsMaxRatio.selector;
        } else if (kind == ZERO_AMOUNT) {
            return IRaindexV7.OrderZeroAmount.selector;
        } else {
            return IRaindexV7.OrderNotFound.selector;
        }
    }

    function isSkipEvent(bytes32 topic) internal pure returns (bool) {
        return topic == IRaindexV7.OrderCalculateReverted.selector || topic == IRaindexV7.OrderExceedsMaxRatio.selector
            || topic == IRaindexV7.OrderZeroAmount.selector || topic == IRaindexV7.OrderNotFound.selector;
    }

    function mockTokens() internal {
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
    }

    function addOrders(uint8[] memory kinds, bool ensureHandle) internal returns (TakeOrderConfigV5[] memory) {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](kinds.length);
        for (uint256 i = 0; i < kinds.length; i++) {
            vm.prank(alice);
            iRaindex.deposit5(address(iToken1), vaultId(i), LibDecimalFloat.packLossless(10, 0), new TaskV3[](0));
            OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
                vm,
                alice,
                expression(kinds[i], i, ensureHandle),
                address(iToken0),
                vaultId(i),
                address(iToken1),
                vaultId(i)
            );
            if (kinds[i] == NOT_FOUND) {
                vm.prank(alice);
                iRaindex.removeOrder4(order, new TaskV3[](0));
            }
            orders[i] = TakeOrderConfigV5(order, 0, 0, new SignedContextV2[](0));
        }
        return orders;
    }

    function countLogs(Vm.Log[] memory logs, bytes32 topic, bytes memory data) internal view returns (uint256) {
        uint256 found;
        for (uint256 j = 0; j < logs.length; j++) {
            if (
                logs[j].emitter == address(iRaindex) && logs[j].topics[0] == topic
                    && keccak256(logs[j].data) == keccak256(data)
            ) {
                found++;
            }
        }
        return found;
    }

    function checkBatch(uint8[] memory kinds, bool ensureHandle) internal {
        mockTokens();
        TakeOrderConfigV5[] memory orders = addOrders(kinds, ensureHandle);
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(orders);
        config.maximumIORatio = LibDecimalFloat.packLossless(15, -1);

        vm.recordLogs();
        vm.prank(bob);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);
        Vm.Log[] memory logs = vm.getRecordedLogs();

        uint256 takenCount;
        for (uint256 i = 0; i < kinds.length; i++) {
            checkOrder(kinds[i], i, orders[i].order, logs);
            if (kinds[i] == TAKEN) {
                takenCount++;
            }
        }

        uint256 skipEvents;
        for (uint256 j = 0; j < logs.length; j++) {
            if (logs[j].emitter == address(iRaindex) && isSkipEvent(logs[j].topics[0])) {
                skipEvents++;
            }
        }
        assertEq(skipEvents, kinds.length - takenCount, "skip event count");

        Float expected = LibDecimalFloat.packLossless(SafeCast.toInt256(takenCount), 0);
        assertTrue(totalTakerInput.eq(expected), "totalTakerInput");
        assertTrue(totalTakerOutput.eq(expected), "totalTakerOutput");
    }

    function checkOrder(uint8 kind, uint256 i, OrderV4 memory order, Vm.Log[] memory logs) internal view {
        Float output = iRaindex.vaultBalance2(alice, address(iToken1), vaultId(i));
        Float input = iRaindex.vaultBalance2(alice, address(iToken0), vaultId(i));
        if (kind == TAKEN) {
            assertTrue(output.eq(LibDecimalFloat.packLossless(9, 0)), "taken output vault");
            assertTrue(input.eq(LibDecimalFloat.packLossless(1, 0)), "taken input vault");
            assertEq(aliceStore(takenKey(i)), bytes32(uint256(1)), "taken handle-io set");
        } else {
            assertTrue(output.eq(LibDecimalFloat.packLossless(10, 0)), "skipped output vault");
            assertTrue(input.isZero(), "skipped input vault");
            assertEq(aliceStore(skippedKey(i)), bytes32(0), "skipped handle-io set");
            assertEq(
                countLogs(logs, skipEvent(kind), abi.encode(bob, alice, LibOrder.hash(order))), 1, "one skip event"
            );
        }
    }

    function checkBothHandles(uint8[] memory kinds) internal {
        uint256 snapshot = vm.snapshotState();
        checkBatch(kinds, true);
        vm.revertToState(snapshot);
        checkBatch(kinds, false);
    }

    function checkEveryPosition(uint8 kind) internal {
        for (uint256 position = 0; position < 3; position++) {
            uint8[] memory kinds = new uint8[](3);
            kinds[position] = kind;
            uint256 snapshot = vm.snapshotState();
            checkBothHandles(kinds);
            vm.revertToState(snapshot);
        }
    }

    function testCalculateRevertSkippedAtAnyPositionRunsNoHandleIO() external {
        checkEveryPosition(CALCULATE_REVERT);
    }

    function testShortStackSkippedAtAnyPositionRunsNoHandleIO() external {
        checkEveryPosition(SHORT_STACK);
    }

    function testExceedsMaxRatioSkippedAtAnyPositionRunsNoHandleIO() external {
        checkEveryPosition(EXCEEDS_MAX_RATIO);
    }

    function testZeroAmountSkippedAtAnyPositionRunsNoHandleIO() external {
        checkEveryPosition(ZERO_AMOUNT);
    }

    function testNotFoundSkippedAtAnyPositionRunsNoHandleIO() external {
        checkEveryPosition(NOT_FOUND);
    }

    function testEverySkipCauseInOneBatchRunsNoHandleIO() external {
        uint8[] memory kinds = new uint8[](8);
        kinds[0] = CALCULATE_REVERT;
        kinds[1] = TAKEN;
        kinds[2] = SHORT_STACK;
        kinds[3] = EXCEEDS_MAX_RATIO;
        kinds[4] = TAKEN;
        kinds[5] = ZERO_AMOUNT;
        kinds[6] = NOT_FOUND;
        kinds[7] = CALCULATE_REVERT;
        checkBothHandles(kinds);
    }

    function testAllSkippedBatchRunsNoHandleIO() external {
        uint8[] memory kinds = new uint8[](5);
        kinds[0] = CALCULATE_REVERT;
        kinds[1] = SHORT_STACK;
        kinds[2] = EXCEEDS_MAX_RATIO;
        kinds[3] = ZERO_AMOUNT;
        kinds[4] = NOT_FOUND;
        checkBothHandles(kinds);
    }

    function testFuzzSkippedOrdersRunNoHandleIO(uint256 length, uint256 seed, bool ensureHandle) external {
        length = bound(length, 1, 6);
        uint8[] memory kinds = new uint8[](length);
        for (uint256 i = 0; i < length; i++) {
            kinds[i] = uint8(uint256(keccak256(abi.encode(seed, i))) % KIND_COUNT);
        }
        checkBatch(kinds, ensureHandle);
    }
}
