// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {LibRainDeploy} from "rain-deploy-0.1.36/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {TaskV3, IRaindexV7} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {IRaindexV7Stub} from "test/util/abstract/IRaindexV7Stub.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";
import {TOFUOutcome} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {ITOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/interface/ITOFUTokenDecimals.sol";

/// @title RaindexV7DepositWithdrawHarnessTest
/// @notice Solvency-critical accounting for the external deposit5/withdraw5
/// entrypoints, exercised against REAL ERC20 tokens (not always-true mocks) via
/// a freshly-compiled harness so source mutations are live. Pins:
/// - withdraw capping at the vault balance (`targetAmount.min(balance)`),
/// - deposit crediting exactly the pulled amount,
/// - the deposit/withdraw event amounts (Float and onchain uint256),
/// - non-18-decimal token scaling end-to-end.
contract RaindexV7DepositWithdrawHarnessTest is Test, IRaindexV7Stub {
    using LibDecimalFloat for Float;

    IRaindexV7 internal harness;
    bytes32 internal constant VAULT = bytes32(uint256(1));

    function setUp() external {
        // deposit5/withdraw5 read token decimals via the deployed TOFU contract.
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        harness = IRaindexV7(payable(address(uint160(uint256(keccak256("deposit.withdraw.harness"))))));
        vm.etch(address(harness), vm.getDeployedCode("RaindexV7.sol:RaindexV7"));
    }

    /// Deposits `amount18` (18-decimal token) into VAULT for `alice` and returns
    /// the Float amount credited. Real token transferFrom moves the tokens.
    function _deposit18(address alice, uint256 amount18) internal returns (Float) {
        MockToken token = sToken;
        token.mint(alice, amount18);
        vm.prank(alice);
        token.approve(address(harness), amount18);
        Float amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(amount18, 18);
        vm.prank(alice);
        harness.deposit5(address(token), VAULT, amount, new TaskV3[](0));
        return amount;
    }

    MockToken internal sToken;

    /// A withdraw whose target STRICTLY EXCEEDS the vault balance is capped at
    /// the balance: `withdrawAmount = targetAmount.min(currentVaultBalance)`. The
    /// vault is fully drained (never overdrawn), exactly the balance amount of
    /// real tokens moves back to the owner, and the event reports `withdrawAmount`
    /// as the balance (NOT the larger target). A mutation flipping the cap to
    /// `.max(...)` would set `withdrawAmount` to the target, attempt to push and
    /// decrement more than the vault holds (reverting `NegativeVaultBalance`), and
    /// never reach these assertions.
    function testWithdrawTargetExceedsBalanceCappedAtBalance() external {
        sToken = new MockToken("Token", "TKN", 18);
        address alice = makeAddr("alice");
        uint256 depositAmount18 = 5e18;
        Float depositAmount = _deposit18(alice, depositAmount18);

        // The harness now holds 5e18 tokens; alice holds none.
        assertEq(sToken.balanceOf(address(harness)), depositAmount18, "harness holds deposit");
        assertEq(sToken.balanceOf(alice), 0, "alice emptied by deposit");

        Float targetAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(10e18, 18);

        // The event reports the CAPPED withdrawAmount (5e18 balance), not the
        // requested target (10e18); the onchain amount is the balance too.
        vm.expectEmit(false, false, false, true);
        emit WithdrawV2(alice, address(sToken), VAULT, targetAmount, depositAmount, depositAmount18);
        vm.prank(alice);
        harness.withdraw5(address(sToken), VAULT, targetAmount, new TaskV3[](0));

        // Exactly the balance (5e18) was returned to alice, never the target.
        assertEq(sToken.balanceOf(alice), depositAmount18, "alice received exactly the balance");
        assertEq(sToken.balanceOf(address(harness)), 0, "harness fully drained");
        // The vault is emptied, never overdrawn.
        assertTrue(harness.vaultBalance2(alice, address(sToken), VAULT).isZero(), "vault drained to zero");
    }

    /// A deposit credits the vault by EXACTLY the deposited amount and the
    /// `DepositV2` event reports the onchain (fixed-decimal) amount pulled. Uses a
    /// 6-decimal token so the Float<->fixed scaling is exercised end-to-end: a
    /// Float of 7 whole tokens becomes 7e6 base units onchain and the vault holds
    /// 7. Pins the credit (`increaseVaultBalance`) and the event amount; a mutation
    /// crediting zero, the wrong amount, or emitting the wrong onchain value is
    /// caught.
    function testDepositCreditsExactAmountAndEvent() external {
        sToken = new MockToken("USDC", "USDC", 6);
        address alice = makeAddr("alice");
        uint256 amount6 = 7_000_000; // 7 whole tokens at 6 decimals.
        sToken.mint(alice, amount6);
        vm.prank(alice);
        sToken.approve(address(harness), amount6);

        Float amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(amount6, 6);

        // The event reports the exact onchain amount pulled (7e6 base units).
        vm.expectEmit(false, false, false, true);
        emit DepositV2(alice, address(sToken), VAULT, amount6);
        vm.prank(alice);
        harness.deposit5(address(sToken), VAULT, amount, new TaskV3[](0));

        // The vault is credited exactly 7 whole tokens, and 7e6 base units moved.
        assertTrue(harness.vaultBalance2(alice, address(sToken), VAULT).eq(amount), "vault credited exact deposit");
        assertEq(sToken.balanceOf(address(harness)), amount6, "harness pulled exactly amount");
        assertEq(sToken.balanceOf(alice), 0, "alice emptied");
    }

    /// Two sequential deposits into the same vault COMPOUND additively: the second
    /// deposit's credit is added to the first, not overwritten. Pins that
    /// `increaseVaultBalance` reads-then-adds the prior balance (a mutation
    /// overwriting with just the new amount would report 3 not 5).
    function testDepositCompoundsAdditively() external {
        sToken = new MockToken("Token", "TKN", 18);
        address alice = makeAddr("alice");
        _deposit18(alice, 2e18);
        _deposit18(alice, 3e18);
        assertTrue(
            harness.vaultBalance2(alice, address(sToken), VAULT)
                .eq(LibDecimalFloat.fromFixedDecimalLosslessPacked(5e18, 18)),
            "two deposits compound to 5"
        );
    }

    /// A deposit of exactly zero reverts `ZeroDepositAmount` and moves no tokens.
    /// Pins the `!depositAmount.gt(0)` guard: a mutation dropping/inverting it
    /// would let a zero deposit through (emitting an event / touching storage for
    /// nothing).
    function testDepositZeroReverts() external {
        sToken = new MockToken("Token", "TKN", 18);
        address alice = makeAddr("alice");
        vm.prank(alice);
        vm.expectRevert(abi.encodeWithSelector(IRaindexV7.ZeroDepositAmount.selector, alice, address(sToken), VAULT));
        harness.deposit5(address(sToken), VAULT, Float.wrap(0), new TaskV3[](0));
    }

    /// A withdraw target of exactly zero reverts `ZeroWithdrawTargetAmount`. Pins
    /// the `!targetAmount.gt(0)` guard.
    function testWithdrawZeroReverts() external {
        sToken = new MockToken("Token", "TKN", 18);
        address alice = makeAddr("alice");
        vm.prank(alice);
        vm.expectRevert(
            abi.encodeWithSelector(IRaindexV7.ZeroWithdrawTargetAmount.selector, alice, address(sToken), VAULT)
        );
        harness.withdraw5(address(sToken), VAULT, Float.wrap(0), new TaskV3[](0));
    }

    /// A partial withdraw (target < balance) moves EXACTLY the target out, reduces
    /// the vault by the target, and the event reports `withdrawAmount == target`.
    /// Pins `decreaseVaultBalance` subtracting the withdrawn amount and the event
    /// values for the uncapped branch (complement to the capped test above).
    function testWithdrawPartialMovesExactTarget() external {
        sToken = new MockToken("Token", "TKN", 18);
        address alice = makeAddr("alice");
        Float deposit = _deposit18(alice, 10e18);
        Float target = LibDecimalFloat.fromFixedDecimalLosslessPacked(4e18, 18);

        vm.expectEmit(false, false, false, true);
        emit WithdrawV2(alice, address(sToken), VAULT, target, target, 4e18);
        vm.prank(alice);
        harness.withdraw5(address(sToken), VAULT, target, new TaskV3[](0));

        assertEq(sToken.balanceOf(alice), 4e18, "alice received exact target");
        assertEq(sToken.balanceOf(address(harness)), 6e18, "harness retains remainder");
        assertTrue(
            harness.vaultBalance2(alice, address(sToken), VAULT).eq(deposit.sub(target)), "vault reduced by target"
        );
    }

    /// A deposit whose Float amount is FINER than the token's base unit is a lossy
    /// float->fixed conversion, and `pullTokens` rounds the truncated amount UP by
    /// one base unit so the contract never pulls in less than the vault accounting
    /// credits (the solvency-favoring rounding). 1.5 base units at 6 decimals
    /// (`packLossless(15, -7)`) pulls 2 base units, not 1: exactly 2 real tokens
    /// move into the harness and the `DepositV2` event reports the rounded-up 2. A
    /// mutation dropping the `++amount18` round-up (truncating down) would pull and
    /// report only 1, leaving the vault credit under-collateralised.
    function testDepositLossyPullRoundsUp() external {
        sToken = new MockToken("USDC", "USDC", 6);
        address alice = makeAddr("alice");
        sToken.mint(alice, 10);
        vm.prank(alice);
        sToken.approve(address(harness), 10);

        // 1.5 base units at 6 decimals: lossy, rounds UP to 2 base units pulled.
        Float amount = LibDecimalFloat.packLossless(15, -7);

        vm.expectEmit(false, false, false, true);
        emit DepositV2(alice, address(sToken), VAULT, 2);
        vm.prank(alice);
        harness.deposit5(address(sToken), VAULT, amount, new TaskV3[](0));

        // Exactly 2 base units moved (rounded UP from the 1.5 base-unit target),
        // never the truncated 1.
        assertEq(sToken.balanceOf(address(harness)), 2, "harness pulled rounded-up 2 base units");
        assertEq(sToken.balanceOf(alice), 8, "alice debited exactly 2 base units");
        // The vault is credited the full (un-rounded) Float amount.
        assertTrue(harness.vaultBalance2(alice, address(sToken), VAULT).eq(amount), "vault credited the deposit Float");
    }

    /// A withdraw whose Float target is FINER than the token's base unit is a lossy
    /// float->fixed conversion, and `pushTokens` TRUNCATES (rounds DOWN) so the
    /// contract never sends more than the vault accounting debits. With an internal
    /// balance larger than the target, a target of 1.5 base units at 6 decimals
    /// (`packLossless(15, -7)`) pushes 1 base unit, not 2: exactly 1 real token
    /// moves out and the `WithdrawV2` event reports the truncated-down 1 onchain
    /// amount against the (un-truncated) `withdrawAmount` Float. A mutation adding a
    /// round-up to the push would over-send 2 and break solvency.
    function testWithdrawLossyPushRoundsDown() external {
        sToken = new MockToken("USDC", "USDC", 6);
        address alice = makeAddr("alice");
        // Seed a whole-token internal balance so the target is uncapped.
        sToken.mint(alice, 1_000_000); // 1 whole token at 6 decimals.
        vm.prank(alice);
        sToken.approve(address(harness), 1_000_000);
        Float seed = LibDecimalFloat.packLossless(1, 0);
        vm.prank(alice);
        harness.deposit5(address(sToken), VAULT, seed, new TaskV3[](0));

        // 1.5 base units at 6 decimals: lossy, truncates DOWN to 1 base unit pushed.
        Float target = LibDecimalFloat.packLossless(15, -7);

        // withdrawAmount == target (target < balance), onchain amount truncates to 1.
        vm.expectEmit(false, false, false, true);
        emit WithdrawV2(alice, address(sToken), VAULT, target, target, 1);
        vm.prank(alice);
        harness.withdraw5(address(sToken), VAULT, target, new TaskV3[](0));

        // Exactly 1 base unit returned (truncated DOWN from 1.5), never the
        // rounded-up 2. Alice held 0 after the seed deposit, so now holds 1.
        assertEq(sToken.balanceOf(alice), 1, "alice received truncated-down 1 base unit");
        assertEq(sToken.balanceOf(address(harness)), 1_000_000 - 1, "harness sent exactly 1 base unit");
        // The vault is debited the full (un-truncated) Float target.
        assertTrue(
            harness.vaultBalance2(alice, address(sToken), VAULT).eq(seed.sub(target)), "vault debited the target Float"
        );
    }

    /// Withdrawing from an empty vault is a noop: `withdrawAmount` caps to zero, no
    /// tokens move, the vault stays at zero, and the event reports a zero
    /// withdrawAmount/onchain amount against the requested target. Pins the cap at
    /// the (zero) balance and the zero-skip transfer for the withdraw path.
    function testWithdrawEmptyVaultNoop() external {
        sToken = new MockToken("Token", "TKN", 18);
        address alice = makeAddr("alice");
        Float target = LibDecimalFloat.fromFixedDecimalLosslessPacked(3e18, 18);

        // No transfer occurs for an empty-vault withdraw.
        vm.expectCall(address(sToken), abi.encodeWithSelector(IERC20.transfer.selector, alice, uint256(0)), 0);
        vm.expectEmit(false, false, false, true);
        emit WithdrawV2(alice, address(sToken), VAULT, target, Float.wrap(0), 0);
        vm.prank(alice);
        harness.withdraw5(address(sToken), VAULT, target, new TaskV3[](0));

        assertEq(sToken.balanceOf(alice), 0, "alice received nothing");
        assertTrue(harness.vaultBalance2(alice, address(sToken), VAULT).isZero(), "vault stays empty");
    }

    /// The effective vault-0 balance is the MINIMUM of the owner's wallet balance
    /// and their approval to Raindex (the most Raindex could ever pull), plus the
    /// vault-0 internal slot which is zero at rest. When the APPROVAL is the
    /// smaller of the two, the approval is returned, not the balance. Exercised
    /// against a REAL ERC20 (real balanceOf/allowance) so a mutation returning the
    /// balance, or `max(...)`, is caught. (The mock-based vaultBalanceVault0 suite
    /// asserts the same selection but runs against the committed pointer bytecode,
    /// so it cannot catch a source mutation here.)
    function testVaultBalanceVault0ApprovalIsSmaller() external {
        sToken = new MockToken("Token", "TKN", 18);
        address owner = makeAddr("owner");
        sToken.mint(owner, 10e18);
        vm.prank(owner);
        sToken.approve(address(harness), 4e18);

        // min(balance 10e18, approval 4e18) == 4e18 (the approval).
        assertTrue(
            harness.vaultBalance2(owner, address(sToken), bytes32(0))
                .eq(LibDecimalFloat.fromFixedDecimalLosslessPacked(4e18, 18)),
            "vault-0 balance is the smaller approval"
        );
    }

    /// When the wallet BALANCE is the smaller of the two, the balance is returned,
    /// not the approval. Complements the approval-smaller case so a `min`->`max`
    /// or operand-swap mutation is killed from both directions.
    function testVaultBalanceVault0BalanceIsSmaller() external {
        sToken = new MockToken("Token", "TKN", 18);
        address owner = makeAddr("owner");
        sToken.mint(owner, 3e18);
        vm.prank(owner);
        sToken.approve(address(harness), 9e18);

        // min(balance 3e18, approval 9e18) == 3e18 (the balance).
        assertTrue(
            harness.vaultBalance2(owner, address(sToken), bytes32(0))
                .eq(LibDecimalFloat.fromFixedDecimalLosslessPacked(3e18, 18)),
            "vault-0 balance is the smaller wallet balance"
        );
    }

    /// A vault-0 balance read MUST revert `TokenDecimalsReadFailure(Inconsistent)`
    /// when the token's decimals have changed from the previously stored TOFU
    /// value. Without the guard, the stale stored decimals would silently misscale
    /// the balance/approval. Pins the `tofuOutcome != Consistent && != Initial`
    /// revert for the Inconsistent outcome on the read-only vault-0 path. (The
    /// mock-based vaultBalanceVault0 suite asserts this but runs against the
    /// committed pointer bytecode, so it cannot catch a source mutation here.)
    function testVaultBalanceVault0TofuInconsistentReverts() external {
        sToken = new MockToken("Token", "TKN", 18);
        address owner = makeAddr("owner");

        // Seed the TOFU store with 18 decimals via a deposit (mutating TOFU read).
        sToken.mint(owner, 1);
        vm.prank(owner);
        sToken.approve(address(harness), 1);
        vm.prank(owner);
        harness.deposit5(address(sToken), VAULT, LibDecimalFloat.packLossless(1, -18), new TaskV3[](0));

        // The token now reports DIFFERENT decimals than were stored.
        vm.mockCall(address(sToken), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(uint8(6)));

        vm.expectRevert(
            abi.encodeWithSelector(
                ITOFUTokenDecimals.TokenDecimalsReadFailure.selector, address(sToken), TOFUOutcome.Inconsistent
            )
        );
        harness.vaultBalance2(owner, address(sToken), bytes32(0));
    }

    /// Positive control: a vault-0 read with decimals CONSISTENT with the stored
    /// TOFU value does NOT revert and returns the effective balance. Confirms the
    /// guard admits the Consistent outcome (a mutation that reverts on Consistent
    /// is caught).
    function testVaultBalanceVault0TofuConsistentOk() external {
        sToken = new MockToken("Token", "TKN", 18);
        address owner = makeAddr("owner");

        // Seed the TOFU store with 18 decimals.
        sToken.mint(owner, 5e18);
        vm.prank(owner);
        sToken.approve(address(harness), 5e18);
        vm.prank(owner);
        harness.deposit5(address(sToken), VAULT, LibDecimalFloat.packLossless(1, -18), new TaskV3[](0));

        // Owner still holds 5e18 - 1 after the seed deposit; approval reduced by 1.
        // A subsequent consistent read returns min(balance, approval) without revert.
        Float balance = harness.vaultBalance2(owner, address(sToken), bytes32(0));
        assertTrue(
            balance.eq(LibDecimalFloat.fromFixedDecimalLosslessPacked(5e18 - 1, 18)),
            "consistent read returns effective balance"
        );
    }

    /// A deposit MUST revert `TokenDecimalsReadFailure(Inconsistent)` when the
    /// token's decimals have changed since they were first stored: `pullTokens`
    /// reverts on any TOFU outcome other than Consistent/Initial. Pins the
    /// pull-side TOFU guard (the mutating `decimalsForToken` path), distinct from
    /// the read-only `_vaultBalance` guard above.
    function testDepositPullTofuInconsistentReverts() external {
        sToken = new MockToken("Token", "TKN", 18);
        address owner = makeAddr("owner");
        sToken.mint(owner, 10e18);
        vm.prank(owner);
        sToken.approve(address(harness), 10e18);

        // First deposit seeds the TOFU store with 18 decimals.
        vm.prank(owner);
        harness.deposit5(address(sToken), VAULT, LibDecimalFloat.packLossless(1, 0), new TaskV3[](0));

        // The token now reports DIFFERENT decimals than were stored.
        vm.mockCall(address(sToken), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(uint8(6)));

        vm.expectRevert(
            abi.encodeWithSelector(
                ITOFUTokenDecimals.TokenDecimalsReadFailure.selector, address(sToken), TOFUOutcome.Inconsistent
            )
        );
        vm.prank(owner);
        harness.deposit5(address(sToken), VAULT, LibDecimalFloat.packLossless(1, 0), new TaskV3[](0));
    }

    /// A withdraw MUST revert `TokenDecimalsReadFailure(Inconsistent)` when the
    /// token's decimals have changed since they were first stored: `pushTokens`
    /// reverts on any TOFU outcome other than Consistent/Initial. Pins the
    /// push-side TOFU guard.
    function testWithdrawPushTofuInconsistentReverts() external {
        sToken = new MockToken("Token", "TKN", 18);
        address owner = makeAddr("owner");
        sToken.mint(owner, 10e18);
        vm.prank(owner);
        sToken.approve(address(harness), 10e18);

        // Seed the TOFU store and a withdrawable internal balance.
        vm.prank(owner);
        harness.deposit5(address(sToken), VAULT, LibDecimalFloat.packLossless(5, 0), new TaskV3[](0));

        // The token now reports DIFFERENT decimals than were stored.
        vm.mockCall(address(sToken), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(uint8(6)));

        vm.expectRevert(
            abi.encodeWithSelector(
                ITOFUTokenDecimals.TokenDecimalsReadFailure.selector, address(sToken), TOFUOutcome.Inconsistent
            )
        );
        vm.prank(owner);
        harness.withdraw5(address(sToken), VAULT, LibDecimalFloat.packLossless(1, 0), new TaskV3[](0));
    }
}
