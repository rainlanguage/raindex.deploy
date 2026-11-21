// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {NegativeVaultBalance, NegativeVaultBalanceChange, NegativePull, NegativePush} from "src/error/ErrRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {LibRainDeploy} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {IRaindexV7VaultBalanceHarness} from "test/util/interface/IRaindexV7VaultBalanceHarness.sol";

/// @title RaindexV7VaultBalanceRevertTest
/// @notice Audit A08-5 / A08-6 (#2535): the vault-balance helpers reject a
/// negative `amount` (`NegativeVaultBalanceChange`) and a decrease that would
/// drive the balance below zero (`NegativeVaultBalance`).
contract RaindexV7VaultBalanceRevertTest is Test {
    using LibDecimalFloat for Float;

    IRaindexV7VaultBalanceHarness internal harness;
    address internal owner = makeAddr("owner");
    address internal token = makeAddr("token");
    // Non-zero vault id so the internal-balance branch (not the vault-0 token
    // transfer branch) is exercised.
    bytes32 internal constant VAULT = bytes32(uint256(1));

    function setUp() external {
        // pullTokens/pushTokens read token decimals via the deployed TOFU
        // contract, so it must exist before the negative-amount check is reached.
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        // Etch the runtime code so the large RaindexV7-derived harness isn't
        // subject to the EIP-170 creation size limit.
        harness = IRaindexV7VaultBalanceHarness(payable(address(uint160(uint256(keccak256("vault.balance.harness"))))));
        vm.etch(address(harness), vm.getDeployedCode("RaindexV7VaultBalanceHarness.sol:RaindexV7VaultBalanceHarness"));
    }

    /// A08-6: increase with a negative amount reverts.
    function testIncreaseNegativeAmountReverts() external {
        vm.expectRevert(
            abi.encodeWithSelector(NegativeVaultBalanceChange.selector, LibDecimalFloat.packLossless(-1, 0))
        );
        harness.exposedIncrease(owner, token, VAULT, LibDecimalFloat.packLossless(-1, 0));
    }

    /// A08-6: decrease with a negative amount reverts.
    function testDecreaseNegativeAmountReverts() external {
        vm.expectRevert(
            abi.encodeWithSelector(NegativeVaultBalanceChange.selector, LibDecimalFloat.packLossless(-1, 0))
        );
        harness.exposedDecrease(owner, token, VAULT, LibDecimalFloat.packLossless(-1, 0));
    }

    /// A08-5: decreasing below the (zero) balance reverts NegativeVaultBalance.
    function testDecreaseBelowZeroReverts() external {
        vm.expectRevert(abi.encodeWithSelector(NegativeVaultBalance.selector, LibDecimalFloat.packLossless(-1, 0)));
        harness.exposedDecrease(owner, token, VAULT, LibDecimalFloat.packLossless(1, 0));
    }

    /// A08-7: pulling a negative amount reverts NegativePull (a real token is
    /// used so the TOFU decimals read passes before the negative check).
    function testPullNegativeAmountReverts() external {
        MockToken realToken = new MockToken("Token", "TKN", 18);
        vm.expectRevert(NegativePull.selector);
        harness.exposedPull(owner, address(realToken), LibDecimalFloat.packLossless(-1, 0));
    }

    /// A08-7: pushing a negative amount reverts NegativePush.
    function testPushNegativeAmountReverts() external {
        MockToken realToken = new MockToken("Token", "TKN", 18);
        vm.expectRevert(NegativePush.selector);
        harness.exposedPush(owner, address(realToken), LibDecimalFloat.packLossless(-1, 0));
    }

    /// Pulling an amount that does NOT divide evenly into the token's fixed
    /// decimals rounds the truncated fixed-decimal amount UP by one base unit
    /// (favouring the dex, never the counterparty). With a 6-decimal token and a
    /// Float of 15e-7 (= 0.0000015), the lossy conversion truncates to 1 base
    /// unit (0.000001) but the lost remainder forces a round-up to 2 base units.
    /// The harness pulls exactly 2 base units from the owner and reports 2 as the
    /// fixed-decimal amount. Pins the `if (!lossless) { ++amount18; }` rounding:
    /// a mutation that drops the increment pulls 1 instead of 2.
    function testPullRoundsUpOnLoss() external {
        MockToken realToken = new MockToken("Token", "TKN", 6);
        // Owner holds and approves enough to cover the rounded-up pull.
        realToken.mint(owner, 10);
        vm.prank(owner);
        realToken.approve(address(harness), 10);

        // 15e-7 at 6 decimals: 15 * 10^(-7+6) = 15 / 10 = 1, remainder 5 lost.
        (uint256 amount, uint8 decimals) =
            harness.exposedPull(owner, address(realToken), LibDecimalFloat.packLossless(15, -7));

        assertEq(decimals, 6, "decimals");
        assertEq(amount, 2, "rounded-up fixed-decimal amount is 2 not 1");
        // The contract actually pulled the rounded-up 2 base units.
        assertEq(realToken.balanceOf(owner), 8, "owner balance after pull");
        assertEq(realToken.balanceOf(address(harness)), 2, "harness balance after pull");
    }

    /// Pulling an amount that DOES divide evenly into the token's fixed decimals
    /// is lossless and is NOT rounded up. With a 6-decimal token and a Float of
    /// 3e-6 (= 0.000003) the conversion yields exactly 3 base units with no loss,
    /// so no increment happens. Pins the `!lossless` predicate guarding the
    /// round-up: a mutation that always increments (or inverts the predicate)
    /// would pull 4 instead of 3.
    function testPullNoRoundWhenLossless() external {
        MockToken realToken = new MockToken("Token", "TKN", 6);
        realToken.mint(owner, 10);
        vm.prank(owner);
        realToken.approve(address(harness), 10);

        // 3e-6 at 6 decimals: 3 * 10^(-6+6) = 3, lossless.
        (uint256 amount, uint8 decimals) =
            harness.exposedPull(owner, address(realToken), LibDecimalFloat.packLossless(3, -6));

        assertEq(decimals, 6, "decimals");
        assertEq(amount, 3, "lossless amount is 3 not rounded to 4");
        assertEq(realToken.balanceOf(owner), 7, "owner balance after pull");
        assertEq(realToken.balanceOf(address(harness)), 3, "harness balance after pull");
    }

    /// Pulling an amount that converts to zero fixed-decimal units skips the
    /// token transfer entirely (the `if (amount18 > 0)` guard). A zero Float
    /// passes the negative guard, converts losslessly to 0 base units, and
    /// `transferFrom` is NEVER invoked. Asserted with an `expectCall(..., 0)`:
    /// a zero-value ERC20 transfer succeeds (does not revert), so the discriminator
    /// is the call-count, not a revert. A mutation forcing the transfer on a zero
    /// amount makes exactly one `transferFrom(owner, harness, 0)` call and fails
    /// the zero-count expectation.
    function testPullZeroSkipsTransfer() external {
        MockToken realToken = new MockToken("Token", "TKN", 6);
        // Assert transferFrom is NOT called for a zero amount.
        vm.expectCall(
            address(realToken),
            abi.encodeWithSelector(IERC20.transferFrom.selector, owner, address(harness), uint256(0)),
            0
        );
        (uint256 amount, uint8 decimals) = harness.exposedPull(owner, address(realToken), Float.wrap(0));

        assertEq(decimals, 6, "decimals");
        assertEq(amount, 0, "zero amount returned");
        assertEq(realToken.balanceOf(owner), 0, "owner untouched");
        assertEq(realToken.balanceOf(address(harness)), 0, "harness untouched");
    }

    /// Pushing an amount that does NOT divide evenly into the token's fixed
    /// decimals TRUNCATES the fixed-decimal amount (rounds DOWN), the OPPOSITE of
    /// pull which rounds up. With a 6-decimal token and a Float of 15e-7 the
    /// lossy conversion yields 1 base unit and push transfers exactly 1 (NOT 2).
    /// Pins that push has no round-up: a mutation copying pull's `++amount`
    /// rounding into push would transfer 2 instead of 1.
    function testPushTruncatesOnLoss() external {
        MockToken realToken = new MockToken("Token", "TKN", 6);
        // The harness holds tokens to push out.
        realToken.mint(address(harness), 10);

        // 15e-7 at 6 decimals truncates to 1 (push does NOT round up).
        (uint256 amount, uint8 decimals) =
            harness.exposedPush(owner, address(realToken), LibDecimalFloat.packLossless(15, -7));

        assertEq(decimals, 6, "decimals");
        assertEq(amount, 1, "truncated fixed-decimal amount is 1 not 2");
        assertEq(realToken.balanceOf(owner), 1, "owner received truncated 1");
        assertEq(realToken.balanceOf(address(harness)), 9, "harness balance after push");
    }

    /// Pushing an amount that converts to zero fixed-decimal units skips the
    /// token transfer entirely (the `if (amount > 0)` guard). A zero Float
    /// converts losslessly to 0 base units, and `transfer` is NEVER invoked.
    /// Asserted with an `expectCall(..., 0)` since a zero-value ERC20 transfer
    /// succeeds rather than reverts: a mutation forcing the transfer on a zero
    /// amount makes exactly one `transfer(owner, 0)` call and fails the
    /// zero-count expectation.
    function testPushZeroSkipsTransfer() external {
        MockToken realToken = new MockToken("Token", "TKN", 6);
        // Assert transfer is NOT called for a zero amount.
        vm.expectCall(address(realToken), abi.encodeWithSelector(IERC20.transfer.selector, owner, uint256(0)), 0);
        (uint256 amount, uint8 decimals) = harness.exposedPush(owner, address(realToken), Float.wrap(0));

        assertEq(decimals, 6, "decimals");
        assertEq(amount, 0, "zero amount returned");
        assertEq(realToken.balanceOf(owner), 0, "owner untouched");
        assertEq(realToken.balanceOf(address(harness)), 0, "harness untouched");
    }

    /// Pulling a positive amount SMALLER than one base unit (the sub-unit
    /// "almost zero" range, 0 < x < 1 base unit) must round UP to exactly 1 base
    /// unit and pull it — it must NOT truncate to zero and skip the transfer,
    /// which would let a non-zero credit be funded by a zero pull. With a
    /// 6-decimal token and a Float of 1e-7 (= 0.1 base units) the lossy
    /// conversion truncates to 0, but because it is lossy the round-up forces 1.
    /// This pins the 0->1 ceil boundary specifically: testPullRoundsUpOnLoss
    /// exercises 1.5 -> 2 (already >= 1) and testPullZeroSkipsTransfer exercises
    /// exactly 0, so a mutation that only increments an already-non-zero
    /// truncation (`if (!lossless && amount18 != 0)`) survives both of those but
    /// is killed here.
    function testPullSubUnitRoundsUpToOne() external {
        MockToken realToken = new MockToken("Token", "TKN", 6);
        realToken.mint(owner, 10);
        vm.prank(owner);
        realToken.approve(address(harness), 10);

        // The contract pulls exactly 1 base unit (not 0) for the sub-unit amount.
        vm.expectCall(
            address(realToken),
            abi.encodeWithSelector(IERC20.transferFrom.selector, owner, address(harness), uint256(1)),
            1
        );
        // 1e-7 at 6 decimals: 1 * 10^(-7+6) = 0.1, truncates to 0 but lossy -> ++ -> 1.
        (uint256 amount, uint8 decimals) =
            harness.exposedPull(owner, address(realToken), LibDecimalFloat.packLossless(1, -7));

        assertEq(decimals, 6, "decimals");
        assertEq(amount, 1, "sub-unit amount rounds up to 1 not 0");
        assertEq(realToken.balanceOf(owner), 9, "owner balance after sub-unit pull");
        assertEq(realToken.balanceOf(address(harness)), 1, "harness pulled exactly 1");
    }

    /// Pushing a positive amount SMALLER than one base unit (sub-unit "almost
    /// zero") TRUNCATES to zero and skips the transfer — push rounds DOWN, so the
    /// residual stays in the contract rather than paying out a base unit of dust.
    /// With a 6-decimal token and a Float of 1e-7 (= 0.1 base units) push
    /// transfers nothing. Pins that push does NOT lift a sub-unit amount to 1
    /// (the over-pay direction): testPushTruncatesOnLoss exercises 1.5 -> 1
    /// (>= 1) and testPushZeroSkipsTransfer exercises exactly 0, so a mutation
    /// paying out 1 base unit for a sub-unit amount survives both but is killed
    /// here.
    function testPushSubUnitTruncatesToZero() external {
        MockToken realToken = new MockToken("Token", "TKN", 6);
        realToken.mint(address(harness), 10);

        // No transfer at all for a sub-unit (truncated-to-zero) push.
        vm.expectCall(address(realToken), abi.encodeWithSelector(IERC20.transfer.selector, owner, uint256(0)), 0);
        // 1e-7 at 6 decimals: 0.1 base units, push truncates DOWN to 0 -> skip.
        (uint256 amount, uint8 decimals) =
            harness.exposedPush(owner, address(realToken), LibDecimalFloat.packLossless(1, -7));

        assertEq(decimals, 6, "decimals");
        assertEq(amount, 0, "sub-unit amount truncates to 0 not 1");
        assertEq(realToken.balanceOf(owner), 0, "owner received nothing");
        assertEq(realToken.balanceOf(address(harness)), 10, "harness untouched");
    }

    /// Increasing a vault-0 balance ALWAYS returns (0, 0): vault 0 holds no
    /// internal balance, it pushes tokens straight out to the owner, so the
    /// reported old and new balances are both exactly zero regardless of the
    /// amount. Pins the return values so a mutation echoing the amount back is
    /// caught.
    function testIncreaseVaultZeroReturnsZero() external {
        MockToken realToken = new MockToken("Token", "TKN", 18);
        // The harness pushes tokens out to the owner for vault 0, so it needs a
        // balance to transfer.
        realToken.mint(address(harness), 1e18);

        (Float oldBalance, Float newBalance) =
            harness.exposedIncrease(owner, address(realToken), bytes32(0), LibDecimalFloat.packLossless(1, 0));

        assertTrue(oldBalance.eq(Float.wrap(0)), "old balance not zero");
        assertTrue(newBalance.eq(Float.wrap(0)), "new balance not zero");
        // The owner actually received the pushed tokens.
        assertEq(realToken.balanceOf(owner), 1e18, "owner balance");
    }

    /// Decreasing a non-zero vault SUBTRACTS the amount from the old balance:
    /// it returns (oldBalance, oldBalance - amount), persists the new balance to
    /// storage, and a follow-up read reflects the reduced balance. Pins the
    /// `.sub` arithmetic, the (old, new) return tuple ordering, and the storage
    /// write — a mutation that adds instead of subtracts, returns the amount or
    /// both-new, or skips the write is caught.
    function testDecreaseNonZeroSubtracts() external {
        // Seed the vault with 10 via an increase from the zero balance.
        (Float seedOld, Float seedNew) =
            harness.exposedIncrease(owner, token, VAULT, LibDecimalFloat.packLossless(10, 0));
        assertTrue(seedOld.eq(Float.wrap(0)), "seed old");
        assertTrue(seedNew.eq(LibDecimalFloat.packLossless(10, 0)), "seed new");

        // Decrease by 3: returns (10, 7).
        (Float oldBalance, Float newBalance) =
            harness.exposedDecrease(owner, token, VAULT, LibDecimalFloat.packLossless(3, 0));
        assertTrue(oldBalance.eq(LibDecimalFloat.packLossless(10, 0)), "old balance is pre-decrease 10");
        assertTrue(newBalance.eq(LibDecimalFloat.packLossless(7, 0)), "new balance is 10 - 3 = 7");

        // The reduced balance is persisted: reading the vault returns 7.
        assertTrue(
            harness.vaultBalance2(owner, token, VAULT).eq(LibDecimalFloat.packLossless(7, 0)), "stored balance is 7"
        );

        // A second decrease compounds off the stored 7, not the original 10:
        // returns (7, 5) and persists 5.
        (Float old2, Float new2) = harness.exposedDecrease(owner, token, VAULT, LibDecimalFloat.packLossless(2, 0));
        assertTrue(old2.eq(LibDecimalFloat.packLossless(7, 0)), "second old reads stored 7");
        assertTrue(new2.eq(LibDecimalFloat.packLossless(5, 0)), "second new is 7 - 2 = 5");
        assertTrue(
            harness.vaultBalance2(owner, token, VAULT).eq(LibDecimalFloat.packLossless(5, 0)), "stored balance is 5"
        );
    }

    /// Decreasing a vault-0 balance ALWAYS returns (0, 0) and PULLS tokens from
    /// the owner into the contract (vault 0 holds no internal balance, every
    /// decrease is a compound matching deposit). Pins both the (0, 0) return and
    /// the pull side-effect (owner's tokens move to the harness) so a mutation
    /// echoing the amount back, or pushing instead of pulling, or storing a
    /// balance, is caught.
    function testDecreaseVaultZeroPullsAndReturnsZero() external {
        MockToken realToken = new MockToken("Token", "TKN", 18);
        // The owner holds tokens and has approved the harness so the pull works.
        realToken.mint(owner, 5e18);
        vm.prank(owner);
        realToken.approve(address(harness), 5e18);

        (Float oldBalance, Float newBalance) =
            harness.exposedDecrease(owner, address(realToken), bytes32(0), LibDecimalFloat.packLossless(2, 0));

        assertTrue(oldBalance.eq(Float.wrap(0)), "vault-0 old balance not zero");
        assertTrue(newBalance.eq(Float.wrap(0)), "vault-0 new balance not zero");
        // Two whole tokens were pulled from the owner into the harness.
        assertEq(realToken.balanceOf(owner), 3e18, "owner balance after pull");
        assertEq(realToken.balanceOf(address(harness)), 2e18, "harness balance after pull");
    }
}
