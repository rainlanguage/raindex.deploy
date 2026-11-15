// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";

import {Math} from "@openzeppelin-contracts-5.7.0/utils/math/Math.sol";

import {RaindexV7ExternalMockTest, REVERTING_MOCK_BYTECODE} from "test/util/abstract/RaindexV7ExternalMockTest.sol";
import {Reenteroor, IERC20} from "test/util/concrete/Reenteroor.sol";
import {TaskV3, IRaindexV7} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";

import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {
    LibDecimalFloatImplementation
} from "rain-math-float-0.3.11/src/lib/implementation/LibDecimalFloatImplementation.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";

/// @title RaindexV7WithdrawTest
/// Tests withdrawing from the raindex.
contract RaindexV7WithdrawTest is RaindexV7ExternalMockTest {
    using Math for uint256;

    using LibDecimalFloat for Float;
    using LibDecimalFloatImplementation for Float;

    uint256 internal sRunID;
    mapping(uint256 => mapping(address => bool)) internal sHasDeposit;

    /// Withdrawing vault ID zero is an error.
    function testWithdrawZeroVaultId(address alice, address token) external {
        vm.prank(alice);
        vm.expectRevert(abi.encodeWithSelector(IRaindexV7.ZeroVaultId.selector, alice, token));
        iRaindex.withdraw5(token, bytes32(0), Float.wrap(0), new TaskV3[](0));
    }

    /// Withdrawing a zero target amount should revert.
    /// forge-config: default.fuzz.runs = 100
    function testWithdrawZero(address alice, address token, bytes32 vaultId) external {
        vm.assume(vaultId != bytes32(0));
        vm.prank(alice);
        vm.expectRevert(abi.encodeWithSelector(ZeroWithdrawTargetAmount.selector, alice, token, vaultId));
        iRaindex.withdraw5(token, vaultId, Float.wrap(0), new TaskV3[](0));
    }

    /// Withdrawing a non-zero amount from an empty vault should be a noop.
    /// forge-config: default.fuzz.runs = 100
    function testWithdrawEmptyVault(address alice, bytes32 vaultId, uint256 amount18) external {
        vm.assume(vaultId != bytes32(0));
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)));
        vm.prank(alice);
        Float amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(amount18, 18);
        vm.expectEmit(false, false, false, true);
        emit WithdrawV2(alice, address(iToken0), vaultId, amount, Float.wrap(0), 0);
        vm.record();
        iRaindex.withdraw5(address(iToken0), vaultId, amount, new TaskV3[](0));
        (bytes32[] memory reads, bytes32[] memory writes) = vm.accesses(address(iRaindex));
        assertEq(reads.length, 6, "reads");
        assertEq(writes.length, 3, "writes");
    }

    /// Withdrawing the full amount from a vault should delete the vault.
    /// forge-config: default.fuzz.runs = 100
    function testWithdrawFullVault(address alice, bytes32 vaultId, uint256 depositAmount18, uint256 withdrawAmount18)
        external
    {
        vm.assume(vaultId != bytes32(0));
        depositAmount18 = bound(depositAmount18, 1, type(uint224).max / 10);
        withdrawAmount18 = bound(withdrawAmount18, depositAmount18, type(uint224).max / 10);
        vm.prank(alice);
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, alice, address(iRaindex), depositAmount18),
            abi.encode(true)
        );

        Float depositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(depositAmount18, 18);
        Float withdrawAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(withdrawAmount18, 18);

        iRaindex.deposit5(address(iToken0), vaultId, depositAmount, new TaskV3[](0));
        assertTrue(iRaindex.vaultBalance2(address(alice), address(iToken0), vaultId).eq(depositAmount));

        vm.prank(alice);
        vm.mockCall(
            address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector, alice, depositAmount18), abi.encode(true)
        );
        vm.expectEmit(false, false, false, true);
        emit WithdrawV2(alice, address(iToken0), vaultId, withdrawAmount, depositAmount, depositAmount18);
        iRaindex.withdraw5(address(iToken0), vaultId, withdrawAmount, new TaskV3[](0));
        assertTrue(iRaindex.vaultBalance2(address(alice), address(iToken0), vaultId).isZero(), "vault balance");
    }

    /// Withdrawing a partial amount from a vault should reduce the vault balance.
    /// forge-config: default.fuzz.runs = 100
    function testWithdrawPartialVault(address alice, bytes32 vaultId, uint256 depositAmount18, uint256 withdrawAmount18)
        external
    {
        vm.assume(vaultId != bytes32(0));
        depositAmount18 = bound(depositAmount18, 2, type(uint224).max / 10);
        withdrawAmount18 = bound(withdrawAmount18, 1, depositAmount18 - 1);
        vm.prank(alice);
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, alice, address(iRaindex), depositAmount18),
            abi.encode(true)
        );

        Float depositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(depositAmount18, 18);
        Float withdrawAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(withdrawAmount18, 18);

        iRaindex.deposit5(address(iToken0), vaultId, depositAmount, new TaskV3[](0));
        assertTrue(iRaindex.vaultBalance2(address(alice), address(iToken0), vaultId).eq(depositAmount));

        vm.prank(alice);
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transfer.selector, alice, withdrawAmount18),
            abi.encode(true)
        );
        vm.expectEmit(false, false, false, true);
        // The full withdraw amount is possible as it's only a partial withdraw.
        emit WithdrawV2(alice, address(iToken0), vaultId, withdrawAmount, withdrawAmount, withdrawAmount18);
        iRaindex.withdraw5(address(iToken0), vaultId, withdrawAmount, new TaskV3[](0));
        // The vault balance is reduced by the withdraw amount.
        assertTrue(
            iRaindex.vaultBalance2(address(alice), address(iToken0), vaultId).eq(depositAmount.sub(withdrawAmount))
        );
    }

    /// Any failure in the withdrawal should revert the entire transaction.
    /// forge-config: default.fuzz.runs = 100
    function testWithdrawFailure(address alice, bytes32 vaultId, uint256 depositAmount18, uint256 withdrawAmount18)
        external
    {
        vm.assume(vaultId != bytes32(0));
        depositAmount18 = bound(depositAmount18, 1, type(uint224).max / 10);
        withdrawAmount18 = bound(withdrawAmount18, 1, type(uint224).max / 10);
        vm.prank(alice);
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, alice, address(iRaindex), depositAmount18),
            abi.encode(true)
        );
        Float depositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(depositAmount18, 18);
        Float withdrawAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(withdrawAmount18, 18);
        iRaindex.deposit5(address(iToken0), vaultId, depositAmount, new TaskV3[](0));
        assertTrue(iRaindex.vaultBalance2(address(alice), address(iToken0), vaultId).eq(depositAmount));

        // The token contract always reverts when not mocked.
        vm.prank(alice);
        vm.expectRevert();
        iRaindex.withdraw5(address(iToken0), vaultId, withdrawAmount, new TaskV3[](0));

        vm.prank(alice);
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transfer.selector, alice, withdrawAmount18.min(depositAmount18)),
            abi.encode(false)
        );
        vm.expectRevert(abi.encodeWithSelector(SafeERC20.SafeERC20FailedOperation.selector, address(iToken0)));
        iRaindex.withdraw5(address(iToken0), vaultId, withdrawAmount, new TaskV3[](0));
    }

    /// Defines an action that can be taken in withdrawal tests.
    /// @param actionKind The kind of action to take. True for deposit, false
    /// for withdraw.
    /// @param alice The address taking action.
    /// @param token The token being deposited/withdrawn.
    /// @param vaultId The vault being deposited/withdrawn from.
    /// @param amount The amount being deposited/withdrawn.
    struct Action {
        bool actionKind;
        address alice;
        address token;
        bytes32 vaultId;
        uint256 amount;
        Float amountFloat;
        uint256 pairedWith;
        bool fresh;
    }

    /// Arbitrary interleavings of deposits and withdrawals should work across
    /// many depositors, tokens, and vaults.
    /// forge-config: default.fuzz.runs = 100
    function testWithdrawMany(Action[] memory actions) external {
        sRunID++;
        vm.assume(actions.length > 0);
        for (uint256 i = 0; i < actions.length; i++) {
            // Deposit and withdrawal amounts must be positive.
            actions[i].amount = bound(actions[i].amount, 1, type(uint224).max / 10);
            // Ensure the token doesn't hit some known address and cause bad
            // etching.
            actions[i].token = address(uint160(uint256(keccak256(abi.encodePacked(actions[i].token)))));
            actions[i].amountFloat = LibDecimalFloat.fromFixedDecimalLosslessPacked(actions[i].amount, 18);

            // If not a fresh action, pair it with a previous action to reuse the
            // same alice, vaultId, and token, creating related deposit/withdraw
            // sequences.
            if (!actions[i].fresh && i > 0) {
                actions[i].pairedWith = bound(actions[i].pairedWith, 0, i - 1);
                actions[i].alice = actions[actions[i].pairedWith].alice;
                actions[i].vaultId = actions[actions[i].pairedWith].vaultId;
                actions[i].token = actions[actions[i].pairedWith].token;
            }
        }
        Action memory action;
        for (uint256 i = 0; i < actions.length; i++) {
            action = actions[i];
            if (action.vaultId == bytes32(0)) {
                action.vaultId = bytes32(uint256(0x01));
            }
            vm.etch(action.token, REVERTING_MOCK_BYTECODE);
            Float balance = iRaindex.vaultBalance2(action.alice, action.token, action.vaultId);

            vm.prank(action.alice);
            if (action.actionKind || !sHasDeposit[sRunID][action.token]) {
                sHasDeposit[sRunID][action.token] = true;
                vm.mockCall(
                    action.token,
                    abi.encodeWithSelector(
                        IERC20.transferFrom.selector, action.alice, address(iRaindex), uint256(action.amount)
                    ),
                    abi.encode(true)
                );
                vm.mockCall(
                    action.token, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(uint8(18))
                );

                vm.expectEmit(false, false, false, true);
                emit DepositV2(action.alice, action.token, action.vaultId, action.amount);
                iRaindex.deposit5(action.token, action.vaultId, action.amountFloat, new TaskV3[](0));
                assertTrue(
                    iRaindex.vaultBalance2(action.alice, action.token, action.vaultId)
                        .eq(balance.add(action.amountFloat)),
                    "vault balance on deposit"
                );
            } else {
                Float expectedActualAmount = action.amountFloat.min(balance);
                uint256 expectedActualAmount18 = expectedActualAmount.toFixedDecimalLossless(18);
                vm.mockCall(
                    action.token,
                    abi.encodeWithSelector(IERC20.transfer.selector, action.alice, expectedActualAmount18),
                    abi.encode(true)
                );
                vm.mockCall(
                    action.token, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(uint8(18))
                );

                if (expectedActualAmount.gt(Float.wrap(0))) {
                    vm.expectEmit(false, false, false, true);
                    emit WithdrawV2(
                        action.alice,
                        action.token,
                        action.vaultId,
                        action.amountFloat,
                        expectedActualAmount,
                        expectedActualAmount18
                    );
                }
                iRaindex.withdraw5(action.token, action.vaultId, action.amountFloat, new TaskV3[](0));
                assertTrue(
                    iRaindex.vaultBalance2(action.alice, action.token, action.vaultId)
                        .eq(balance.sub(expectedActualAmount)),
                    "vault balance on withdraw"
                );
            }
        }
    }
}
