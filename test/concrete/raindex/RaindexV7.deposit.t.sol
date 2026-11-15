// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {stdError} from "forge-std-1.17.0/src/Test.sol";
import {REVERTING_MOCK_BYTECODE, CONSOLE_ADDRESS} from "test/util/lib/LibTestConstants.sol";
import {RaindexV7ExternalMockTest} from "test/util/abstract/RaindexV7ExternalMockTest.sol";
import {TaskV3, EvaluableV4, IRaindexV7} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Reenteroor} from "test/util/concrete/Reenteroor.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {LibDecimalFloat, Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";
import {ReentrancyGuard} from "@openzeppelin-contracts-5.7.0/utils/ReentrancyGuard.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";

/// @title RaindexV7DepositTest
/// Tests depositing to a raindex.
contract RaindexV7DepositTest is RaindexV7ExternalMockTest {
    using LibDecimalFloat for Float;

    /// Tests that we can deposit some amount and view the new vault balance.
    /// forge-config: default.fuzz.runs = 100
    function testDepositSimple(address depositor, bytes32 vaultId, uint256 amount18) external {
        vm.assume(vaultId != bytes32(0));
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)) / 10);
        vm.prank(depositor);
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, depositor, address(iRaindex), amount18),
            abi.encode(true)
        );

        Float amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(amount18, 18);

        iRaindex.deposit5(address(iToken0), vaultId, amount, new TaskV3[](0));
        assertTrue(iRaindex.vaultBalance2(depositor, address(iToken0), vaultId).eq(amount));
    }

    /// Depositing zero should revert.
    /// forge-config: default.fuzz.runs = 100
    function testDepositZero(address depositor, bytes32 vaultId) external {
        vm.assume(vaultId != bytes32(0));
        vm.prank(depositor);
        vm.expectRevert(
            abi.encodeWithSelector(IRaindexV7.ZeroDepositAmount.selector, address(depositor), address(iToken0), vaultId)
        );
        iRaindex.deposit5(address(iToken0), vaultId, LibDecimalFloat.packLossless(0, 0), new TaskV3[](0));
    }

    /// Depositing a negative amount is caught by the same non-positive guard as
    /// zero and reverts ZeroDepositAmount (NOT NegativePull from pullTokens). The
    /// guard is `!depositAmount.gt(0)` which is true for both zero and negatives,
    /// so a negative deposit never reaches the token pull.
    /// forge-config: default.fuzz.runs = 100
    function testDepositNegative(address depositor, bytes32 vaultId, uint256 magnitude18) external {
        vm.assume(vaultId != bytes32(0));
        magnitude18 = bound(magnitude18, 1, uint256(int256(type(int224).max)) / 10);
        // A strictly negative deposit amount.
        // magnitude18 is bound above so safe to typecast.
        // forge-lint: disable-next-line(unsafe-typecast)
        Float amount = LibDecimalFloat.packLossless(-int256(magnitude18), -18);
        assertTrue(amount.lt(Float.wrap(0)));
        vm.prank(depositor);
        vm.expectRevert(
            abi.encodeWithSelector(IRaindexV7.ZeroDepositAmount.selector, address(depositor), address(iToken0), vaultId)
        );
        iRaindex.deposit5(address(iToken0), vaultId, amount, new TaskV3[](0));
    }

    /// Depositing vaultID of zero should revert.
    function testDepositZeroVaultId(address depositor, address token, Float amount) external {
        vm.assume(amount.gt(Float.wrap(0)));
        vm.prank(depositor);
        vm.expectRevert(abi.encodeWithSelector(IRaindexV7.ZeroVaultId.selector, address(depositor), address(token)));
        iRaindex.deposit5(token, bytes32(0), amount, new TaskV3[](0));
    }

    /// Test a warm deposit, which is the best case scenario for gas. In this
    /// case the storage backing the vault balance is already warm so an
    /// additional deposit gets a much cheaper sstore.
    function testDepositGas00() external {
        vm.pauseGasMetering();
        // warm up storage
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, address(this), address(iRaindex), 1),
            abi.encode(true)
        );
        iRaindex.deposit5(
            address(iToken0), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(1, -18), new TaskV3[](0)
        );
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, address(this), address(iRaindex), 1),
            abi.encode(true)
        );
        vm.resumeGasMetering();
        iRaindex.deposit5(
            address(iToken0), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(1, -18), new TaskV3[](0)
        );
    }

    /// Test a cold deposit, which is the worst case scenario for gas. In this
    /// case the storage backing the vault balance is cold so the first deposit
    /// gets a much more expensive sstore. Unfortunately this is the case for
    /// most deposits.
    function testDepositGas01() external {
        vm.pauseGasMetering();
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, address(this), address(iRaindex), 1),
            abi.encode(true)
        );
        vm.resumeGasMetering();
        iRaindex.deposit5(
            address(iToken0), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(1, -18), new TaskV3[](0)
        );
    }

    /// Any failure in the deposit should revert the entire transaction.
    /// forge-config: default.fuzz.runs = 100
    function testDepositFail(address depositor, bytes32 vaultId, uint256 amount18) external {
        vm.assume(vaultId != bytes32(0));
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)) / 10);
        Float amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(amount18, 18);

        // The token contract always reverts when not mocked.
        vm.prank(depositor);
        vm.expectRevert();
        iRaindex.deposit5(address(iToken0), vaultId, amount, new TaskV3[](0));

        // Mocking the token to return false should also revert.
        vm.prank(depositor);
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, depositor, address(iRaindex), amount18),
            abi.encode(false)
        );
        // This error string appears when the call completes but returns false.
        vm.expectRevert(abi.encodeWithSelector(SafeERC20.SafeERC20FailedOperation.selector, address(iToken0)));
        iRaindex.deposit5(address(iToken0), vaultId, amount, new TaskV3[](0));
    }

    /// Defines a deposit to be used in testDepositMany.
    /// @param depositor The address of the depositor.
    /// @param token The address of the token to deposit.
    /// @param vaultId The vaultId to deposit to.
    /// @param amount The amount to deposit.
    struct Action {
        address depositor;
        address token;
        bytes32 vaultId;
        uint256 amount18;
        Float amount;
    }

    /// Any combination of depositors, tokens, vaults, amounts should not cause
    /// collisions or other illogical outcomes.
    /// forge-config: default.fuzz.runs = 100
    function testDepositMany(Action[] memory actions) external {
        vm.assume(actions.length > 0);
        for (uint256 i = 0; i < actions.length; i++) {
            if (actions[i].vaultId == bytes32(0)) {
                actions[i].vaultId = bytes32(uint256(0x01 + i));
            }
            // Deposit amounts must be non-zero.
            actions[i].amount18 = bound(actions[i].amount18, 1, uint256(int256(type(int224).max)) / 10);
            actions[i].amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(actions[i].amount18, 18);
            // Avoid errors from attempting to etch precompiles.
            vm.assume(uint160(actions[i].token) < 1 || 10 < uint160(actions[i].token));
            vm.assume(actions[i].token.code.length == 0);
            vm.assume(actions[i].token != CONSOLE_ADDRESS);
        }

        for (uint256 i = 0; i < actions.length; i++) {
            vm.etch(actions[i].token, REVERTING_MOCK_BYTECODE);
            Float vaultBalanceBefore =
                iRaindex.vaultBalance2(actions[i].depositor, actions[i].token, actions[i].vaultId);
            vm.prank(actions[i].depositor);
            vm.mockCall(
                actions[i].token,
                abi.encodeWithSelector(
                    IERC20.transferFrom.selector,
                    actions[i].depositor,
                    address(iRaindex),
                    LibDecimalFloat.toFixedDecimalLossless(actions[i].amount, 18)
                ),
                abi.encode(true)
            );
            vm.mockCall(
                actions[i].token, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(uint8(18))
            );
            vm.expectEmit(false, false, false, true);
            emit DepositV2(actions[i].depositor, actions[i].token, actions[i].vaultId, actions[i].amount18);
            vm.record();
            vm.recordLogs();
            iRaindex.deposit5(actions[i].token, actions[i].vaultId, actions[i].amount, new TaskV3[](0));
            (bytes32[] memory reads, bytes32[] memory writes) = vm.accesses(address(iRaindex));
            assertEq(vm.getRecordedLogs().length, 1, "logs");
            // - reentrancy guard x3
            // - vault balance floats x2
            // - token decimals x2
            assertTrue(reads.length == 5, "reads");
            // // - reentrancy guard x2
            // // - vault balance x1
            assertTrue(writes.length == 4 || writes.length == 3, "writes");
            assertTrue(
                iRaindex.vaultBalance2(actions[i].depositor, actions[i].token, actions[i].vaultId)
                    .eq(actions[i].amount.add(vaultBalanceBefore)),
                "vault balance"
            );
        }
    }

    /// Depositing should emit an event with the sender and all deposit details.
    /// forge-config: default.fuzz.runs = 100
    function testDepositEvent(address depositor, bytes32 vaultId, uint256 amount18) external {
        vm.assume(vaultId != bytes32(0));
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)) / 10);
        vm.prank(depositor);
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, depositor, address(iRaindex), amount18),
            abi.encode(true)
        );
        vm.expectEmit(false, false, false, true);
        emit DepositV2(depositor, address(iToken0), vaultId, amount18);
        Float amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(amount18, 18);
        iRaindex.deposit5(address(iToken0), vaultId, amount, new TaskV3[](0));
    }

    /// Depositing should NOT allow reentrancy.
    /// forge-config: default.fuzz.runs = 100
    function testDepositReentrancy(
        address depositor,
        bytes32 vaultId,
        uint256 amount18,
        address reToken,
        bytes32 reVaultId,
        uint256 reAmount18
    ) external {
        vm.assume(vaultId != bytes32(0));
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)) / 10);
        reAmount18 = bound(reAmount18, 1, uint256(int256(type(int224).max)) / 10);
        vm.prank(depositor);
        Reenteroor reenteroor = new Reenteroor();
        Float amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(amount18, 18);
        Float reAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(reAmount18, 18);
        vm.mockCall(reToken, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(uint8(18)));
        vm.mockCall(
            address(reenteroor), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(uint8(18))
        );
        reenteroor.reenterWith(
            abi.encodeWithSelector(IRaindexV7.deposit5.selector, reToken, reVaultId, reAmount, new TaskV3[](0))
        );
        vm.expectRevert(abi.encodeWithSelector(ReentrancyGuard.ReentrancyGuardReentrantCall.selector));
        iRaindex.deposit5(address(reenteroor), vaultId, amount, new TaskV3[](0));
    }
}
