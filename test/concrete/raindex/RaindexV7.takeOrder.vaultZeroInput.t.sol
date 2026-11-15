// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {Reenteroor} from "test/util/concrete/Reenteroor.sol";
import {
    IRaindexV7,
    OrderV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    ClearConfigV2,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {NegativeBounty} from "../../../src/error/ErrRaindexV7.sol";

/// @title RaindexV7TakeOrderVaultZeroInputTest
/// @notice An order with a `vaultId == 0` INPUT is settled as a direct wallet
/// transfer to the owner (`increaseVaultBalance` -> `pushTokens`) rather than an
/// internal vault credit. The orderbook must pull every incoming token before it
/// pushes that input out: `takeOrders5` defers vault-0 input pushes until after
/// the taker payment is pulled, and `clear4` pulls both orders' outputs before
/// pushing either input. These tests use real `MockToken` ERC20s with zero
/// ambient orderbook balance, so a vault-0 input is fully funded by the
/// counterparty alone.
contract RaindexV7TakeOrderVaultZeroInputTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    address internal owner = address(uint160(uint256(keccak256("owner.rain.test"))));
    address internal bob = address(uint160(uint256(keccak256("bob.rain.test"))));
    address internal alice = address(uint160(uint256(keccak256("alice.rain.test"))));

    bytes32 internal constant OUTPUT_VAULT_ID = bytes32(uint256(0x01));

    MockToken internal token0;
    MockToken internal token1;

    function setUp() public {
        token0 = new MockToken("Token Zero", "TK0", 18);
        token1 = new MockToken("Token One", "TK1", 18);
    }

    /// Mint `amount18` of `token` to `who`, approve the orderbook, and deposit it
    /// into `vaultId` as a Float `amount` (whole token units, 18 decimals).
    function _deposit(address who, MockToken token, bytes32 vaultId, uint256 amount) internal {
        token.mint(who, amount * 1e18);
        vm.startPrank(who);
        token.approve(address(iRaindex), amount * 1e18);
        // amount is a whole-token test quantity, far below int256 max.
        // forge-lint: disable-next-line(unsafe-typecast)
        iRaindex.deposit5(address(token), vaultId, LibDecimalFloat.packLossless(int256(amount), 0), new TaskV3[](0));
        vm.stopPrank();
    }

    /// `takeOrders5`: an order whose INPUT is `token0` at vault 0 fills against a
    /// taker who holds `token0`, while the orderbook holds no ambient `token0`.
    /// The vault-0 input is pushed to the owner only after the taker's `token0`
    /// payment has been pulled, so the trade is funded by the taker alone.
    function testVaultZeroInputSucceedsWithoutAmbientBalance() external {
        // Owner: input token0 @ vault 0 (wallet), output token1 @ a normal vault.
        _deposit(owner, token1, OUTPUT_VAULT_ID, 10);
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 1 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );

        // Taker holds token0 to pay; the orderbook holds none.
        token0.mint(bob, 1e18);
        vm.prank(bob);
        token0.approve(address(iRaindex), 1e18);
        assertEq(token0.balanceOf(address(iRaindex)), 0, "orderbook starts with zero ambient token0");

        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        vm.prank(bob);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        assertTrue(totalTakerInput.eq(LibDecimalFloat.packLossless(1, 0)), "1 token0 filled");
        assertTrue(totalTakerOutput.eq(LibDecimalFloat.packLossless(1, 0)), "1 token1 filled");

        // The vault-0 input was pushed straight to the owner's wallet, funded by
        // the taker's payment that was pulled first.
        assertEq(token0.balanceOf(owner), 1e18, "owner received vault-0 input directly");
        assertEq(token0.balanceOf(bob), 0, "taker paid token0");
        assertEq(token1.balanceOf(bob), 1e18, "taker received token1");
        assertEq(token0.balanceOf(address(iRaindex)), 0, "token0 passed straight through the orderbook");
        assertEq(token1.balanceOf(address(iRaindex)), 9e18, "unfilled token1 stays in the owner's output vault");
    }

    /// `clear4`: Alice's INPUT is `token0` at vault 0, funded by Bob's `token0`
    /// vault-0 OUTPUT. Both orders' outputs are pulled before either input is
    /// pushed, so Alice's vault-0 input is funded by Bob's output with no ambient
    /// balance needed.
    ///
    ///   Alice: input = token0 (vault 0, wallet),  output = token1 (vault 0x01)
    ///   Bob:   input = token1 (vault 0x01),        output = token0 (vault 0, wallet)
    function testClearVaultZeroInputSucceedsWithoutAmbientBalance() external {
        // Alice funds her token1 output vault.
        _deposit(alice, token1, OUTPUT_VAULT_ID, 10);

        // Bob's output is vault-0 token0 (pulled from his wallet on clear).
        token0.mint(bob, 1e18);
        vm.prank(bob);
        token0.approve(address(iRaindex), 1e18);

        OrderV4 memory aliceOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, alice, "_ _: 1 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );
        OrderV4 memory bobOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, bob, "_ _: 1 1;:;", address(token1), OUTPUT_VAULT_ID, address(token0), bytes32(0)
        );

        assertEq(token0.balanceOf(address(iRaindex)), 0, "orderbook starts with zero ambient token0");

        iRaindex.clear4(
            aliceOrder, bobOrder, ClearConfigV2(0, 0, 0, 0, 0, 0), new SignedContextV2[](0), new SignedContextV2[](0)
        );

        // Alice's vault-0 input was pushed to her wallet, funded by Bob's vault-0
        // output pulled first.
        assertEq(token0.balanceOf(alice), 1e18, "alice received vault-0 input directly");
        assertEq(token0.balanceOf(bob), 0, "bob paid his vault-0 output");
        assertEq(token0.balanceOf(address(iRaindex)), 0, "token0 passed straight through the orderbook");

        // Bob's token1 input was credited to his internal vault; Alice's token1
        // output vault was debited by the same amount.
        assertTrue(
            iRaindex.vaultBalance2(bob, address(token1), OUTPUT_VAULT_ID).eq(LibDecimalFloat.packLossless(1, 0)),
            "bob credited token1 input vault"
        );
        assertTrue(
            iRaindex.vaultBalance2(alice, address(token1), OUTPUT_VAULT_ID).eq(LibDecimalFloat.packLossless(9, 0)),
            "alice token1 output vault debited"
        );
    }

    function _take(OrderV4 memory a, OrderV4 memory b) internal returns (Float, Float) {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](2);
        orders[0] =
            TakeOrderConfigV5({order: a, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        orders[1] =
            TakeOrderConfigV5({order: b, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        vm.prank(bob);
        return iRaindex.takeOrders5(LibTestTakeOrder.defaultTakeConfig(orders));
    }

    /// Two same-owner vault-0 input orders in one batch. Their input credits
    /// accrue in the loop and settle as a single netted push to the owner after
    /// the taker's aggregate payment is pulled. With zero ambient `token0` all 3
    /// units flow taker -> orderbook -> owner; if the push were inline (before the
    /// pull) the first order would revert.
    function testVaultZeroInputMultipleSameOwnerNettedWithoutAmbientBalance() external {
        _deposit(owner, token1, OUTPUT_VAULT_ID, 10);
        OrderV4 memory order1 = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 1 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );
        // Distinct output max keeps this a distinct order from order1.
        OrderV4 memory order2 = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 2 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );

        token0.mint(bob, 3e18);
        vm.prank(bob);
        token0.approve(address(iRaindex), 3e18);
        assertEq(token0.balanceOf(address(iRaindex)), 0, "orderbook starts with zero ambient token0");

        (Float totalTakerInput, Float totalTakerOutput) = _take(order1, order2);

        assertTrue(totalTakerInput.eq(LibDecimalFloat.packLossless(3, 0)), "3 token0 filled across both orders");
        assertTrue(totalTakerOutput.eq(LibDecimalFloat.packLossless(3, 0)), "3 token1 filled across both orders");

        assertEq(token0.balanceOf(owner), 3e18, "owner received both vault-0 inputs (1 + 2), netted");
        assertEq(token0.balanceOf(bob), 0, "taker paid token0 for both");
        assertEq(token1.balanceOf(bob), 3e18, "taker received token1 for both");
        assertEq(token0.balanceOf(address(iRaindex)), 0, "token0 passed straight through the orderbook");
        assertEq(token1.balanceOf(address(iRaindex)), 7e18, "remaining token1 stays in the owner's output vault");
    }

    /// Two different-owner vault-0 input orders in one batch. Each owner's input
    /// is pushed to them after the loop (per owner, not netted across owners),
    /// all funded by the taker's single aggregate payment. Zero ambient `token0`.
    function testVaultZeroInputMultipleOwnersWithoutAmbientBalance() external {
        _deposit(owner, token1, OUTPUT_VAULT_ID, 10);
        _deposit(alice, token1, OUTPUT_VAULT_ID, 10);
        OrderV4 memory ownerOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 1 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );
        OrderV4 memory aliceOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, alice, "_ _: 1 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );

        token0.mint(bob, 2e18);
        vm.prank(bob);
        token0.approve(address(iRaindex), 2e18);
        assertEq(token0.balanceOf(address(iRaindex)), 0, "orderbook starts with zero ambient token0");

        _take(ownerOrder, aliceOrder);

        assertEq(token0.balanceOf(owner), 1e18, "owner received their own vault-0 input");
        assertEq(token0.balanceOf(alice), 1e18, "alice received their own vault-0 input");
        assertEq(token0.balanceOf(bob), 0, "taker paid token0 for both");
        assertEq(token1.balanceOf(bob), 2e18, "taker received token1 for both");
        assertEq(token0.balanceOf(address(iRaindex)), 0, "token0 passed straight through the orderbook");
    }

    /// Pool safety across REPEATED takes: a vault-0 input is a pure pass-through
    /// funded by the taker, so the orderbook's holdings of the input token must
    /// not change across the batch — and must never dip into OTHER users' pooled
    /// balances of that token. A third party parks 50 token0 in a normal vault;
    /// the owner then fills a vault-0 input order twice. Each take the owner must
    /// receive exactly one unit and the pool must stay at exactly 50.
    ///
    /// This is the strongest guard on the transient vault-0 slot. If take 1 left
    /// the slot non-zero (a dangling credit), take 2 would re-push it: the owner
    /// would receive 2 on the second take and the pool would drop to 49. If the
    /// push ever exceeded the accrued amount, the pool would drop on take 1.
    /// Either way the orderbook would be paying out tokens it does not own —
    /// draining the third party — and this test catches it to the wei.
    function testVaultZeroInputPoolUntouchedAcrossRepeatedTakes() external {
        address carol = address(uint160(uint256(keccak256("carol.rain.test"))));
        bytes32 poolVaultId = bytes32(uint256(0x99));
        // Carol's token0 sits in the orderbook as a pooled vault balance.
        _deposit(carol, token0, poolVaultId, 50);

        _deposit(owner, token1, OUTPUT_VAULT_ID, 10);
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 1 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );

        // Taker funds exactly two units across two takes.
        token0.mint(bob, 2e18);
        vm.prank(bob);
        token0.approve(address(iRaindex), 2e18);
        assertEq(token0.balanceOf(address(iRaindex)), 50e18, "orderbook holds only carol's pool pre-take");

        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));

        vm.prank(bob);
        iRaindex.takeOrders5(config);
        assertEq(token0.balanceOf(owner), 1e18, "take 1: owner receives exactly one unit");
        assertEq(token0.balanceOf(address(iRaindex)), 50e18, "take 1: pool intact");

        vm.prank(bob);
        iRaindex.takeOrders5(config);
        assertEq(token0.balanceOf(owner), 2e18, "take 2: owner receives exactly one more unit, no dangling re-push");
        assertEq(token0.balanceOf(bob), 0, "taker funded exactly two units total");
        assertEq(token0.balanceOf(address(iRaindex)), 50e18, "take 2: pool still intact, no drain from a dangling slot");
        assertTrue(
            iRaindex.vaultBalance2(carol, address(token0), poolVaultId).eq(LibDecimalFloat.packLossless(50, 0)),
            "carol's pooled vault balance untouched across both takes"
        );
    }

    /// Same pool safety for `clear4`'s vault-0 input push: Alice's vault-0 token0
    /// input is funded entirely by Bob's token0 output, so a third party's pooled
    /// token0 is untouched. A vault-0 push that exceeded what Bob supplied would
    /// drain the pool.
    function testClearVaultZeroInputDoesNotTouchPooledBalance() external {
        address carol = address(uint160(uint256(keccak256("carol.rain.test"))));
        bytes32 poolVaultId = bytes32(uint256(0x99));
        _deposit(carol, token0, poolVaultId, 50);

        _deposit(alice, token1, OUTPUT_VAULT_ID, 10);
        token0.mint(bob, 1e18);
        vm.prank(bob);
        token0.approve(address(iRaindex), 1e18);

        OrderV4 memory aliceOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, alice, "_ _: 1 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );
        OrderV4 memory bobOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, bob, "_ _: 1 1;:;", address(token1), OUTPUT_VAULT_ID, address(token0), bytes32(0)
        );

        assertEq(token0.balanceOf(address(iRaindex)), 50e18, "orderbook holds only carol's pool pre-clear");

        iRaindex.clear4(
            aliceOrder, bobOrder, ClearConfigV2(0, 0, 0, 0, 0, 0), new SignedContextV2[](0), new SignedContextV2[](0)
        );

        assertEq(token0.balanceOf(alice), 1e18, "alice received her vault-0 input");
        assertEq(token0.balanceOf(bob), 0, "bob funded his vault-0 output in full");
        assertEq(token0.balanceOf(address(iRaindex)), 50e18, "clear vault-0 push must not touch the pooled balance");
        assertTrue(
            iRaindex.vaultBalance2(carol, address(token0), poolVaultId).eq(LibDecimalFloat.packLossless(50, 0)),
            "carol's pooled vault balance is untouched"
        );
    }

    /// The OUTPUT side of vault 0: a `vaultId == 0` OUTPUT is funded directly from
    /// the order owner's wallet (`recordVaultOutput` -> `decreaseVaultBalance`
    /// pulls the token in) and handed to the taker, so a third party's pooled
    /// balance of the output token is untouched. Owner outputs token1 from vault
    /// 0; the taker supplies token0 and receives token1; carol's pooled token1 is
    /// unchanged and the token0 input is credited to the owner's normal vault.
    function testVaultZeroOutputDoesNotTouchPooledBalance() external {
        address carol = address(uint160(uint256(keccak256("carol.rain.test"))));
        bytes32 poolVaultId = bytes32(uint256(0x99));
        bytes32 inputVaultId = bytes32(uint256(0x02));
        _deposit(carol, token1, poolVaultId, 50);

        // Owner funds the vault-0 output from their wallet.
        token1.mint(owner, 1e18);
        vm.prank(owner);
        token1.approve(address(iRaindex), 1e18);
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 1 1;:;", address(token0), inputVaultId, address(token1), bytes32(0)
        );

        // Taker supplies token0 (the order's input), receives token1 (the output).
        token0.mint(bob, 1e18);
        vm.prank(bob);
        token0.approve(address(iRaindex), 1e18);
        assertEq(token1.balanceOf(address(iRaindex)), 50e18, "orderbook holds only carol's token1 pool pre-take");

        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        vm.prank(bob);
        iRaindex.takeOrders5(config);

        assertEq(token1.balanceOf(owner), 0, "owner's wallet funded the vault-0 output");
        assertEq(token1.balanceOf(bob), 1e18, "taker received the vault-0 output");
        assertEq(token0.balanceOf(bob), 0, "taker supplied token0");
        assertEq(token1.balanceOf(address(iRaindex)), 50e18, "vault-0 output must not touch the token1 pool");
        assertTrue(
            iRaindex.vaultBalance2(carol, address(token1), poolVaultId).eq(LibDecimalFloat.packLossless(50, 0)),
            "carol's pooled token1 untouched"
        );
        assertTrue(
            iRaindex.vaultBalance2(owner, address(token0), inputVaultId).eq(LibDecimalFloat.packLossless(1, 0)),
            "owner's token0 input vault credited"
        );
    }

    /// A capped (partial) fill accrues only the ACTUAL filled vault-0 input, not
    /// the order's offered max. The order offers up to 5 but the owner's output
    /// vault holds only 2 token1, so the output (and thus the matching token0
    /// input) is capped at 2. The owner must receive exactly 2 token0 — funded by
    /// the taker — and the pooled token0 must be untouched. Accruing the offered
    /// max instead of the filled amount would push 5 and drain the pool by 3.
    function testVaultZeroInputPartialFillAccruesActualNotMax() external {
        address carol = address(uint160(uint256(keccak256("carol.rain.test"))));
        bytes32 poolVaultId = bytes32(uint256(0x99));
        _deposit(carol, token0, poolVaultId, 50);

        // Output vault holds only 2, capping the order's offered-5 output at 2.
        _deposit(owner, token1, OUTPUT_VAULT_ID, 2);
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 5 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );

        // Taker could fund 5 but only 2 will fill.
        token0.mint(bob, 5e18);
        vm.prank(bob);
        token0.approve(address(iRaindex), 5e18);
        assertEq(token0.balanceOf(address(iRaindex)), 50e18, "orderbook holds only carol's pool pre-take");

        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        vm.prank(bob);
        iRaindex.takeOrders5(config);

        assertEq(token0.balanceOf(owner), 2e18, "owner received exactly the 2 filled, not the offered 5");
        assertEq(token0.balanceOf(bob), 3e18, "taker spent only the 2 that filled, keeps 3");
        assertEq(token1.balanceOf(bob), 2e18, "taker received the 2 available token1");
        assertEq(token0.balanceOf(address(iRaindex)), 50e18, "partial vault-0 fill must not touch the pool");
        assertTrue(
            iRaindex.vaultBalance2(carol, address(token0), poolVaultId).eq(LibDecimalFloat.packLossless(50, 0)),
            "carol's pooled token0 untouched"
        );
    }

    /// Settlement isolation: one owner fills two same-token (token0) input orders
    /// in one batch — A via vault 0 (pushed to the wallet) and B via a normal
    /// vault (credited internally). The two must settle to DISTINCT places: A's
    /// unit lands in the owner's wallet and B's unit in the internal vault, with
    /// neither leaking into the other and a third party's pooled token0 untouched.
    /// Over-pushing the vault-0 slot would put 2 in the wallet instead of 1.
    function testVaultZeroAndInternalInputSettleToDistinctPlaces() external {
        address carol = address(uint160(uint256(keccak256("carol.rain.test"))));
        bytes32 poolVaultId = bytes32(uint256(0x99));
        bytes32 internalVaultId = bytes32(uint256(0x02));
        _deposit(carol, token0, poolVaultId, 50);

        _deposit(owner, token1, OUTPUT_VAULT_ID, 10);
        OrderV4 memory vaultZeroOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 1 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );
        OrderV4 memory internalOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 1 1;:;", address(token0), internalVaultId, address(token1), OUTPUT_VAULT_ID
        );

        token0.mint(bob, 2e18);
        vm.prank(bob);
        token0.approve(address(iRaindex), 2e18);

        _take(vaultZeroOrder, internalOrder);

        // A (vault 0) -> owner's wallet; B (internal) -> owner's internal vault.
        assertEq(token0.balanceOf(owner), 1e18, "only the vault-0 input reached the owner's wallet");
        assertTrue(
            iRaindex.vaultBalance2(owner, address(token0), internalVaultId).eq(LibDecimalFloat.packLossless(1, 0)),
            "the internal input was credited to the internal vault, not the wallet"
        );
        // The vault-0 slot read for the wallet must not double-count the internal credit.
        assertEq(token0.balanceOf(bob), 0, "taker funded both units");
        assertEq(token1.balanceOf(bob), 2e18, "taker received both outputs");
        // Orderbook holds the pool (50) plus the internally-credited unit (1).
        assertEq(token0.balanceOf(address(iRaindex)), 51e18, "orderbook holds pool + internal credit only");
        assertTrue(
            iRaindex.vaultBalance2(carol, address(token0), poolVaultId).eq(LibDecimalFloat.packLossless(50, 0)),
            "carol's pooled token0 untouched"
        );
    }

    /// An untaken (zero-amount) order moves NO tokens at the real-token level: it
    /// runs no recordVaultInput/Output and no handleIO, so it neither charges the
    /// taker, credits the owner, nor touches any vault. Batch [untaken A, taken
    /// B]: only B's single unit flows; A contributes nothing. The overlay-removal
    /// adversarial suite asserts the state (kvs) no-op under mocked transfers;
    /// this pins the token-movement no-op with real ERC20s.
    function testUntakenOrderContributesNoTokenMovement() external {
        address carol = address(uint160(uint256(keccak256("carol.rain.test"))));
        bytes32 poolVaultId = bytes32(uint256(0x99));
        _deposit(carol, token0, poolVaultId, 50);

        _deposit(owner, token1, OUTPUT_VAULT_ID, 10);
        OrderV4 memory untaken = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 0 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );
        OrderV4 memory taken = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 1 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );

        // Taker is funded for two units; only one must be spent.
        token0.mint(bob, 2e18);
        vm.prank(bob);
        token0.approve(address(iRaindex), 2e18);

        (Float totalTakerInput,) = _take(untaken, taken);

        assertTrue(totalTakerInput.eq(LibDecimalFloat.packLossless(1, 0)), "only the taken order filled");
        assertEq(token0.balanceOf(bob), 1e18, "taker charged for the taken order only");
        assertEq(token1.balanceOf(bob), 1e18, "taker received the taken order's output only");
        assertEq(token0.balanceOf(owner), 1e18, "owner received only the taken order's vault-0 input");
        assertEq(token0.balanceOf(address(iRaindex)), 50e18, "untaken order moved nothing; pool intact");
        assertTrue(
            iRaindex.vaultBalance2(owner, address(token1), OUTPUT_VAULT_ID).eq(LibDecimalFloat.packLossless(9, 0)),
            "only the taken order drew from the output vault"
        );
    }

    /// The flash-swap reentrancy window: takeOrders5 pushes the order outputs to
    /// the taker and invokes its `onTakeOrders2` callback BEFORE it pulls the
    /// taker's payment and BEFORE it flushes the vault-0 input slots. So during
    /// the callback the vault-0 slot is accrued but not yet settled — the most
    /// dangerous moment to re-enter. A malicious taker that re-enters takeOrders5
    /// from its callback must be stopped by the nonReentrant guard, so it can
    /// never act on the mid-flight slot. (The existing arb reentrancy test uses a
    /// mock orderbook; this exercises the real RaindexV7 guard with a live vault-0
    /// slot.)
    function testTakeOrdersReentrancyViaCallbackBlocked() external {
        _deposit(owner, token1, OUTPUT_VAULT_ID, 10);
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 1 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );

        Reenteroor taker = new Reenteroor();

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        orders[0] = TakeOrderConfigV5({
            order: order, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)
        });
        // Non-empty data makes takeOrders5 invoke the onTakeOrders2 callback.
        TakeOrdersConfigV6 memory config = TakeOrdersConfigV6({
            orders: orders,
            minimumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
            maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
            IOIsInput: true,
            data: hex"01"
        });

        // The taker's callback re-enters takeOrders5 (its config is irrelevant:
        // the nonReentrant guard reverts at function entry).
        taker.reenterWith(abi.encodeWithSelector(IRaindexV7.takeOrders5.selector, config));

        vm.expectRevert(abi.encodeWithSelector(bytes4(keccak256("ReentrancyGuardReentrantCall()"))));
        vm.prank(address(taker));
        iRaindex.takeOrders5(config);
    }

    /// `clear4` where BOTH orders take their input via vault 0, so the orderbook
    /// must accrue and flush two distinct vault-0 slots. Alice's token0 input and
    /// Bob's token1 input are each pushed to their wallet, funded entirely by the
    /// counterparty's output (Bob's deposited token0 funds Alice; Alice's
    /// deposited token1 funds Bob). The clear pulls both outputs before pushing
    /// either input, so no ambient balance is needed and a third party's pooled
    /// token0 is untouched.
    function testClearBothVaultZeroInputsDoesNotTouchPool() external {
        address carol = address(uint160(uint256(keccak256("carol.rain.test"))));
        bytes32 poolVaultId = bytes32(uint256(0x99));
        _deposit(carol, token0, poolVaultId, 50);

        // Each owner deposits the token they will give as output.
        _deposit(alice, token1, OUTPUT_VAULT_ID, 1);
        _deposit(bob, token0, OUTPUT_VAULT_ID, 1);

        OrderV4 memory aliceOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, alice, "_ _: 1 1;:;", address(token0), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );
        OrderV4 memory bobOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, bob, "_ _: 1 1;:;", address(token1), bytes32(0), address(token0), OUTPUT_VAULT_ID
        );

        assertEq(token0.balanceOf(address(iRaindex)), 51e18, "pool (50) + bob's deposited output (1)");

        iRaindex.clear4(
            aliceOrder, bobOrder, ClearConfigV2(0, 0, 0, 0, 0, 0), new SignedContextV2[](0), new SignedContextV2[](0)
        );

        // Both vault-0 inputs reached the respective wallets, funded by the
        // counterparty's output.
        assertEq(token0.balanceOf(alice), 1e18, "alice received her token0 vault-0 input");
        assertEq(token1.balanceOf(bob), 1e18, "bob received his token1 vault-0 input");
        // Pool untouched; orderbook drained of exactly the two cleared units.
        assertEq(token0.balanceOf(address(iRaindex)), 50e18, "carol's pooled token0 untouched");
        assertEq(token1.balanceOf(address(iRaindex)), 0, "alice's deposited token1 went to bob");
        assertTrue(
            iRaindex.vaultBalance2(carol, address(token0), poolVaultId).eq(LibDecimalFloat.packLossless(50, 0)),
            "carol's pooled vault balance untouched"
        );
        assertTrue(
            iRaindex.vaultBalance2(alice, address(token1), OUTPUT_VAULT_ID).isZero(), "alice's output vault drawn"
        );
        assertTrue(iRaindex.vaultBalance2(bob, address(token0), OUTPUT_VAULT_ID).isZero(), "bob's output vault drawn");
    }

    /// Vault-0 settlement with a NON-18-decimal input token (6 decimals, USDC-
    /// like). The accrued slot is a decimal-agnostic Float; the push converts it
    /// back to the token's own units. Conservation must hold to the wei across
    /// that round-trip — the taker funds the input, the owner receives exactly
    /// 1.0 (= 1e6 units), and a pooled balance of the 6-decimal token is
    /// untouched. Guards the Float<->units rescale specifically on the vault-0
    /// path.
    function testVaultZeroInputSixDecimalTokenConserves() external {
        MockToken token0d6 = new MockToken("Six", "TK6", 6);
        address carol = address(uint160(uint256(keccak256("carol.rain.test"))));
        bytes32 poolVaultId = bytes32(uint256(0x99));

        // Carol pools 50.0 of the 6-decimal token.
        token0d6.mint(carol, 50e6);
        vm.startPrank(carol);
        token0d6.approve(address(iRaindex), 50e6);
        iRaindex.deposit5(address(token0d6), poolVaultId, LibDecimalFloat.packLossless(50, 0), new TaskV3[](0));
        vm.stopPrank();

        _deposit(owner, token1, OUTPUT_VAULT_ID, 10);
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 1 1;:;", address(token0d6), bytes32(0), address(token1), OUTPUT_VAULT_ID
        );

        token0d6.mint(bob, 1e6);
        vm.prank(bob);
        token0d6.approve(address(iRaindex), 1e6);
        assertEq(token0d6.balanceOf(address(iRaindex)), 50e6, "orderbook holds only carol's 6-decimal pool pre-take");

        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        vm.prank(bob);
        iRaindex.takeOrders5(config);

        assertEq(token0d6.balanceOf(owner), 1e6, "owner received the 6-decimal vault-0 input (1.0 -> 1e6)");
        assertEq(token0d6.balanceOf(bob), 0, "taker funded the input in full");
        assertEq(token0d6.balanceOf(address(iRaindex)), 50e6, "6-decimal pool untouched to the wei");
        assertTrue(
            iRaindex.vaultBalance2(carol, address(token0d6), poolVaultId).eq(LibDecimalFloat.packLossless(50, 0)),
            "carol's 6-decimal pool vault untouched"
        );
    }

    /// A vault-0 OUTPUT's spendable balance is capped by the owner's ALLOWANCE,
    /// not just their wallet balance: the orderbook can only pull what's approved.
    /// The owner holds 10 token0 but approves only 3; an order offering up to 5
    /// must fill exactly 3 (the allowance), debiting the wallet by 3. If the
    /// vault-0 balance ignored the allowance it would try to pull 5 against a 3
    /// approval and revert.
    function testVaultZeroOutputCappedByAllowanceNotBalance() external {
        bytes32 inputVaultId = bytes32(uint256(0x02));
        // Owner holds 10 token0 in their wallet but approves only 3.
        token0.mint(owner, 10e18);
        vm.prank(owner);
        token0.approve(address(iRaindex), 3e18);

        // Order: output token0 @ vault 0 (wallet), input token1 @ a normal vault.
        OrderV4 memory order = LibTestTakeOrder.addOrderWithExpression(
            vm, owner, "_ _: 5 1;:;", address(token1), inputVaultId, address(token0), bytes32(0)
        );

        // Taker supplies token1 (the order's input), receives token0 (the output).
        token1.mint(bob, 5e18);
        vm.prank(bob);
        token1.approve(address(iRaindex), 5e18);

        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        vm.prank(bob);
        iRaindex.takeOrders5(config);

        // Filled exactly the approved 3, not the offered 5.
        assertEq(token0.balanceOf(bob), 3e18, "taker received exactly the approved 3 token0");
        assertEq(token0.balanceOf(owner), 7e18, "owner's wallet debited only the approved 3");
        assertEq(token1.balanceOf(bob), 2e18, "taker paid 3 token1 for the 3 filled (had 5)");
        assertTrue(
            iRaindex.vaultBalance2(owner, address(token1), inputVaultId).eq(LibDecimalFloat.packLossless(3, 0)),
            "owner's token1 input vault credited the 3 filled"
        );
    }

    /// A clear4 bounty with bountyVaultId == 0 is pushed straight to the clearer's
    /// wallet (increaseVaultBalance's vault-0 branch), not an internal vault. With
    /// a spread — alice outputs 1 token1 wanting 1 token0, bob outputs up to 2
    /// token0 wanting 0.5 token1 — the match gives bob a bounty of
    /// bobOutput(2) - aliceInput(1) = 1 token0. The clearer (this contract) sets
    /// bobBountyVaultId = 0, so that 1 token0 lands in its wallet, funded by the
    /// surplus the orderbook already holds; a third party's pooled token0 is
    /// untouched.
    function testClearVaultZeroBountyPushedToClearerWallet() external {
        address carol = address(uint160(uint256(keccak256("carol.rain.test"))));
        bytes32 poolVaultId = bytes32(uint256(0x99));
        bytes32 inVaultId = bytes32(uint256(0x02));
        _deposit(carol, token0, poolVaultId, 50);

        // alice funds her token1 output; bob funds his token0 output (needs >= 2).
        _deposit(alice, token1, OUTPUT_VAULT_ID, 10);
        _deposit(bob, token0, OUTPUT_VAULT_ID, 10);

        OrderV4 memory aliceOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, alice, "_ _: 1 1;:;", address(token0), inVaultId, address(token1), OUTPUT_VAULT_ID
        );
        OrderV4 memory bobOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, bob, "_ _: 2 0.5;:;", address(token1), inVaultId, address(token0), OUTPUT_VAULT_ID
        );

        assertEq(token0.balanceOf(address(this)), 0, "clearer starts with no token0");

        // 6th ClearConfig arg (bobBountyVaultId) = 0 -> bob's bounty -> clearer wallet.
        iRaindex.clear4(
            aliceOrder, bobOrder, ClearConfigV2(0, 0, 0, 0, 0, 0), new SignedContextV2[](0), new SignedContextV2[](0)
        );

        assertEq(token0.balanceOf(address(this)), 1e18, "clearer received the 1 token0 bounty in their wallet");
        assertEq(token0.balanceOf(address(iRaindex)), 59e18, "orderbook token0 = pool(50) + bob(10) - bounty(1)");
        assertTrue(
            iRaindex.vaultBalance2(carol, address(token0), poolVaultId).eq(LibDecimalFloat.packLossless(50, 0)),
            "carol's pooled token0 untouched"
        );
    }

    /// `_vaultBalance` for vault 0 reads the token's live decimals via TOFU and
    /// reverts on an inconsistency rather than computing a balance from changed
    /// decimals. This guard is redundant in the take/clear path (calculateOrderIO
    /// checks the token's TOFU first and reverts), so its only independent surface
    /// is the `vaultBalance2` view getter. token0's TOFU is established at 18 by a
    /// deposit; once its reported decimals change to 6, a vault-0 `vaultBalance2`
    /// read must revert TokenDecimalsReadFailure.
    function testVaultBalance2VaultZeroRevertsOnInconsistentDecimals() external {
        bytes4 decimalsSel = bytes4(keccak256("decimals()"));
        // Establish token0's TOFU at 18 decimals (the deposit's pull writes it).
        vm.mockCall(address(token0), abi.encodeWithSelector(decimalsSel), abi.encode(uint8(18)));
        _deposit(owner, token0, OUTPUT_VAULT_ID, 1);

        // The token now reports different decimals -> TOFU is inconsistent.
        vm.mockCall(address(token0), abi.encodeWithSelector(decimalsSel), abi.encode(uint8(6)));

        // Reverts (TokenDecimalsReadFailure) rather than returning a balance.
        vm.expectRevert();
        iRaindex.vaultBalance2(owner, address(token0), bytes32(0));
    }

    /// A `clear4` whose orders cross into a negative bounty reverts with the
    /// explicit `NegativeBounty` before any vault is settled. Both orders take
    /// and give via vault 0 against an orderbook with zero ambient balance, so
    /// the only funds that could settle a vault-0 input are the capped outputs
    /// pulled from the counterparty. The negative-bounty guard runs ahead of that
    /// settlement, so the explicit error is the revert rather than the vault-0
    /// token push running short first.
    ///
    ///   Alice: gives token0, wants token1 @ IO ratio 2 (both vault 0)
    ///   Bob:   gives token1, wants token0 @ IO ratio 2 (both vault 0)
    ///
    /// Each order's input demand (1 * 2 = 2) caps to the counterparty's max
    /// output (1): each output is 0.5 and each input is 1, so both bounties are
    /// 0.5 - 1 = -0.5.
    function testClearNegativeBountyVaultZeroRevertsNegativeBounty() external {
        token0.mint(alice, 10e18);
        vm.prank(alice);
        token0.approve(address(iRaindex), 10e18);

        token1.mint(bob, 10e18);
        vm.prank(bob);
        token1.approve(address(iRaindex), 10e18);

        OrderV4 memory aliceOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, alice, "_ _: 1 2;:;", address(token1), bytes32(0), address(token0), bytes32(0)
        );
        OrderV4 memory bobOrder = LibTestTakeOrder.addOrderWithExpression(
            vm, bob, "_ _: 1 2;:;", address(token0), bytes32(0), address(token1), bytes32(0)
        );

        assertEq(token0.balanceOf(address(iRaindex)), 0, "orderbook starts with zero ambient token0");
        assertEq(token1.balanceOf(address(iRaindex)), 0, "orderbook starts with zero ambient token1");

        vm.expectRevert(NegativeBounty.selector);
        iRaindex.clear4(
            aliceOrder, bobOrder, ClearConfigV2(0, 0, 0, 0, 0, 0), new SignedContextV2[](0), new SignedContextV2[](0)
        );
    }
}
