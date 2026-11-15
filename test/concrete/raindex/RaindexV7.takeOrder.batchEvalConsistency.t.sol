// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {
    OrderV4,
    OrderConfigV4,
    EvaluableV4,
    IOV2,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// @title RaindexV7TakeOrderBatchEvalConsistencyTest
/// @notice Within a single `takeOrders5` batch, each order's `calculate` should
/// see the complete effects of every earlier order in the batch — a sequential
/// view. Today that holds for some state and not others, depending purely on how
/// the effect is applied:
///
///   - internal vault balance change -> applied in-loop (SSTORE)        -> VISIBLE
///   - vault-0 input credit          -> token push deferred past loop   -> NOT visible
///   - earlier order's calculate set -> threaded into later calculate   -> VISIBLE
///   - earlier order's handle-IO set -> committed after the loop         -> NOT visible
///
/// The pairwise tests use two same-owner orders: A fills and produces an effect;
/// B reads it in `calculate` and only fills while it has NOT observed it
/// (`if(seen 0 1)`). A sequential batch fills one order (total 1 — B skips); an
/// inconsistent one fills both (total 2 — B is blind to A). The chain tests add
/// a third order: A and B both produce an effect and C reads the accumulated sum
/// as its output, so a sequential batch totals 4 and an inconsistent one totals
/// 2. The internal-vault and calculate-write cases pass today; the vault-0 input
/// and handle-IO cases fail, pinning the two seams.
///
/// Further baselines map the boundary: effects are isolated per owner, the first
/// order in a batch sees no later effect, and handle-IO-to-handle-IO IS
/// consistent (handle-IO entrypoints run in order over a direct SSTORE/SLOAD
/// store) — so the handle-IO seam is specifically a later `calculate` reading an
/// earlier order's handle-IO write, not handle-IO ordering.
contract RaindexV7TakeOrderBatchEvalConsistencyTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    address internal immutable iTaker = address(uint160(uint256(keccak256("taker.rain.test"))));

    bytes32 internal constant INTERNAL_VAULT = bytes32(uint256(0x01));
    bytes32 internal constant VAULT_ZERO = bytes32(0);

    function owner(string memory tag) internal pure returns (address) {
        return address(uint160(uint256(keccak256(bytes(tag)))));
    }

    /// Allow every settlement transfer to succeed (deposits, taker legs, and the
    /// deferred vault-0 input push to the owner).
    function mockTransfers() internal {
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
    }

    /// Vault-0 input reads the owner's live wallet balance/allowance for the
    /// input token; mock both to zero so the only thing that could make a later
    /// order see a balance is an earlier order's (currently deferred) credit.
    function mockVaultZeroReads(address o) internal {
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.balanceOf.selector, o), abi.encode(0));
        vm.mockCall(
            address(iToken0), abi.encodeWithSelector(IERC20.allowance.selector, o, address(iRaindex)), abi.encode(0)
        );
    }

    /// Fund the owner's iToken1 output vault so orders have output to give.
    function fundOutput(address o) internal {
        vm.prank(o);
        iRaindex.deposit5(address(iToken1), INTERNAL_VAULT, LibDecimalFloat.packLossless(100, 0), new TaskV3[](0));
    }

    function addOrder(address o, bytes memory expr, bytes32 inputVaultId) internal returns (OrderV4 memory) {
        return LibTestTakeOrder.addOrderWithExpression(
            vm, o, expr, address(iToken0), inputVaultId, address(iToken1), INTERNAL_VAULT
        );
    }

    /// Like `addOrder` but with an explicit nonce, so two otherwise-identical
    /// orders (same owner, expression and vaults) are distinct orders rather than
    /// a duplicate that emits no `AddOrderV3`.
    function addOrderNonce(address o, bytes memory expr, bytes32 inputVaultId, uint256 nonce)
        internal
        returns (OrderV4 memory)
    {
        IOV2[] memory inputs = new IOV2[](1);
        inputs[0] = IOV2({token: address(iToken0), vaultId: inputVaultId});
        IOV2[] memory outputs = new IOV2[](1);
        outputs[0] = IOV2({token: address(iToken1), vaultId: INTERNAL_VAULT});
        OrderConfigV4 memory config = OrderConfigV4({
            evaluable: EvaluableV4({interpreter: iInterpreter, store: iStore, bytecode: iParserV2.parse2(expr)}),
            validInputs: inputs,
            validOutputs: outputs,
            nonce: bytes32(nonce),
            secret: 0,
            meta: ""
        });
        vm.prank(o);
        iRaindex.addOrder5(config, new TaskV3[](0));
        return OrderV4(o, config.evaluable, config.validInputs, config.validOutputs, config.nonce);
    }

    function take(OrderV4 memory a, OrderV4 memory b) internal returns (Float totalTakerInput) {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](2);
        orders[0] =
            TakeOrderConfigV5({order: a, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        orders[1] =
            TakeOrderConfigV5({order: b, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        vm.prank(iTaker);
        (totalTakerInput,) = iRaindex.takeOrders5(LibTestTakeOrder.defaultTakeConfig(orders));
    }

    // A: plain fill of 1, crediting its input vault.
    bytes constant FILL = "_ _:1 1;:;";
    // B: fill 1 only while its input vault balance is still zero. `context<3 3>`
    // is the input vault balance before the trade (the `input-vault-before` word).
    bytes constant FILL_WHILE_INPUT_EMPTY = "bal:context<3 3>(),_ _:if(bal 0 1) 1;:;";

    /// Baseline (consistent): an internal-vault input credit from A is visible to
    /// B's calculate in the same batch, so B observes it and skips. Total 1.
    function testInternalInputBalanceVisibleToLaterSameOwnerOrder() external {
        mockTransfers();
        address o = owner("alice");
        fundOutput(o);
        OrderV4 memory a = addOrder(o, FILL, INTERNAL_VAULT);
        OrderV4 memory b = addOrder(o, FILL_WHILE_INPUT_EMPTY, INTERNAL_VAULT);
        assertTrue(take(a, b).eq(LibDecimalFloat.packLossless(1, 0)), "B must see A's internal input credit and skip");
    }

    /// Baseline (consistent): A's calculate `set` is threaded into B's calculate
    /// in the same batch, so B observes it and skips. Total 1.
    function testCalculateWriteVisibleToLaterSameOwnerOrder() external {
        mockTransfers();
        address o = owner("bob");
        fundOutput(o);
        OrderV4 memory a = addOrder(o, ":set(0 1),_ _:1 1;:;", INTERNAL_VAULT);
        OrderV4 memory b = addOrder(o, "seen:get(0),_ _:if(seen 0 1) 1;:;", INTERNAL_VAULT);
        assertTrue(take(a, b).eq(LibDecimalFloat.packLossless(1, 0)), "B must see A's calculate write and skip");
    }

    /// Inconsistent (FAILS today): A's vault-0 input credit is a token push
    /// deferred past the loop, so B's calculate reads a zero balance and fills.
    /// Identical to the internal-vault baseline except the input vault id, which
    /// is the whole point: the visibility must not depend on the vault id.
    function testVaultZeroInputBalanceVisibleToLaterSameOwnerOrder() external {
        mockTransfers();
        address o = owner("carol");
        mockVaultZeroReads(o);
        fundOutput(o);
        OrderV4 memory a = addOrder(o, FILL, VAULT_ZERO);
        OrderV4 memory b = addOrder(o, FILL_WHILE_INPUT_EMPTY, VAULT_ZERO);
        assertTrue(take(a, b).eq(LibDecimalFloat.packLossless(1, 0)), "B must see A's vault-0 input credit and skip");
    }

    /// Inconsistent (FAILS today): A's handle-IO `set` is committed after the
    /// loop, so B's calculate reads a zero value and fills. Identical to the
    /// calculate-write baseline except the `set` is in handle-IO rather than
    /// calculate.
    function testHandleIOWriteVisibleToLaterSameOwnerOrder() external {
        mockTransfers();
        address o = owner("dave");
        fundOutput(o);
        OrderV4 memory a = addOrder(o, "_ _:1 1;:set(0 1);", INTERNAL_VAULT);
        OrderV4 memory b = addOrder(o, "seen:get(0),_ _:if(seen 0 1) 1;:;", INTERNAL_VAULT);
        assertTrue(take(a, b).eq(LibDecimalFloat.packLossless(1, 0)), "B must see A's handle-IO write and skip");
    }

    function take3(OrderV4 memory a, OrderV4 memory b, OrderV4 memory c) internal returns (Float totalTakerInput) {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](3);
        orders[0] =
            TakeOrderConfigV5({order: a, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        orders[1] =
            TakeOrderConfigV5({order: b, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        orders[2] =
            TakeOrderConfigV5({order: c, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        vm.prank(iTaker);
        (totalTakerInput,) = iRaindex.takeOrders5(LibTestTakeOrder.defaultTakeConfig(orders));
    }

    // C: output as much as its input vault holds (reads the accumulated balance,
    // not just whether it is nonzero).
    bytes constant OUTPUT_INPUT_BALANCE = "_ _:context<3 3>() 1;:;";

    /// Baseline (consistent): internal input credits from A and B accumulate and
    /// the sum is visible to C's calculate. C outputs 2 -> total 1+1+2 = 4.
    function testInternalInputBalanceAccumulatesAcrossSameOwnerChain() external {
        mockTransfers();
        address o = owner("erin");
        fundOutput(o);
        OrderV4 memory a = addOrderNonce(o, FILL, INTERNAL_VAULT, 1);
        OrderV4 memory b = addOrderNonce(o, FILL, INTERNAL_VAULT, 2);
        OrderV4 memory c = addOrder(o, OUTPUT_INPUT_BALANCE, INTERNAL_VAULT);
        assertTrue(take3(a, b, c).eq(LibDecimalFloat.packLossless(4, 0)), "C must see the sum of A and B input credits");
    }

    /// Inconsistent (FAILS today): vault-0 input credits from A and B are token
    /// pushes deferred past the loop, so C reads a zero balance and outputs
    /// nothing -> total 1+1+0 = 2. A sequential batch gives 4.
    function testVaultZeroInputBalanceAccumulatesAcrossSameOwnerChain() external {
        mockTransfers();
        address o = owner("frank");
        mockVaultZeroReads(o);
        fundOutput(o);
        OrderV4 memory a = addOrderNonce(o, FILL, VAULT_ZERO, 1);
        OrderV4 memory b = addOrderNonce(o, FILL, VAULT_ZERO, 2);
        OrderV4 memory c = addOrder(o, OUTPUT_INPUT_BALANCE, VAULT_ZERO);
        assertTrue(
            take3(a, b, c).eq(LibDecimalFloat.packLossless(4, 0)), "C must see the sum of A and B vault-0 credits"
        );
    }

    /// Inconsistent (FAILS today): A sets a counter and B increments it, both in
    /// handle-IO; C's calculate reads it as its output. Handle-IO commits after
    /// the loop, so C reads 0 and outputs nothing -> total 2. Sequential: C reads
    /// 2 -> total 4.
    function testHandleIOWriteAccumulatesAcrossSameOwnerChain() external {
        mockTransfers();
        address o = owner("grace");
        fundOutput(o);
        OrderV4 memory a = addOrder(o, "_ _:1 1;:set(0 1);", INTERNAL_VAULT);
        OrderV4 memory b = addOrder(o, "_ _:1 1;:set(0 add(get(0) 1));", INTERNAL_VAULT);
        OrderV4 memory c = addOrder(o, "v:get(0),_ _:v 1;:;", INTERNAL_VAULT);
        assertTrue(take3(a, b, c).eq(LibDecimalFloat.packLossless(4, 0)), "C must see A and B handle-IO writes");
    }

    /// Baseline (consistent): an order's output vault decrease is applied in-loop,
    /// so a later same-owner order observes the reduced balance. A consumes 1 of
    /// the 100 output vault; B fills only while the vault is still full, observes
    /// A's decrease and skips -> total 1.
    function testOutputVaultBalanceVisibleToLaterSameOwnerOrder() external {
        mockTransfers();
        address o = owner("heidi");
        fundOutput(o);
        OrderV4 memory a = addOrder(o, FILL, INTERNAL_VAULT);
        OrderV4 memory b = addOrder(o, "full:equal-to(context<4 3>() 100),_ _:if(full 1 0) 1;:;", INTERNAL_VAULT);
        assertTrue(take(a, b).eq(LibDecimalFloat.packLossless(1, 0)), "B must see A's output vault decrease and skip");
    }

    /// Characterises the boundary of the handle-IO seam. Handle-IO entrypoints
    /// run in order and the store is a direct SSTORE/SLOAD, so a later order's
    /// handle-IO should observe an earlier order's handle-IO write. B's handle-IO
    /// reverts via `ensure` if A's write is missing; if both fill (total 2) the
    /// seam is calculate-reading-handle-IO specifically, not handle-IO to
    /// handle-IO.
    function testHandleIOWriteVisibleToLaterHandleIO() external {
        mockTransfers();
        address o = owner("ivan");
        fundOutput(o);
        OrderV4 memory a = addOrder(o, "_ _:1 1;:set(0 1);", INTERNAL_VAULT);
        OrderV4 memory b = addOrder(o, "_ _:1 1;:ensure(get(0) \"B sees A\");", INTERNAL_VAULT);
        assertTrue(take(a, b).eq(LibDecimalFloat.packLossless(2, 0)), "B handle-IO must see A handle-IO write");
    }

    /// Inconsistent (FAILS today) and pins per-owner isolation: P's order A and
    /// Q's order M each credit their own vault-0; P's order B reads P's vault-0
    /// input balance as its output. A correct sequential, owner-keyed batch lets
    /// B see exactly A's credit (1) -> total 1+1+1 = 3. Today B is blind -> 2. A
    /// fix that leaked Q's credit into P's read would give 4.
    function testVaultZeroInputCreditIsolatedPerOwner() external {
        mockTransfers();
        address p = owner("pat");
        address q = owner("quinn");
        mockVaultZeroReads(p);
        mockVaultZeroReads(q);
        fundOutput(p);
        fundOutput(q);
        OrderV4 memory a = addOrder(p, FILL, VAULT_ZERO);
        OrderV4 memory m = addOrder(q, FILL, VAULT_ZERO);
        OrderV4 memory b = addOrder(p, OUTPUT_INPUT_BALANCE, VAULT_ZERO);
        assertTrue(take3(a, m, b).eq(LibDecimalFloat.packLossless(3, 0)), "B must see only P's own vault-0 credit");
    }

    /// Baseline (consistent): calculate writes are namespaced per owner, so P's
    /// order B reads P's key (5), not Q's (99). B outputs 5 -> total 1+1+5 = 7.
    function testCalculateWriteIsolatedPerOwner() external {
        mockTransfers();
        address p = owner("rita");
        address q = owner("sam");
        fundOutput(p);
        fundOutput(q);
        OrderV4 memory a = addOrder(p, ":set(0 5),_ _:1 1;:;", INTERNAL_VAULT);
        OrderV4 memory m = addOrder(q, ":set(0 99),_ _:1 1;:;", INTERNAL_VAULT);
        OrderV4 memory b = addOrder(p, "v:get(0),_ _:v 1;:;", INTERNAL_VAULT);
        assertTrue(take3(a, m, b).eq(LibDecimalFloat.packLossless(7, 0)), "B must read only P's own calculate write");
    }

    /// Baseline (consistent): the first order in a batch sees no later order's
    /// effect. A reads its input balance and only fills while empty; it runs
    /// first, so it fills, then B fills and credits the vault -> total 2. A fix
    /// must not retroactively expose B's credit to A.
    function testFirstOrderSeesNoLaterOrderState() external {
        mockTransfers();
        address o = owner("tara");
        fundOutput(o);
        OrderV4 memory a = addOrder(o, FILL_WHILE_INPUT_EMPTY, INTERNAL_VAULT);
        OrderV4 memory b = addOrder(o, FILL, INTERNAL_VAULT);
        assertTrue(take(a, b).eq(LibDecimalFloat.packLossless(2, 0)), "first order must not see a later order's credit");
    }

    /// Inconsistent (FAILS today): a single order C depends on BOTH seams at once
    /// — A's vault-0 input credit and A's handle-IO write. C fills only if it sees
    /// both. Today it sees neither -> total 1. A sequential batch -> 2.
    function testVaultZeroAndHandleIOReadsCompound() external {
        mockTransfers();
        address o = owner("umar");
        mockVaultZeroReads(o);
        fundOutput(o);
        OrderV4 memory a = addOrder(o, "_ _:1 1;:set(0 1);", VAULT_ZERO);
        OrderV4 memory c = addOrder(o, "bal:context<3 3>(),k:get(0),_ _:if(bal if(k 1 0) 0) 1;:;", VAULT_ZERO);
        assertTrue(
            take(a, c).eq(LibDecimalFloat.packLossless(2, 0)), "C must see A's vault-0 credit and handle-IO write"
        );
    }

    /// The vault-0 input slot is a transient in-batch accumulator: it must be
    /// zeroed once settled so it never leaks into a later read or a later batch.
    /// `mockVaultZeroReads` pins the wallet at zero, so after the take the vault-0
    /// balance is exactly the slot, which must be zero.
    function testVaultZeroInputSlotZeroedAfterTake() external {
        mockTransfers();
        address o = owner("walt");
        mockVaultZeroReads(o);
        fundOutput(o);
        OrderV4 memory a = addOrderNonce(o, FILL, VAULT_ZERO, 1);
        OrderV4 memory b = addOrderNonce(o, FILL, VAULT_ZERO, 2);
        take(a, b);
        assertTrue(
            iRaindex.vaultBalance2(o, address(iToken0), VAULT_ZERO).isZero(),
            "vault-0 input slot must be zeroed after settlement"
        );
    }

    bytes32 internal constant OTHER_INTERNAL_VAULT = bytes32(uint256(0x05));

    /// The vault-0 input slot and internal `sVaultBalances` are independent
    /// accounting paths. A credits the vault-0 slot; B reads a different internal
    /// input vault and must NOT see A's credit, so it fills. Total 2. A read that
    /// folded the vault-0 slot into an internal balance would make B skip (total 1).
    function testVaultZeroCreditDoesNotLeakIntoInternalRead() external {
        mockTransfers();
        address o = owner("xena");
        mockVaultZeroReads(o);
        fundOutput(o);
        OrderV4 memory a = addOrder(o, FILL, VAULT_ZERO);
        OrderV4 memory b = addOrder(o, FILL_WHILE_INPUT_EMPTY, OTHER_INTERNAL_VAULT);
        assertTrue(
            take(a, b).eq(LibDecimalFloat.packLossless(2, 0)), "a vault-0 credit must not appear in an internal read"
        );
    }

    /// The reverse: A credits an internal input vault; B reads vault 0 and must
    /// NOT see it, so it fills. Total 2. An accrue that routed internal credits
    /// into the vault-0 slot would make B skip (total 1).
    function testInternalCreditDoesNotLeakIntoVaultZeroRead() external {
        mockTransfers();
        address o = owner("yuri");
        mockVaultZeroReads(o);
        fundOutput(o);
        OrderV4 memory a = addOrder(o, FILL, OTHER_INTERNAL_VAULT);
        OrderV4 memory b = addOrder(o, FILL_WHILE_INPUT_EMPTY, VAULT_ZERO);
        assertTrue(
            take(a, b).eq(LibDecimalFloat.packLossless(2, 0)), "an internal credit must not appear in a vault-0 read"
        );
    }
}
