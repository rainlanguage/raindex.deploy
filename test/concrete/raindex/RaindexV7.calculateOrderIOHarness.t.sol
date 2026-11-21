// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {OrderIOCalculationV4} from "src/struct/OrderIOCalculationV4.sol";
import {UnsupportedCalculateOutputs} from "src/error/ErrRaindexV7.sol";
import {
    CONTEXT_CALLING_CONTEXT_COLUMN,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_COUNTERPARTY,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER,
    CONTEXT_CALLING_CONTEXT_ROWS,
    CONTEXT_CALCULATIONS_COLUMN,
    CONTEXT_CALCULATIONS_ROW_MAX_OUTPUT,
    CONTEXT_CALCULATIONS_ROW_IO_RATIO,
    CONTEXT_VAULT_INPUTS_COLUMN,
    CONTEXT_VAULT_OUTPUTS_COLUMN,
    CONTEXT_VAULT_IO_TOKEN,
    CONTEXT_VAULT_IO_TOKEN_DECIMALS,
    CONTEXT_VAULT_IO_VAULT_ID,
    CONTEXT_VAULT_IO_BALANCE_BEFORE,
    CONTEXT_VAULT_IO_BALANCE_DIFF
} from "src/lib/LibRaindex.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {
    IRaindexV7,
    OrderV4,
    OrderConfigV4,
    QuoteV3,
    IOV2,
    EvaluableV4,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IInterpreterV4, EvalV4, StackItem} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {ITOFUTokenDecimals, TOFUOutcome} from "rain-tofu-erc20-decimals-0.1.5/src/interface/ITOFUTokenDecimals.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRainDeploy} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {LibOrder} from "src/lib/LibOrder.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";
import {REVERTING_MOCK_BYTECODE} from "test/util/lib/LibTestConstants.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";
import {FixedStackInterpreter} from "test/util/concrete/FixedStackInterpreter.sol";
import {IRaindexV7CalculateOrderIOHarness} from "test/util/interface/IRaindexV7CalculateOrderIOHarness.sol";

/// @title RaindexV7CalculateOrderIOHarnessTest
/// @notice Mutation-hardening for `calculateOrderIO` against a FRESH-COMPILED
/// RaindexV7 (etched runtime code), which — unlike the committed-pointer etched
/// suites — observes `src/*.sol` mutations.
contract RaindexV7CalculateOrderIOHarnessTest is Test {
    using LibDecimalFloat for Float;
    using LibOrder for OrderV4;

    IRaindexV7CalculateOrderIOHarness internal harness;
    FixedStackInterpreter internal interpreter;
    IInterpreterStoreV3 internal store;

    address internal owner = makeAddr("owner");
    address internal counterparty = makeAddr("counterparty");

    // Distinct input/output tokens (self-trade is checked elsewhere; here the
    // tokens must differ so the calculate path runs).
    address internal inputToken = address(uint160(uint256(keccak256("input.token"))));
    address internal outputToken = address(uint160(uint256(keccak256("output.token"))));

    uint8 internal constant INPUT_DECIMALS = 6;
    uint8 internal constant OUTPUT_DECIMALS = 12;

    bytes32 internal constant INPUT_VAULT_ID = bytes32(uint256(0x1111));
    bytes32 internal constant OUTPUT_VAULT_ID = bytes32(uint256(0x2222));

    function setUp() external {
        // TOFU decimals reads hit the deployed Zoltu TOFU contract.
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        // Etch the runtime code so the large RaindexV7-derived harness isn't
        // subject to the EIP-170 creation size limit.
        harness = IRaindexV7CalculateOrderIOHarness(
            payable(address(uint160(uint256(keccak256("calculate.order.io.harness")))))
        );
        vm.etch(
            address(harness),
            vm.getDeployedCode("RaindexV7CalculateOrderIOHarness.sol:RaindexV7CalculateOrderIOHarness")
        );

        interpreter = new FixedStackInterpreter();
        store = IInterpreterStoreV3(address(uint160(uint256(keccak256("store.calc.harness")))));
        vm.etch(address(store), REVERTING_MOCK_BYTECODE);

        // The two tokens report fixed (distinct) decimals.
        vm.etch(inputToken, REVERTING_MOCK_BYTECODE);
        vm.mockCall(inputToken, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(INPUT_DECIMALS));
        vm.etch(outputToken, REVERTING_MOCK_BYTECODE);
        vm.mockCall(outputToken, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(OUTPUT_DECIMALS));
    }

    /// Build a single-input single-output order pointing at the fixed-stack
    /// interpreter. The evaluable carries the canonical two-source bytecode
    /// (calculate + handle IO) so `addOrder5`'s source-count guard accepts it;
    /// `calculateOrderIO`/`quote3` drive the fixed-stack interpreter regardless
    /// of the bytecode, so the sources only matter to the add path.
    function _order() internal view returns (OrderV4 memory order) {
        IOV2[] memory inputs = new IOV2[](1);
        inputs[0] = IOV2(inputToken, INPUT_VAULT_ID);
        IOV2[] memory outputs = new IOV2[](1);
        outputs[0] = IOV2(outputToken, OUTPUT_VAULT_ID);
        order = OrderV4({
            owner: owner,
            evaluable: EvaluableV4({
                interpreter: IInterpreterV4(address(interpreter)),
                store: store,
                bytecode: LibTestAddOrder.VALID_ORDER_BYTECODE
            }),
            validInputs: inputs,
            validOutputs: outputs,
            nonce: bytes32(uint256(0xDEAD))
        });
    }

    /// Set the interpreter's calculate-order stack. The RAW stack layout is
    /// (IORatio, outputMax): `calculateOrderIO` reads stack[0] as IORatio and
    /// stack[1] as outputMax. (This is the REVERSE of the calculations CONTEXT
    /// column, which stores [outputMax, IORatio].)
    function _setStack(Float ioRatio, Float outputMax) internal {
        bytes32[] memory stack = new bytes32[](2);
        stack[0] = Float.unwrap(ioRatio);
        stack[1] = Float.unwrap(outputMax);
        interpreter.setStack(stack);
    }

    function _setStackRaw(bytes32[] memory stack) internal {
        interpreter.setStack(stack);
    }

    /// Outside a clear the order column is [orderHash, owner, counterparty].
    /// Pins each row against a mutation that swaps a row, reuses a single value,
    /// or builds the wrong column.
    function testCallingContextColumn() external {
        OrderV4 memory order = _order();
        // Give the output vault a large balance so it doesn't cap the output max.
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(1000, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(5, 0));

        OrderIOCalculationV4 memory calc =
            harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));

        // `LibContext.build` prepends a base column (sender, this) at index 0,
        // so the FINAL context matrix is indexed directly by the column constant
        // (NOT `- 1`, which addresses the pre-build calling-context array).
        bytes32[] memory callingContext = calc.context[CONTEXT_CALLING_CONTEXT_COLUMN];
        assertEq(callingContext[CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH], order.hash(), "calling context order hash");
        assertEq(
            callingContext[CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER],
            bytes32(uint256(uint160(owner))),
            "calling context order owner"
        );
        assertEq(
            callingContext[CONTEXT_CALLING_CONTEXT_ROW_ORDER_COUNTERPARTY],
            bytes32(uint256(uint160(counterparty))),
            "calling context counterparty"
        );
        assertEq(callingContext.length, CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER, "no clearer row outside a clear");
    }

    /// A clear appends the clearer (`msg.sender`) to the order column.
    function testCallingContextColumnClearing() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(1000, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(5, 0));

        OrderIOCalculationV4 memory calc =
            harness.exposedCalculateOrderIOClearing(order, 0, 0, counterparty, new SignedContextV2[](0));

        bytes32[] memory callingContext = calc.context[CONTEXT_CALLING_CONTEXT_COLUMN];
        assertEq(callingContext.length, CONTEXT_CALLING_CONTEXT_ROWS, "clearer row in a clear");
        assertEq(callingContext[CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH], order.hash(), "order hash");
        assertEq(
            callingContext[CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER], bytes32(uint256(uint160(owner))), "order owner"
        );
        assertEq(
            callingContext[CONTEXT_CALLING_CONTEXT_ROW_ORDER_COUNTERPARTY],
            bytes32(uint256(uint160(counterparty))),
            "counterparty"
        );
        assertEq(
            callingContext[CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER],
            bytes32(uint256(uint160(address(this)))),
            "clearer"
        );
    }

    /// The vault INPUT column (column 2) is
    /// [inputToken, inputDecimals, inputVaultId, inputBalanceBefore, 0].
    /// Pins every row: token address, the TOFU-read decimals, the vault id, the
    /// pre-cleared balance, and the zero balance-diff placeholder.
    function testVaultInputColumn() external {
        OrderV4 memory order = _order();
        // Seed a non-zero INPUT vault balance so balance-before is observable.
        harness.exposedIncrease(owner, inputToken, INPUT_VAULT_ID, LibDecimalFloat.packLossless(7, 0));
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(1000, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(5, 0));

        OrderIOCalculationV4 memory calc =
            harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));

        bytes32[] memory inputCol = calc.context[CONTEXT_VAULT_INPUTS_COLUMN];
        assertEq(inputCol[CONTEXT_VAULT_IO_TOKEN], bytes32(uint256(uint160(inputToken))), "input token");
        assertEq(inputCol[CONTEXT_VAULT_IO_TOKEN_DECIMALS], bytes32(uint256(INPUT_DECIMALS)), "input decimals");
        assertEq(inputCol[CONTEXT_VAULT_IO_VAULT_ID], INPUT_VAULT_ID, "input vault id");
        assertTrue(
            Float.wrap(inputCol[CONTEXT_VAULT_IO_BALANCE_BEFORE]).eq(LibDecimalFloat.packLossless(7, 0)),
            "input balance before"
        );
        assertEq(inputCol[CONTEXT_VAULT_IO_BALANCE_DIFF], bytes32(0), "input balance diff placeholder is zero");
    }

    /// The vault OUTPUT column (column 3) is
    /// [outputToken, outputDecimals, outputVaultId, outputBalanceBefore, 0].
    /// Pins every row, including that the OUTPUT decimals (12) are read for the
    /// OUTPUT token — distinct from the input decimals (6) — so a mutation that
    /// reads the wrong token's decimals into this column is caught.
    function testVaultOutputColumn() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(13, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(50, 0));

        OrderIOCalculationV4 memory calc =
            harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));

        bytes32[] memory outputCol = calc.context[CONTEXT_VAULT_OUTPUTS_COLUMN];
        assertEq(outputCol[CONTEXT_VAULT_IO_TOKEN], bytes32(uint256(uint160(outputToken))), "output token");
        assertEq(outputCol[CONTEXT_VAULT_IO_TOKEN_DECIMALS], bytes32(uint256(OUTPUT_DECIMALS)), "output decimals");
        assertEq(outputCol[CONTEXT_VAULT_IO_VAULT_ID], OUTPUT_VAULT_ID, "output vault id");
        assertTrue(
            Float.wrap(outputCol[CONTEXT_VAULT_IO_BALANCE_BEFORE]).eq(LibDecimalFloat.packLossless(13, 0)),
            "output balance before"
        );
        assertEq(outputCol[CONTEXT_VAULT_IO_BALANCE_DIFF], bytes32(0), "output balance diff placeholder is zero");
    }

    /// The output max is read back from stack[1] and the IO ratio from stack[0].
    /// With a generous output vault balance the cap is inert, so the returned
    /// outputMax/IORatio equal the raw stack values. Pins the read-back indices:
    /// a mutation swapping the two slots returns (ratio, max) reversed.
    function testReadBackOutputMaxAndIORatio() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(1000, 0));
        Float ioRatio = LibDecimalFloat.packLossless(3, 0);
        Float outputMax = LibDecimalFloat.packLossless(7, 0);
        _setStack(ioRatio, outputMax);

        OrderIOCalculationV4 memory calc =
            harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));

        assertTrue(calc.IORatio.eq(ioRatio), "IORatio is stack[0]");
        assertTrue(calc.outputMax.eq(outputMax), "outputMax is stack[1] (uncapped)");
    }

    /// The output max is CAPPED at the output vault balance: when the calculated
    /// output max (50) exceeds the vault balance (9), the returned outputMax is
    /// the balance (9), NOT the calculated value. Pins `min(outputMax, balance)`:
    /// a mutation dropping the cap, or using `max`, returns 50.
    function testOutputMaxCappedAtVaultBalance() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(50, 0));

        OrderIOCalculationV4 memory calc =
            harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));

        assertTrue(calc.outputMax.eq(LibDecimalFloat.packLossless(9, 0)), "output max capped to vault balance 9");
    }

    /// When the calculated output max (4) is BELOW the vault balance (9), the cap
    /// is inert and the calculated value passes through. Pins the cap direction:
    /// a mutation swapping `min` for `max` would return 9 here.
    function testOutputMaxBelowVaultBalanceUncapped() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(4, 0));

        OrderIOCalculationV4 memory calc =
            harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));

        assertTrue(calc.outputMax.eq(LibDecimalFloat.packLossless(4, 0)), "output max below balance is uncapped");
    }

    /// A zero output vault balance caps the output max to zero regardless of the
    /// calculated value. Pins that the cap reads the OUTPUT vault balance
    /// specifically (a fresh non-zero INPUT balance is present but must not be
    /// the cap source).
    function testOutputMaxCappedToZeroOnEmptyVault() external {
        OrderV4 memory order = _order();
        // INPUT vault has a balance; OUTPUT vault is empty.
        harness.exposedIncrease(owner, inputToken, INPUT_VAULT_ID, LibDecimalFloat.packLossless(100, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(50, 0));

        OrderIOCalculationV4 memory calc =
            harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));

        assertTrue(calc.outputMax.isZero(), "output max capped to empty OUTPUT vault balance");
    }

    /// The calculations column (column 1) is pre-filled with the CAPPED output
    /// max at row 0 and the IO ratio at row 1. Pins the row ordering and that the
    /// VAULT-CAPPED (not raw) output max lands there: output vault balance 9,
    /// calculated max 50, so the calculations column carries 9 (capped), not 50.
    function testCalculationsColumnPrefillCappedMaxThenRatio() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        Float ioRatio = LibDecimalFloat.packLossless(2, 0);
        _setStack(ioRatio, LibDecimalFloat.packLossless(50, 0));

        OrderIOCalculationV4 memory calc =
            harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));

        bytes32[] memory calculations = calc.context[CONTEXT_CALCULATIONS_COLUMN];
        assertEq(calculations.length, 2, "calculations column has two rows");
        assertTrue(
            Float.wrap(calculations[CONTEXT_CALCULATIONS_ROW_MAX_OUTPUT]).eq(LibDecimalFloat.packLossless(9, 0)),
            "calculations row 0 is the vault-capped output max"
        );
        assertTrue(
            Float.wrap(calculations[CONTEXT_CALCULATIONS_ROW_IO_RATIO]).eq(ioRatio),
            "calculations row 1 is the IO ratio"
        );
    }

    /// A calculate stack with FEWER than CALCULATE_ORDER_MIN_OUTPUTS (2) entries
    /// reverts `UnsupportedCalculateOutputs(length)` reporting the actual length.
    /// Pins both the `< 2` boundary and the reported argument.
    function testUnsupportedCalculateOutputsOnShortStack() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        // A single-element stack (length 1 < 2).
        bytes32[] memory stack = new bytes32[](1);
        stack[0] = Float.unwrap(LibDecimalFloat.packLossless(2, 0));
        _setStackRaw(stack);

        vm.expectRevert(abi.encodeWithSelector(UnsupportedCalculateOutputs.selector, uint256(1)));
        harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));
    }

    /// The exact boundary: a stack of EXACTLY 2 entries is accepted (not
    /// reverted). Pins the `<` in `length < 2`: a mutation to `<=` would revert
    /// here. A length-2 stack is the minimum legal calculate output.
    function testExactlyMinOutputsAccepted() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(4, 0));

        OrderIOCalculationV4 memory calc =
            harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));
        assertTrue(calc.outputMax.eq(LibDecimalFloat.packLossless(4, 0)), "length-2 stack accepted");
    }

    /// An empty calculate stack (length 0) reverts
    /// `UnsupportedCalculateOutputs(0)`. Pins the reported length on the
    /// zero-length boundary.
    function testUnsupportedCalculateOutputsOnEmptyStack() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStackRaw(new bytes32[](0));

        vm.expectRevert(abi.encodeWithSelector(UnsupportedCalculateOutputs.selector, uint256(0)));
        harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));
    }

    /// The INPUT token's TOFU decimals check reverts `TokenDecimalsReadFailure`
    /// with `Inconsistent` when the input token's decimals change after a value
    /// was stored. Pins that the INPUT column performs the safe TOFU check (and
    /// reverts on Inconsistent), reporting the INPUT token + Inconsistent.
    function testInputTofuInconsistentReverts() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(4, 0));

        // Seed the input token's decimals (6), then change them.
        harness.seedTofu(inputToken);
        vm.mockCall(inputToken, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(uint8(9)));

        vm.expectRevert(
            abi.encodeWithSelector(
                ITOFUTokenDecimals.TokenDecimalsReadFailure.selector, inputToken, TOFUOutcome.Inconsistent
            )
        );
        harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));
    }

    /// The INPUT token's TOFU check reverts `TokenDecimalsReadFailure` with
    /// `ReadFailure` when the input token can no longer be read at all after a
    /// value was stored. Pins the ReadFailure branch on the INPUT column.
    function testInputTofuReadFailureReverts() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(4, 0));

        harness.seedTofu(inputToken);
        // Input token decimals now revert (cleared mock over reverting bytecode).
        vm.mockCallRevert(
            inputToken, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode("read failure")
        );

        vm.expectRevert(
            abi.encodeWithSelector(
                ITOFUTokenDecimals.TokenDecimalsReadFailure.selector, inputToken, TOFUOutcome.ReadFailure
            )
        );
        harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));
    }

    /// The OUTPUT token's TOFU decimals check reverts `TokenDecimalsReadFailure`
    /// with `Inconsistent` when the output token's decimals change after a value
    /// was stored. The INPUT token stays consistent, so the revert proves the
    /// OUTPUT column runs its own safe TOFU check and reports the OUTPUT token.
    function testOutputTofuInconsistentReverts() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(4, 0));

        // Seed BOTH tokens so the input read is Consistent (passes) and only the
        // output read becomes Inconsistent.
        harness.seedTofu(inputToken);
        harness.seedTofu(outputToken);
        vm.mockCall(outputToken, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(uint8(5)));

        vm.expectRevert(
            abi.encodeWithSelector(
                ITOFUTokenDecimals.TokenDecimalsReadFailure.selector, outputToken, TOFUOutcome.Inconsistent
            )
        );
        harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));
    }

    /// The OUTPUT token's TOFU check reverts `TokenDecimalsReadFailure` with
    /// `ReadFailure` when the output token can no longer be read. The input read
    /// stays Consistent. Pins the ReadFailure branch on the OUTPUT column.
    function testOutputTofuReadFailureReverts() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(4, 0));

        harness.seedTofu(inputToken);
        harness.seedTofu(outputToken);
        vm.mockCallRevert(
            outputToken, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode("read failure")
        );

        vm.expectRevert(
            abi.encodeWithSelector(
                ITOFUTokenDecimals.TokenDecimalsReadFailure.selector, outputToken, TOFUOutcome.ReadFailure
            )
        );
        harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));
    }

    /// Positive control: with both tokens' decimals CONSISTENT (a stored value
    /// matching the live read), and `Initial` reads also accepted, calculate
    /// completes without reverting. Distinguishes the TOFU guards (which only
    /// reject Inconsistent/ReadFailure) from a mutation that rejects all outcomes
    /// or rejects Consistent/Initial.
    function testTofuConsistentAndInitialAccepted() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStack(LibDecimalFloat.packLossless(2, 0), LibDecimalFloat.packLossless(4, 0));

        // Seed only the INPUT token (Consistent on read). The OUTPUT token has no
        // stored value so its read is Initial. Both must be accepted.
        harness.seedTofu(inputToken);

        OrderIOCalculationV4 memory calc =
            harness.exposedCalculateOrderIO(order, 0, 0, counterparty, new SignedContextV2[](0));
        assertTrue(calc.outputMax.eq(LibDecimalFloat.packLossless(4, 0)), "consistent+initial accepted");
    }

    // ---------------------------------------------------------------------
    // quote3 wrapper behaviors against the fresh-compiled RaindexV7.
    // ---------------------------------------------------------------------

    /// Add the harness's standard order so `sOrders[hash] == ORDER_LIVE`.
    /// `addOrder5` does not call the interpreter, so the fixed-stack interpreter
    /// is fine. Pranks the owner so the recorded owner matches.
    function _addOrder(OrderV4 memory order) internal {
        OrderConfigV4 memory config = OrderConfigV4({
            evaluable: order.evaluable,
            validInputs: order.validInputs,
            validOutputs: order.validOutputs,
            nonce: order.nonce,
            secret: bytes32(0),
            meta: ""
        });
        vm.prank(order.owner);
        IRaindexV7(address(harness)).addOrder5(config, new TaskV3[](0));
    }

    /// A LIVE order quotes `(true, vault-capped outputMax, IORatio)`. The output
    /// vault holds 9; the calculated max (50) is capped to 9; the ratio (3)
    /// passes through. Pins the `quote3` happy path: the `true` exists flag and
    /// the (outputMax, IORatio) tuple matching `calculateOrderIO`'s capped result.
    function testQuoteLiveOrderReturnsCappedMaxAndRatio() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStack(LibDecimalFloat.packLossless(3, 0), LibDecimalFloat.packLossless(50, 0));
        _addOrder(order);

        QuoteV3 memory quoteConfig =
            QuoteV3({order: order, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        (bool exists, Float outputMax, Float ioRatio) = IRaindexV7(address(harness)).quote3(quoteConfig);

        assertTrue(exists, "live order exists");
        assertTrue(outputMax.eq(LibDecimalFloat.packLossless(9, 0)), "quote output max vault-capped to 9");
        assertTrue(ioRatio.eq(LibDecimalFloat.packLossless(3, 0)), "quote io ratio is 3");
    }

    /// A NON-LIVE order (never added) quotes `(false, 0, 0)` WITHOUT running the
    /// calculate path at all. The interpreter is loaded with a non-zero stack so
    /// a mutation that ran calculate anyway would return a non-zero tuple; the
    /// guard short-circuits to all-zero. Pins the `sOrders[hash] != ORDER_LIVE`
    /// dead-order early return and its exact `(false, 0, 0)` payload.
    function testQuoteDeadOrderReturnsFalseZeroZero() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStack(LibDecimalFloat.packLossless(3, 0), LibDecimalFloat.packLossless(50, 0));
        // Deliberately do NOT add the order.

        QuoteV3 memory quoteConfig =
            QuoteV3({order: order, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        (bool exists, Float outputMax, Float ioRatio) = IRaindexV7(address(harness)).quote3(quoteConfig);

        assertFalse(exists, "dead order does not exist");
        assertTrue(outputMax.isZero(), "dead order output max zero");
        assertTrue(ioRatio.isZero(), "dead order io ratio zero");
    }

    /// A removed (was-live, now dead) order also quotes `(false, 0, 0)`. Pins
    /// that the liveness check is `== ORDER_LIVE` and not merely "ever existed":
    /// after `removeOrder4` flips the slot back to ORDER_DEAD the quote returns
    /// the dead payload even though a non-zero stack is loaded.
    function testQuoteRemovedOrderReturnsFalseZeroZero() external {
        OrderV4 memory order = _order();
        harness.exposedIncrease(owner, outputToken, OUTPUT_VAULT_ID, LibDecimalFloat.packLossless(9, 0));
        _setStack(LibDecimalFloat.packLossless(3, 0), LibDecimalFloat.packLossless(50, 0));
        _addOrder(order);
        vm.prank(order.owner);
        IRaindexV7(address(harness)).removeOrder4(order, new TaskV3[](0));

        QuoteV3 memory quoteConfig =
            QuoteV3({order: order, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        (bool exists, Float outputMax, Float ioRatio) = IRaindexV7(address(harness)).quote3(quoteConfig);

        assertFalse(exists, "removed order does not exist");
        assertTrue(outputMax.isZero(), "removed order output max zero");
        assertTrue(ioRatio.isZero(), "removed order io ratio zero");
    }

    /// `quote3` runs `checkTokenSelfTrade` AFTER the liveness gate: a LIVE order
    /// whose input and output tokens are the same reverts `TokenSelfTrade`. Pins
    /// that the self-trade guard fires on a live order (the dead path would have
    /// returned false instead of reverting). A single same-token IO is used at
    /// (inputIOIndex 0, outputIOIndex 0).
    function testQuoteLiveSelfTradeReverts() external {
        // Build a live order whose single input and output share a token.
        IOV2[] memory inputs = new IOV2[](1);
        inputs[0] = IOV2(outputToken, INPUT_VAULT_ID);
        IOV2[] memory outputs = new IOV2[](1);
        outputs[0] = IOV2(outputToken, OUTPUT_VAULT_ID);
        OrderV4 memory order = OrderV4({
            owner: owner,
            evaluable: EvaluableV4({
                interpreter: IInterpreterV4(address(interpreter)),
                store: store,
                bytecode: LibTestAddOrder.VALID_ORDER_BYTECODE
            }),
            validInputs: inputs,
            validOutputs: outputs,
            nonce: bytes32(uint256(0xBEEF))
        });
        _setStack(LibDecimalFloat.packLossless(3, 0), LibDecimalFloat.packLossless(50, 0));
        _addOrder(order);

        QuoteV3 memory quoteConfig =
            QuoteV3({order: order, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)});
        vm.expectRevert(abi.encodeWithSelector(IRaindexV7.TokenSelfTrade.selector));
        IRaindexV7(address(harness)).quote3(quoteConfig);
    }
}
