// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test, Vm} from "forge-std-1.17.0/src/Test.sol";
import {NegativeBounty, ClearZeroAmount} from "src/error/ErrRaindexV7.sol";
import {ExponentUnderflow} from "rain-math-float-0.3.13/src/error/ErrDecimalFloat.sol";
import {
    OrderV4,
    OrderConfigV4,
    IOV2,
    ClearConfigV2,
    ClearStateChangeV2,
    EvaluableV4,
    SignedContextV2,
    IInterpreterV4,
    IInterpreterStoreV3,
    TaskV3
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.37/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {LibOrder} from "src/lib/LibOrder.sol";
import {CalcInterpreter} from "test/util/concrete/CalcInterpreter.sol";
import {IRaindexV7ClearDriverHarness} from "test/util/interface/IRaindexV7ClearDriverHarness.sol";
import {IParserV2} from "rainlang-0.2.40/src/interface/IParserV2.sol";
import {FullyQualifiedNamespace} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {StateNamespace} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {LibNamespace} from "rainlang-0.2.40/src/lib/ns/LibNamespace.sol";
import {
    LibTestInterpreterDeploy,
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS,
    TEST_COMPILER_ADDRESS
} from "rainlang-0.2.40/test/lib/deploy/LibTestInterpreterDeploy.sol";
import {
    CREATION_CODE as DECIMAL_FLOAT_CREATION_CODE
} from "rain-math-float-deploy-0.1.13/src/generated/0_1_5/DecimalFloat.sol";

/// @title RaindexV7ClearFreshDriverTest
/// @notice Drives the FULL `clear4` against fresh-compiled RaindexV7 bytecode so
/// source mutations to the bounty computation, the NegativeBounty guard, the
/// record-vault ordering, the bounty crediting, ClearZeroAmount and AfterClearV2
/// are actually observable (the etched mock/real suites are blind to src
/// mutations). Uses internal (non-zero) vaults so the IO legs are pure internal
/// balance changes with no token plumbing, isolating the clear4 accounting.
contract RaindexV7ClearFreshDriverTest is Test {
    using LibDecimalFloat for Float;
    using LibOrder for OrderV4;

    IRaindexV7ClearDriverHarness internal raindex;
    MockToken internal token0;
    MockToken internal token1;

    address internal alice = makeAddr("alice");
    address internal bob = makeAddr("bob");
    address internal bountyBot = makeAddr("bountyBot");

    bytes32 internal constant ALICE_OUT_VAULT = bytes32(uint256(0x0a07));
    bytes32 internal constant ALICE_IN_VAULT = bytes32(uint256(0x0a14));
    bytes32 internal constant BOB_OUT_VAULT = bytes32(uint256(0x0b07));
    bytes32 internal constant BOB_IN_VAULT = bytes32(uint256(0x0b14));
    bytes32 internal constant ALICE_BOUNTY_VAULT = bytes32(uint256(0xaab0));
    bytes32 internal constant BOB_BOUNTY_VAULT = bytes32(uint256(0xbbb0));

    // Events mirrored from IRaindexV7 for expectEmit.
    event AfterClearV2(address sender, ClearStateChangeV2 clearStateChange);
    event ClearV3(address sender, OrderV4 alice, OrderV4 bob, ClearConfigV2 clearConfig);

    function setUp() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        // Etch fresh-compiled runtime code (EIP-170-safe) for the harness.
        raindex = IRaindexV7ClearDriverHarness(payable(address(uint160(uint256(keccak256("clear.driver.harness"))))));
        vm.etch(address(raindex), vm.getDeployedCode("RaindexV7ClearDriverHarness.sol:RaindexV7ClearDriverHarness"));

        token0 = new MockToken("Token0", "TK0", 18);
        token1 = new MockToken("Token1", "TK1", 18);
    }

    function f(int256 sig, int256 exp) internal pure returns (Float) {
        return LibDecimalFloat.packLossless(sig, exp);
    }

    /// Build an order whose calculate eval returns (ratio, max). token in/out and
    /// vault ids are wired by the caller. Marks it live in the harness.
    function buildLiveOrder(
        address owner,
        address inputToken,
        bytes32 inputVault,
        address outputToken,
        bytes32 outputVault,
        Float ratio,
        Float max,
        uint256 nonce
    ) internal returns (OrderV4 memory order) {
        CalcInterpreter interpreter = new CalcInterpreter(ratio, max);
        IOV2[] memory inputs = new IOV2[](1);
        inputs[0] = IOV2(inputToken, inputVault);
        IOV2[] memory outputs = new IOV2[](1);
        outputs[0] = IOV2(outputToken, outputVault);
        EvaluableV4 memory evaluable = EvaluableV4({
            interpreter: IInterpreterV4(address(interpreter)), store: IInterpreterStoreV3(address(0)), bytecode: hex""
        });
        order = OrderV4(owner, evaluable, inputs, outputs, bytes32(nonce));
        raindex.setOrderLive(order.hash());
    }

    function clearConfig() internal pure returns (ClearConfigV2 memory) {
        return ClearConfigV2({
            aliceInputIOIndex: 0,
            aliceOutputIOIndex: 0,
            bobInputIOIndex: 0,
            bobOutputIOIndex: 0,
            aliceBountyVaultId: ALICE_BOUNTY_VAULT,
            bobBountyVaultId: BOB_BOUNTY_VAULT
        });
    }

    /// Wire two token-compatible live orders with given (ratio, max, seedAmount)
    /// for each, seeding output vaults so outputMax is not vault-capped below the
    /// order max.
    function wireOrders(Float aliceRatio, Float aliceMax, Float bobRatio, Float bobMax, Float seed)
        internal
        returns (OrderV4 memory aliceOrder, OrderV4 memory bobOrder)
    {
        // alice outputs token0, inputs token1; bob outputs token1, inputs token0.
        raindex.seedVault(alice, address(token0), ALICE_OUT_VAULT, seed);
        raindex.seedVault(bob, address(token1), BOB_OUT_VAULT, seed);
        aliceOrder = buildLiveOrder(
            alice, address(token1), ALICE_IN_VAULT, address(token0), ALICE_OUT_VAULT, aliceRatio, aliceMax, 1
        );
        bobOrder =
            buildLiveOrder(bob, address(token0), BOB_IN_VAULT, address(token1), BOB_OUT_VAULT, bobRatio, bobMax, 2);
    }

    function doClear(OrderV4 memory aliceOrder, OrderV4 memory bobOrder) internal {
        vm.prank(bountyBot);
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));
    }

    // ---------------------------------------------------------------------
    // Happy clear: bounty crediting + state change + AfterClearV2
    // ---------------------------------------------------------------------

    /// Alice: max 1, ratio 0.5 -> input 0.5. Bob: max 1, ratio 0.5 -> input 0.5.
    /// Both uncapped. aliceOutput=1, bobOutput=1, aliceInput=0.5, bobInput=0.5.
    /// aliceBounty = aliceOutput - bobInput = 1 - 0.5 = 0.5 (token0).
    /// bobBounty   = bobOutput - aliceInput = 1 - 0.5 = 0.5 (token1).
    /// Pins B7/B8 (bounty formula), B10 (bounty credited to msg.sender at the
    /// right token+vault), the order owner vault deltas, and the AfterClearV2
    /// payload, all on fresh-compiled bytecode.
    function testFreshClearBountyAndVaults() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) =
            wireOrders(f(5, -1), f(1, 0), f(5, -1), f(1, 0), f(10, 0));

        ClearStateChangeV2 memory expected =
            ClearStateChangeV2({aliceOutput: f(1, 0), bobOutput: f(1, 0), aliceInput: f(5, -1), bobInput: f(5, -1)});
        vm.expectEmit(address(raindex));
        emit AfterClearV2(bountyBot, expected);

        doClear(aliceOrder, bobOrder);

        // Order owner vault deltas: outputs decreased by output, inputs += input.
        assertTrue(raindex.vaultBalance2(alice, address(token0), ALICE_OUT_VAULT).eq(f(9, 0)), "alice out vault 10-1=9");
        assertTrue(raindex.vaultBalance2(bob, address(token1), BOB_OUT_VAULT).eq(f(9, 0)), "bob out vault 10-1=9");
        assertTrue(raindex.vaultBalance2(alice, address(token1), ALICE_IN_VAULT).eq(f(5, -1)), "alice in vault 0.5");
        assertTrue(raindex.vaultBalance2(bob, address(token0), BOB_IN_VAULT).eq(f(5, -1)), "bob in vault 0.5");

        // Bounties to msg.sender: alice bounty is alice's OUTPUT token (token0),
        // bob bounty is bob's OUTPUT token (token1).
        assertTrue(
            raindex.vaultBalance2(bountyBot, address(token0), ALICE_BOUNTY_VAULT).eq(f(5, -1)),
            "alice bounty token0 = 0.5"
        );
        assertTrue(
            raindex.vaultBalance2(bountyBot, address(token1), BOB_BOUNTY_VAULT).eq(f(5, -1)), "bob bounty token1 = 0.5"
        );
    }

    /// ASYMMETRIC bounty: alice's bounty and bob's bounty are DIFFERENT values, so
    /// a mutation that credits one order's bounty using the OTHER order's bounty
    /// amount is observable. alice: max 2, ratio 0.5 -> aliceInput=1 (uncapped),
    /// aliceOutput=2. bob: max 2, ratio 0.25 -> bobInput=0.5 (uncapped),
    /// bobOutput=2. So:
    ///   aliceBounty = aliceOutput - bobInput = 2 - 0.5 = 1.5 (token0, alice vault)
    ///   bobBounty   = bobOutput   - aliceInput = 2 - 1   = 1.0 (token1, bob vault)
    /// Pins B10 amount: alice's credited bounty is exactly 1.5 (NOT bob's 1.0) and
    /// bob's is exactly 1.0 (NOT alice's 1.5).
    function testFreshClearAsymmetricBounties() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) =
            wireOrders(f(5, -1), f(2, 0), f(25, -2), f(2, 0), f(10, 0));

        doClear(aliceOrder, bobOrder);

        // Value-based (.eq) bounty assertions: alice and bob bounties are distinct,
        // so a swap of the credited amount is caught.
        assertTrue(
            raindex.vaultBalance2(bountyBot, address(token0), ALICE_BOUNTY_VAULT).eq(f(15, -1)),
            "alice bounty token0 = 1.5 (not bob's 1.0)"
        );
        assertTrue(
            raindex.vaultBalance2(bountyBot, address(token1), BOB_BOUNTY_VAULT).eq(f(1, 0)),
            "bob bounty token1 = 1.0 (not alice's 1.5)"
        );
        // Order-owner input vaults also reflect the asymmetric inputs.
        assertTrue(raindex.vaultBalance2(alice, address(token1), ALICE_IN_VAULT).eq(f(1, 0)), "alice in = 1");
        assertTrue(raindex.vaultBalance2(bob, address(token0), BOB_IN_VAULT).eq(f(5, -1)), "bob in = 0.5");
    }

    /// Zero bounty: both ratios are exactly 1 and both maxes 1 -> output==input
    /// on each side, so both bounties are exactly 0 (no spread). The clear still
    /// succeeds (zero is not negative), owner vaults move 1<->1, and both bounty
    /// vaults stay zero. Pins that the NegativeBounty guard uses a STRICT lt (zero
    /// passes) and that a zero bounty credits nothing.
    function testFreshClearZeroBounty() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) = wireOrders(f(1, 0), f(1, 0), f(1, 0), f(1, 0), f(10, 0));

        doClear(aliceOrder, bobOrder);

        assertTrue(raindex.vaultBalance2(bountyBot, address(token0), ALICE_BOUNTY_VAULT).isZero(), "alice bounty zero");
        assertTrue(raindex.vaultBalance2(bountyBot, address(token1), BOB_BOUNTY_VAULT).isZero(), "bob bounty zero");
        assertTrue(raindex.vaultBalance2(alice, address(token1), ALICE_IN_VAULT).eq(f(1, 0)), "alice in 1");
        assertTrue(raindex.vaultBalance2(bob, address(token0), BOB_IN_VAULT).eq(f(1, 0)), "bob in 1");
    }

    // ---------------------------------------------------------------------
    // NegativeBounty guard (a ratio spread > 1)
    // ---------------------------------------------------------------------

    /// Both ratios > 1 so each side's required input exceeds the counterparty's
    /// output: aliceInput caps to bob.outputMax, but aliceOutput = input/ratio <
    /// input, making aliceBounty = aliceOutput - bobInput negative. clear4 must
    /// revert NegativeBounty (the spread is unprofitable / would drain the dex).
    function testFreshClearNegativeBountyReverts() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) =
            wireOrders(f(11, -1), f(1, 0), f(11, -1), f(1, 0), f(10, 0));

        vm.prank(bountyBot);
        vm.expectRevert(NegativeBounty.selector);
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));
    }

    /// ASYMMETRIC: ONLY alice's bounty is negative, bob's is exactly zero.
    /// alice ratio 2, max 1 -> aliceInput would be 2 > bob.max(1) -> capped to 1,
    /// aliceOutput = 1/2 = 0.5. bob ratio 0.6, max 1 -> bobInput = 0.6 (<= alice
    /// max 1, uncapped), bobOutput = 1. So:
    ///   aliceBounty = aliceOutput - bobInput = 0.5 - 0.6 = -0.1  (NEGATIVE)
    ///   bobBounty   = bobOutput   - aliceInput = 1   - 1   =  0  (NON-negative)
    /// The clear MUST still revert NegativeBounty even though bob's bounty is fine.
    /// This pins that the guard checks BOTH terms with OR: a mutation dropping the
    /// aliceBounty term, or changing OR to AND, would NOT revert here.
    function testFreshClearOnlyAliceBountyNegativeReverts() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) = wireOrders(f(2, 0), f(1, 0), f(6, -1), f(1, 0), f(10, 0));

        vm.prank(bountyBot);
        vm.expectRevert(NegativeBounty.selector);
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));
    }

    /// ASYMMETRIC mirror: ONLY bob's bounty is negative, alice's is exactly zero.
    /// alice ratio 0.6, max 1 -> aliceInput = 0.6 (uncapped), aliceOutput = 1.
    /// bob ratio 2, max 1 -> bobInput capped to 1, bobOutput = 1/2 = 0.5. So:
    ///   aliceBounty = aliceOutput - bobInput = 1   - 1   =  0  (NON-negative)
    ///   bobBounty   = bobOutput   - aliceInput = 0.5 - 0.6 = -0.1 (NEGATIVE)
    /// The clear MUST revert NegativeBounty. Pins the bobBounty term of the OR
    /// guard: a mutation dropping the bobBounty term, or OR->AND, would NOT revert.
    function testFreshClearOnlyBobBountyNegativeReverts() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) = wireOrders(f(6, -1), f(1, 0), f(2, 0), f(1, 0), f(10, 0));

        vm.prank(bountyBot);
        vm.expectRevert(NegativeBounty.selector);
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));
    }

    /// The NegativeBounty guard must run BEFORE any bounty vault is credited and
    /// before the order-owner vaults are committed in a way that survives. The
    /// revert rolls back ALL state, so after a reverting clear the order owner
    /// output vaults are untouched (still the full seed) and bounty vaults stay
    /// zero. Pins that no partial settlement leaks on the negative-bounty path.
    function testFreshClearNegativeBountyNoStateLeak() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) =
            wireOrders(f(11, -1), f(1, 0), f(11, -1), f(1, 0), f(10, 0));

        vm.prank(bountyBot);
        try raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0)) {
            revert("clear should have reverted");
        } catch {}

        assertTrue(raindex.vaultBalance2(alice, address(token0), ALICE_OUT_VAULT).eq(f(10, 0)), "alice out untouched");
        assertTrue(raindex.vaultBalance2(bob, address(token1), BOB_OUT_VAULT).eq(f(10, 0)), "bob out untouched");
        assertTrue(
            raindex.vaultBalance2(bountyBot, address(token0), ALICE_BOUNTY_VAULT).isZero(), "alice bounty stays 0"
        );
        assertTrue(raindex.vaultBalance2(bountyBot, address(token1), BOB_BOUNTY_VAULT).isZero(), "bob bounty stays 0");
    }

    // ---------------------------------------------------------------------
    // ClearZeroAmount
    // ---------------------------------------------------------------------

    function expectClearZeroAmount(Float aliceRatio, Float aliceMax, Float bobRatio, Float bobMax) internal {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) =
            wireOrders(aliceRatio, aliceMax, bobRatio, bobMax, f(10, 0));

        vm.prank(bountyBot);
        vm.expectRevert(ClearZeroAmount.selector);
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));
    }

    function testFreshClearBothZeroOutputReverts() external {
        expectClearZeroAmount(f(1, 0), f(0, 0), f(1, 0), f(0, 0));
    }

    function testFreshClearAliceZeroOutputReverts() external {
        expectClearZeroAmount(f(1, 0), f(0, 0), f(0, 0), f(1, 0));
    }

    function testFreshClearBobZeroOutputReverts() external {
        expectClearZeroAmount(f(0, 0), f(1, 0), f(1, 0), f(0, 0));
    }

    function testFreshClearAliceNegativeOutputReverts() external {
        expectClearZeroAmount(f(1, 0), f(-1, 0), f(1, 0), f(1, 0));
    }

    function testFreshClearBobNegativeOutputReverts() external {
        expectClearZeroAmount(f(1, 0), f(1, 0), f(1, 0), f(-1, 0));
    }

    function testFreshClearAliceNegativeRatioReverts() external {
        expectClearZeroAmount(f(-1, 0), f(1, 0), f(1, 0), f(1, 0));
    }

    function testFreshClearBobNegativeRatioReverts() external {
        expectClearZeroAmount(f(1, 0), f(1, 0), f(-1, 0), f(1, 0));
    }

    /// Alice's calculate sets a store key while her output is zero against a
    /// zero-ratio Bob; the refused clear leaves her store untouched.
    function testFreshClearAliceZeroOutputLeavesStoreUnchanged() external {
        LibRainDeploy.deployZoltu(DECIMAL_FLOAT_CREATION_CODE);
        LibTestInterpreterDeploy.etchTestRainlang(vm);
        IParserV2 parser = IParserV2(TEST_COMPILER_ADDRESS);
        IInterpreterStoreV3 store = IInterpreterStoreV3(TEST_STORE_ADDRESS);

        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) = wireOrders(f(1, 0), f(1, 0), f(1, 0), f(1, 0), f(10, 0));
        aliceOrder.evaluable = EvaluableV4({
            interpreter: IInterpreterV4(TEST_INTERPRETER_ADDRESS),
            store: store,
            bytecode: parser.parse2(":set(1 2),_ _:0 1;:;")
        });
        bobOrder.evaluable = EvaluableV4({
            interpreter: IInterpreterV4(TEST_INTERPRETER_ADDRESS), store: store, bytecode: parser.parse2("_ _:1 0;:;")
        });
        raindex.setOrderLive(aliceOrder.hash());
        raindex.setOrderLive(bobOrder.hash());

        FullyQualifiedNamespace aliceNamespace =
            LibNamespace.qualifyNamespace(StateNamespace.wrap(uint256(uint160(alice))), address(raindex));

        vm.prank(bountyBot);
        vm.expectRevert(ClearZeroAmount.selector);
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));

        assertEq(store.get(aliceNamespace, bytes32(uint256(1))), bytes32(0), "alice store unchanged");
    }

    /// A zero ratio against a positive counterparty clears: the zero-ratio side
    /// gives its output for nothing.
    function testFreshClearAliceZeroRatioClears() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) = wireOrders(f(0, 0), f(1, 0), f(1, 0), f(1, 0), f(10, 0));

        doClear(aliceOrder, bobOrder);

        assertTrue(raindex.vaultBalance2(alice, address(token0), ALICE_OUT_VAULT).eq(f(9, 0)), "alice out 9");
        assertTrue(raindex.vaultBalance2(bob, address(token1), BOB_OUT_VAULT).eq(f(9, 0)), "bob out 9");
        assertTrue(raindex.vaultBalance2(alice, address(token1), ALICE_IN_VAULT).isZero(), "alice in 0");
        assertTrue(raindex.vaultBalance2(bob, address(token0), BOB_IN_VAULT).eq(f(1, 0)), "bob in 1");
        assertTrue(raindex.vaultBalance2(bountyBot, address(token1), BOB_BOUNTY_VAULT).eq(f(1, 0)), "bob bounty 1");
    }

    function testFreshClearBobZeroRatioClears() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) = wireOrders(f(1, 0), f(1, 0), f(0, 0), f(1, 0), f(10, 0));

        doClear(aliceOrder, bobOrder);

        assertTrue(raindex.vaultBalance2(alice, address(token0), ALICE_OUT_VAULT).eq(f(9, 0)), "alice out 9");
        assertTrue(raindex.vaultBalance2(bob, address(token1), BOB_OUT_VAULT).eq(f(9, 0)), "bob out 9");
        assertTrue(raindex.vaultBalance2(alice, address(token1), ALICE_IN_VAULT).eq(f(1, 0)), "alice in 1");
        assertTrue(raindex.vaultBalance2(bob, address(token0), BOB_IN_VAULT).isZero(), "bob in 0");
        assertTrue(raindex.vaultBalance2(bountyBot, address(token0), ALICE_BOUNTY_VAULT).eq(f(1, 0)), "alice bounty 1");
    }

    /// A zero ratio caps against a negative counterparty max, which would
    /// divide by that zero ratio if the sides were not refused first.
    function testFreshClearAliceZeroRatioBobNegativeOutputReverts() external {
        expectClearZeroAmount(f(0, 0), f(1, 0), f(1, 0), f(-1, 0));
    }

    function testFreshClearBobZeroRatioAliceNegativeOutputReverts() external {
        expectClearZeroAmount(f(1, 0), f(-1, 0), f(0, 0), f(1, 0));
    }

    /// Both sides pass the refusal but the capped output, the smallest max
    /// over the largest ratio, is below every non-zero Float: the division
    /// reverts rather than yielding a zero output.
    function testFreshClearAliceCappedOutputUnderflowReverts() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) =
            wireOrders(f(1, type(int32).max), f(1, 0), f(1, 0), f(1, type(int32).min), f(10, 0));

        vm.prank(bountyBot);
        vm.expectPartialRevert(ExponentUnderflow.selector);
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));
    }

    function testFreshClearBobCappedOutputUnderflowReverts() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) =
            wireOrders(f(1, 0), f(1, type(int32).min), f(1, type(int32).max), f(1, 0), f(10, 0));

        vm.prank(bountyBot);
        vm.expectPartialRevert(ExponentUnderflow.selector);
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));
    }

    // ---------------------------------------------------------------------
    // Vault-0 (token-plumbing) clear: record ordering / pushVaultZeroInput (B11)
    // ---------------------------------------------------------------------

    /// Build a live order whose INPUT and OUTPUT vaults are BOTH vault 0, so the
    /// output leg pulls real tokens from the owner into the orderbook and the
    /// input leg is settled by pushing real tokens out. The owner is funded and
    /// approves the orderbook for the pull.
    function buildVault0Order(
        address owner,
        address inputToken,
        address outputToken,
        Float ratio,
        Float max,
        uint256 nonce,
        uint256 fund
    ) internal returns (OrderV4 memory order) {
        MockToken(outputToken).mint(owner, fund);
        vm.prank(owner);
        MockToken(outputToken).approve(address(raindex), type(uint256).max);

        CalcInterpreter interpreter = new CalcInterpreter(ratio, max);
        IOV2[] memory inputs = new IOV2[](1);
        inputs[0] = IOV2(inputToken, bytes32(0));
        IOV2[] memory outputs = new IOV2[](1);
        outputs[0] = IOV2(outputToken, bytes32(0));
        EvaluableV4 memory evaluable = EvaluableV4({
            interpreter: IInterpreterV4(address(interpreter)), store: IInterpreterStoreV3(address(0)), bytecode: hex""
        });
        order = OrderV4(owner, evaluable, inputs, outputs, bytes32(nonce));
        raindex.setOrderLive(order.hash());
    }

    /// Full vault-0 clear: both orders use vault 0 for input and output, so the
    /// clear moves REAL tokens. alice outputs token0 / inputs token1; bob outputs
    /// token1 / inputs token0; both ratio 0.5, max 1. Outcome:
    ///   alice pays 1 token0, receives 0.5 token1.  bob pays 1 token1, receives
    ///   0.5 token0.  bounty 0.5 token0 + 0.5 token1 to the clearer's vaults.
    /// Because both outputs are pulled INTO the orderbook before either input is
    /// pushed OUT, the orderbook always holds enough of each token to settle; the
    /// real balances reconcile exactly. Pins B11 (record ordering) and the
    /// pushVaultZeroInput settlement against fresh source: a mutation that pushed
    /// an input before pulling the matching output would leave the orderbook short
    /// and revert.
    function testFreshClearVault0RealTokenFlow() external {
        OrderV4 memory aliceOrder =
            buildVault0Order(alice, address(token1), address(token0), f(5, -1), f(1, 0), 1, 1e18);
        OrderV4 memory bobOrder = buildVault0Order(bob, address(token0), address(token1), f(5, -1), f(1, 0), 2, 1e18);

        doClear(aliceOrder, bobOrder);

        // Alice paid 1 token0 (out), received 0.5 token1 (in).
        assertEq(token0.balanceOf(alice), 0, "alice token0 spent fully (1 out)");
        assertEq(token1.balanceOf(alice), 0.5e18, "alice received 0.5 token1");
        // Bob paid 1 token1 (out), received 0.5 token0 (in).
        assertEq(token1.balanceOf(bob), 0, "bob token1 spent fully (1 out)");
        assertEq(token0.balanceOf(bob), 0.5e18, "bob received 0.5 token0");
        // The orderbook retains the bounty (0.5 of each token) in the clearer's
        // internal bounty vaults.
        assertEq(token0.balanceOf(address(raindex)), 0.5e18, "orderbook holds alice bounty token0");
        assertEq(token1.balanceOf(address(raindex)), 0.5e18, "orderbook holds bob bounty token1");
        assertTrue(
            raindex.vaultBalance2(bountyBot, address(token0), ALICE_BOUNTY_VAULT).eq(f(5, -1)),
            "alice bounty vault 0.5 token0"
        );
        assertTrue(
            raindex.vaultBalance2(bountyBot, address(token1), BOB_BOUNTY_VAULT).eq(f(5, -1)),
            "bob bounty vault 0.5 token1"
        );
    }

    // ---------------------------------------------------------------------
    // Guards: SameOwner / TokenMismatch / TokenSelfTrade / dead orders / ClearV3
    // ---------------------------------------------------------------------

    /// Build an order struct WITHOUT registering it live (caller decides). Tokens
    /// and owner are explicit so the guard scenarios can be constructed precisely.
    function makeOrder(address owner, address inputToken, address outputToken, uint256 nonce)
        internal
        returns (OrderV4 memory order)
    {
        CalcInterpreter interpreter = new CalcInterpreter(f(1, 0), f(1, 0));
        IOV2[] memory inputs = new IOV2[](1);
        inputs[0] = IOV2(inputToken, ALICE_IN_VAULT);
        IOV2[] memory outputs = new IOV2[](1);
        outputs[0] = IOV2(outputToken, ALICE_OUT_VAULT);
        EvaluableV4 memory evaluable = EvaluableV4({
            interpreter: IInterpreterV4(address(interpreter)), store: IInterpreterStoreV3(address(0)), bytecode: hex""
        });
        order = OrderV4(owner, evaluable, inputs, outputs, bytes32(nonce));
    }

    /// SameOwner: alice.owner == bob.owner reverts SameOwner. The owner check is
    /// the FIRST guard. Pins B14.
    function testFreshClearSameOwnerReverts() external {
        OrderV4 memory aliceOrder = makeOrder(alice, address(token1), address(token0), 1);
        OrderV4 memory bobOrder = makeOrder(alice, address(token0), address(token1), 2);

        vm.prank(bountyBot);
        // SameOwner is selector-checked from src; import not needed because we
        // use the raw 4-byte selector of the error in the src file.
        vm.expectRevert(bytes4(keccak256("SameOwner()")));
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));
    }

    /// TokenMismatch (leg 1 ONLY): alice's OUTPUT token must equal bob's INPUT
    /// token. Wired so ONLY leg 1 mismatches and leg 2 is satisfied, so the revert
    /// is attributable to leg 1 alone (a mutation disabling leg 1 must NOT be
    /// rescued by leg 2 here). alice: in token1, out token0. bob: in token1,
    /// out token1. Then:
    ///   leg1: alice.out(token0) != bob.in(token1) -> MISMATCH (this is the trip)
    ///   leg2: bob.out(token1)   != alice.in(token1) -> token1==token1, OK
    /// Pins the first disjunct of B15 in isolation.
    function testFreshClearTokenMismatchAliceOutputReverts() external {
        OrderV4 memory aliceOrder = makeOrder(alice, address(token1), address(token0), 1);
        OrderV4 memory bobOrder = makeOrder(bob, address(token1), address(token1), 2);

        vm.prank(bountyBot);
        vm.expectRevert(bytes4(keccak256("TokenMismatch()")));
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));
    }

    /// TokenMismatch (leg 2): bob's OUTPUT token must equal alice's INPUT token.
    /// alice inputs token1, bob outputs token0 (mismatch) while leg 1 is fine.
    /// Pins the SECOND leg of B15 specifically (a mutation dropping the second
    /// disjunct survives leg-1-only tests).
    function testFreshClearTokenMismatchBobOutputReverts() external {
        // alice: in token1, out token0. bob: in token0 (ok for leg1), out token0
        // (leg2 wants alice's input token1, but bob outputs token0 -> mismatch).
        OrderV4 memory aliceOrder = makeOrder(alice, address(token1), address(token0), 1);
        OrderV4 memory bobOrder = makeOrder(bob, address(token0), address(token0), 2);

        vm.prank(bountyBot);
        vm.expectRevert(bytes4(keccak256("TokenMismatch()")));
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));
    }

    /// TokenSelfTrade: alice's input token == alice's output token reverts
    /// TokenSelfTrade. Wire bob so the token-MISMATCH guard passes first (bob in
    /// == alice out, bob out == alice in) but alice in == alice out. Pins B16, and
    /// that this guard is distinct from TokenMismatch.
    function testFreshClearTokenSelfTradeReverts() external {
        // alice in==out==token0. For leg1 (alice out token0 == bob in) bob in
        // token0; for leg2 (bob out == alice in token0) bob out token0.
        OrderV4 memory aliceOrder = makeOrder(alice, address(token0), address(token0), 1);
        OrderV4 memory bobOrder = makeOrder(bob, address(token0), address(token0), 2);

        vm.prank(bountyBot);
        vm.expectRevert(bytes4(keccak256("TokenSelfTrade()")));
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));
    }

    /// Dead alice order (never made live): clear4 emits OrderNotFound for alice's
    /// hash and RETURNS without reverting (best-effort bulk-clear semantics).
    /// Exactly one log, no ClearV3/AfterClearV2. Pins the first dead-order check
    /// (B17) AND that ClearV3 has not been emitted yet (B18 ordering).
    function testFreshClearAliceDeadEmitsAndReturns() external {
        OrderV4 memory aliceOrder = makeOrder(alice, address(token1), address(token0), 1);
        OrderV4 memory bobOrder = makeOrder(bob, address(token0), address(token1), 2);
        // Neither is live. Alice is checked first.

        vm.recordLogs();
        vm.prank(bountyBot);
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));

        Vm.Log[] memory logs = vm.getRecordedLogs();
        assertEq(logs.length, 1, "only OrderNotFound emitted");
        assertEq(logs[0].topics[0], keccak256("OrderNotFound(address,address,bytes32)"), "log is OrderNotFound");
        (address sender, address owner, bytes32 orderHash) = abi.decode(logs[0].data, (address, address, bytes32));
        assertEq(sender, bountyBot, "sender is clearer");
        assertEq(owner, alice, "owner is alice");
        assertEq(orderHash, aliceOrder.hash(), "hash is alice's");
    }

    /// Dead bob order while alice is live: the SECOND dead-order check fires,
    /// emitting OrderNotFound for BOB's hash/owner (not alice's) and returning.
    /// Pins that the second check exists and uses bob's identity.
    function testFreshClearBobDeadEmitsBobAndReturns() external {
        OrderV4 memory aliceOrder = makeOrder(alice, address(token1), address(token0), 1);
        OrderV4 memory bobOrder = makeOrder(bob, address(token0), address(token1), 2);
        raindex.setOrderLive(aliceOrder.hash());
        // bob stays dead.

        vm.recordLogs();
        vm.prank(bountyBot);
        raindex.clear4(aliceOrder, bobOrder, clearConfig(), new SignedContextV2[](0), new SignedContextV2[](0));

        Vm.Log[] memory logs = vm.getRecordedLogs();
        assertEq(logs.length, 1, "only OrderNotFound emitted");
        assertEq(logs[0].topics[0], keccak256("OrderNotFound(address,address,bytes32)"), "log is OrderNotFound");
        (address sender, address owner, bytes32 orderHash) = abi.decode(logs[0].data, (address, address, bytes32));
        assertEq(sender, bountyBot, "sender is clearer");
        assertEq(owner, bob, "owner is bob (second check)");
        assertEq(orderHash, bobOrder.hash(), "hash is bob's");
    }

    /// A successful clear emits ClearV3 BEFORE AfterClearV2, with the clearer as
    /// sender and the two orders + config in order (alice, then bob). Pins B18:
    /// the full event payload (sender == clearer, NOT an order owner; alice and
    /// bob in the correct argument positions) is asserted via expectEmit, and the
    /// ClearV3-before-AfterClearV2 ordering is asserted from the recorded logs.
    function testFreshClearEmitsClearV3() external {
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) =
            wireOrders(f(5, -1), f(1, 0), f(5, -1), f(1, 0), f(10, 0));

        // Full-data ClearV3 payload check: sender is the clearer (bountyBot), and
        // the orders are in (alice, bob) order with the exact clear config.
        vm.expectEmit(address(raindex));
        emit ClearV3(bountyBot, aliceOrder, bobOrder, clearConfig());

        vm.recordLogs();
        doClear(aliceOrder, bobOrder);

        Vm.Log[] memory logs = vm.getRecordedLogs();
        bytes32 clearV3Topic = keccak256(
            "ClearV3(address,(address,(address,address,bytes),(address,bytes32)[],"
            "(address,bytes32)[],bytes32),(address,(address,address,bytes),(address,bytes32)[],(address,bytes32)[],"
            "bytes32),(uint256,uint256,uint256,uint256,bytes32,bytes32))"
        );
        bytes32 afterClearTopic = keccak256("AfterClearV2(address,(bytes32,bytes32,bytes32,bytes32))");
        bool foundClearV3 = false;
        bool foundAfterClear = false;
        uint256 clearV3Index = type(uint256).max;
        uint256 afterClearIndex = type(uint256).max;
        for (uint256 i = 0; i < logs.length; i++) {
            if (logs[i].topics.length > 0 && logs[i].topics[0] == clearV3Topic) {
                foundClearV3 = true;
                clearV3Index = i;
            }
            if (logs[i].topics.length > 0 && logs[i].topics[0] == afterClearTopic) {
                foundAfterClear = true;
                afterClearIndex = i;
            }
        }
        assertTrue(foundClearV3, "ClearV3 emitted");
        assertTrue(foundAfterClear, "AfterClearV2 emitted");
        assertTrue(clearV3Index < afterClearIndex, "ClearV3 emitted before AfterClearV2");
    }
}
