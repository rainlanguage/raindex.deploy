// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalMockTest} from "test/util/abstract/RaindexV7ExternalMockTest.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";
import {LibDecimalFloat, Float} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {TaskV3} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {TOFUOutcome} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {ITOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/interface/ITOFUTokenDecimals.sol";

/// @title RaindexV7VaultBalanceVault0Test
/// @notice Vault ID 0 has no internal balance; its effective balance is the
/// MINIMUM of the owner's wallet token balance and their approval to Raindex
/// (the most Raindex could ever pull). Existing maximum-input/output tests that
/// exercise vault 0 mock `balanceOf` and `allowance` to the SAME value, so a
/// bug that returned only the balance, only the approval, or the max of the two
/// would survive them. These pin the `min(balance, approval)` selection by
/// making the two differ.
contract RaindexV7VaultBalanceVault0Test is RaindexV7ExternalMockTest {
    using LibDecimalFloat for Float;

    /// When the approval is smaller than the wallet balance, the effective
    /// vault-0 balance is the approval (Raindex can't pull more than approved).
    function testVaultBalanceVault0ApprovalLimits(address owner, uint256 balance18, uint256 approval18) external {
        // iToken0 decimals are mocked to 18 by the base.
        balance18 = bound(balance18, 1, uint256(int256(type(int224).max)));
        approval18 = bound(approval18, 0, balance18 - 1);

        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.balanceOf.selector, owner), abi.encode(balance18));
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.allowance.selector, owner, address(iRaindex)),
            abi.encode(approval18)
        );

        Float effective = iRaindex.vaultBalance2(owner, address(iToken0), bytes32(0));
        // The smaller of the two (approval) is returned, not the balance.
        assertTrue(effective.eq(LibDecimalFloat.fromFixedDecimalLosslessPacked(approval18, 18)), "approval limits");
    }

    /// When the wallet balance is smaller than the approval, the effective
    /// vault-0 balance is the balance (Raindex can't pull tokens the owner
    /// doesn't hold).
    function testVaultBalanceVault0BalanceLimits(address owner, uint256 balance18, uint256 approval18) external {
        approval18 = bound(approval18, 1, uint256(int256(type(int224).max)));
        balance18 = bound(balance18, 0, approval18 - 1);

        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.balanceOf.selector, owner), abi.encode(balance18));
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.allowance.selector, owner, address(iRaindex)),
            abi.encode(approval18)
        );

        Float effective = iRaindex.vaultBalance2(owner, address(iToken0), bytes32(0));
        // The smaller of the two (balance) is returned, not the approval.
        assertTrue(effective.eq(LibDecimalFloat.fromFixedDecimalLosslessPacked(balance18, 18)), "balance limits");
    }

    /// A wallet balance above the Float coefficient range (int224.max, ~1.35e67)
    /// must still return the approval when the approval is the binding `min`. The
    /// balance is lossy-packed (saturates the low digit) like the approval, so a
    /// balance over int224.max does not revert the vault-0 read on the min path.
    function testVaultBalanceVault0OverflowBalanceReturnsApproval(address owner, uint256 approval18) external {
        // Wallet balance just over int224.max (lossless packing would overflow);
        // approval is the small binding min.
        uint256 balance18 = uint256(int256(type(int224).max)) + 1;
        approval18 = bound(approval18, 0, 1e30);

        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.balanceOf.selector, owner), abi.encode(balance18));
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.allowance.selector, owner, address(iRaindex)),
            abi.encode(approval18)
        );

        // The smaller of the two (approval) is returned; the extreme balance
        // saturates instead of reverting CoefficientOverflow.
        Float effective = iRaindex.vaultBalance2(owner, address(iToken0), bytes32(0));
        assertTrue(effective.eq(LibDecimalFloat.fromFixedDecimalLosslessPacked(approval18, 18)), "approval limits");
    }

    /// Deposits to a non-zero vault to seed the TOFU store with the token's
    /// decimals (18). The deposit makes `pullTokens` persist the decimals via the
    /// mutating TOFU read so subsequent read-only reads have a stored value to
    /// compare against.
    function _seedTofuDecimals(address depositor) internal {
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, depositor, address(iRaindex), 1),
            abi.encode(true)
        );
        vm.prank(depositor);
        iRaindex.deposit5(address(iToken0), bytes32(uint256(1)), LibDecimalFloat.packLossless(1, -18), new TaskV3[](0));
    }

    /// A vault-0 balance read MUST revert `TokenDecimalsReadFailure` when the
    /// token's decimals have become inconsistent with the previously stored TOFU
    /// value. Without the guard, the stale stored decimals would be used to scale
    /// the balance and approval, silently misreporting the effective balance.
    function testVaultBalanceVault0TofuInconsistentReverts(address depositor) external {
        _seedTofuDecimals(depositor);

        // The token now reports a DIFFERENT number of decimals than was stored.
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(uint8(6)));

        vm.expectRevert(
            abi.encodeWithSelector(
                ITOFUTokenDecimals.TokenDecimalsReadFailure.selector, address(iToken0), TOFUOutcome.Inconsistent
            )
        );
        iRaindex.vaultBalance2(depositor, address(iToken0), bytes32(0));
    }

    /// A vault-0 balance read MUST revert `TokenDecimalsReadFailure` when the
    /// token can no longer be read for decimals at all (read failure) after a
    /// value was previously stored.
    function testVaultBalanceVault0TofuReadFailureReverts(address depositor) external {
        _seedTofuDecimals(depositor);

        // The token now reverts on `decimals()` (REVERTING_MOCK_BYTECODE under
        // the cleared mock), producing a ReadFailure outcome.
        vm.clearMockedCalls();

        vm.expectRevert(
            abi.encodeWithSelector(
                ITOFUTokenDecimals.TokenDecimalsReadFailure.selector, address(iToken0), TOFUOutcome.ReadFailure
            )
        );
        iRaindex.vaultBalance2(depositor, address(iToken0), bytes32(0));
    }

    /// Positive control: when the token's decimals remain CONSISTENT with the
    /// stored TOFU value, a vault-0 read does NOT revert and returns the
    /// effective balance (min of balance and approval).
    function testVaultBalanceVault0TofuConsistentOk(address depositor, uint256 balance18, uint256 approval18) external {
        _seedTofuDecimals(depositor);

        balance18 = bound(balance18, 1, uint256(int256(type(int224).max)));
        approval18 = bound(approval18, 0, balance18 - 1);

        // decimals() still returns 18 (consistent with the stored value).
        vm.mockCall(
            address(iToken0), abi.encodeWithSelector(IERC20.balanceOf.selector, depositor), abi.encode(balance18)
        );
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.allowance.selector, depositor, address(iRaindex)),
            abi.encode(approval18)
        );

        Float effective = iRaindex.vaultBalance2(depositor, address(iToken0), bytes32(0));
        assertTrue(effective.eq(LibDecimalFloat.fromFixedDecimalLosslessPacked(approval18, 18)), "consistent ok");
    }
}
