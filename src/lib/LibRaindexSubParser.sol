// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.19;

import {AuthoringMetaV2} from "rainlang-0.2.40/src/interface/ISubParserV4.sol";
import {OperandV2} from "rainlang-0.2.40/src/lib/parse/LibParse.sol";
import {LibUint256Matrix} from "rain-solmem-0.1.28/src/lib/LibUint256Matrix.sol";
import {LibSubParse} from "rainlang-0.2.40/src/lib/parse/LibSubParse.sol";
import {
    CONTEXT_BASE_COLUMN,
    CONTEXT_BASE_ROW_SENDER,
    CONTEXT_BASE_ROW_CALLING_CONTRACT,
    CONTEXT_BASE_ROWS
} from "rainlang-0.2.40/src/lib/caller/LibContext.sol";
import {
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER,
    CONTEXT_DEPOSIT_COLUMN,
    CONTEXT_WITHDRAW_COLUMN,
    CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_DEPOSITOR,
    CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_WITHDRAWER,
    CONTEXT_COLUMNS_EXTENDED,
    CONTEXT_VAULT_OUTPUTS_COLUMN,
    CONTEXT_VAULT_INPUTS_COLUMN,
    CONTEXT_CALCULATIONS_COLUMN,
    CONTEXT_CALLING_CONTEXT_COLUMN,
    CONTEXT_VAULT_IO_BALANCE_DIFF,
    CONTEXT_VAULT_IO_BALANCE_BEFORE,
    CONTEXT_VAULT_IO_VAULT_ID,
    CONTEXT_VAULT_IO_TOKEN_DECIMALS,
    CONTEXT_VAULT_IO_TOKEN,
    CONTEXT_VAULT_IO_ROWS,
    CONTEXT_CALCULATIONS_ROW_IO_RATIO,
    CONTEXT_CALCULATIONS_ROW_MAX_OUTPUT,
    CONTEXT_CALCULATIONS_ROWS,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_COUNTERPARTY,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH,
    CONTEXT_CALLING_CONTEXT_ROWS,
    CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN,
    CONTEXT_SIGNED_CONTEXT_SIGNERS_ROW,
    CONTEXT_SIGNED_CONTEXT_SIGNERS_ROWS,
    CONTEXT_SIGNED_CONTEXT_DOMAINS_COLUMN,
    CONTEXT_SIGNED_CONTEXT_DOMAINS_ROW,
    CONTEXT_SIGNED_CONTEXT_DOMAINS_ROWS,
    CONTEXT_SIGNED_CONTEXT_START_COLUMN,
    CONTEXT_SIGNED_CONTEXT_START_ROW,
    CONTEXT_SIGNED_CONTEXT_START_ROWS,
    CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_TOKEN,
    CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_VAULT_ID,
    CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_VAULT_BEFORE,
    CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_VAULT_AFTER,
    CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_TOKEN,
    CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_VAULT_ID,
    CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_VAULT_BEFORE,
    CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_VAULT_AFTER,
    CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_TARGET_AMOUNT,
    CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_TOKEN_DECIMALS,
    CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_TOKEN_DECIMALS
} from "./LibRaindex.sol";

uint256 constant SUB_PARSER_WORD_PARSERS_LENGTH = 2;
uint8 constant EXTERN_PARSE_META_BUILD_DEPTH = 1;

bytes constant WORD_SENDER = "sender";
bytes constant WORD_ORDER_CLEARER = "order-clearer";
bytes constant WORD_RAINDEX = "raindex";
bytes constant WORD_ORDER_HASH = "order-hash";
bytes constant WORD_ORDER_OWNER = "order-owner";
bytes constant WORD_ORDER_COUNTERPARTY = "order-counterparty";
bytes constant WORD_CALCULATED_MAX_OUTPUT = "calculated-max-output";
bytes constant WORD_CALCULATED_IO_RATIO = "calculated-io-ratio";
bytes constant WORD_INPUT_TOKEN = "input-token";
bytes constant WORD_INPUT_TOKEN_DECIMALS = "input-token-decimals";
bytes constant WORD_INPUT_VAULT_ID = "input-vault-id";
bytes constant WORD_INPUT_VAULT_BALANCE_BEFORE = "input-vault-before";
bytes constant WORD_INPUT_VAULT_BALANCE_INCREASE = "input-vault-increase";
bytes constant WORD_OUTPUT_TOKEN = "output-token";
bytes constant WORD_OUTPUT_TOKEN_DECIMALS = "output-token-decimals";
bytes constant WORD_OUTPUT_VAULT_ID = "output-vault-id";
bytes constant WORD_OUTPUT_VAULT_BALANCE_BEFORE = "output-vault-before";
bytes constant WORD_OUTPUT_VAULT_BALANCE_DECREASE = "output-vault-decrease";

bytes constant WORD_DEPOSITOR = "depositor";
bytes constant WORD_DEPOSIT_TOKEN = "deposit-token";
bytes constant WORD_DEPOSIT_VAULT_ID = "deposit-vault-id";
bytes constant WORD_DEPOSIT_VAULT_BEFORE = "deposit-vault-before";
bytes constant WORD_DEPOSIT_VAULT_AFTER = "deposit-vault-after";
bytes constant WORD_DEPOSIT_TOKEN_DECIMALS = "deposit-token-decimals";

bytes constant WORD_WITHDRAWER = "withdrawer";
bytes constant WORD_WITHDRAW_TOKEN = "withdraw-token";
bytes constant WORD_WITHDRAW_VAULT_ID = "withdraw-vault-id";
bytes constant WORD_WITHDRAW_VAULT_BEFORE = "withdraw-vault-before";
bytes constant WORD_WITHDRAW_VAULT_AFTER = "withdraw-vault-after";
bytes constant WORD_WITHDRAW_TARGET_AMOUNT = "withdraw-target-amount";
bytes constant WORD_WITHDRAW_TOKEN_DECIMALS = "withdraw-token-decimals";

uint256 constant DEPOSIT_WORD_DEPOSITOR = 0;
uint256 constant DEPOSIT_WORD_TOKEN = 1;
uint256 constant DEPOSIT_WORD_VAULT_ID = 2;
uint256 constant DEPOSIT_WORD_VAULT_BEFORE = 3;
uint256 constant DEPOSIT_WORD_VAULT_AFTER = 4;
uint256 constant DEPOSIT_WORD_TOKEN_DECIMALS = 5;
uint256 constant DEPOSIT_WORDS_LENGTH = 6;

uint256 constant WITHDRAW_WORD_WITHDRAWER = 0;
uint256 constant WITHDRAW_WORD_TOKEN = 1;
uint256 constant WITHDRAW_WORD_VAULT_ID = 2;
uint256 constant WITHDRAW_WORD_VAULT_BEFORE = 3;
uint256 constant WITHDRAW_WORD_VAULT_AFTER = 4;
uint256 constant WITHDRAW_WORD_TARGET_AMOUNT = 5;
uint256 constant WITHDRAW_WORD_TOKEN_DECIMALS = 6;
uint256 constant WITHDRAW_WORDS_LENGTH = 7;

/// @title LibRaindexSubParser
/// @notice Sub-parser word dispatch and authoring metadata for Raindex
/// context columns (base, calling, calculations, vault IO, signed).
library LibRaindexSubParser {
    using LibUint256Matrix for uint256[][];

    /// @dev Maps the "sender" word to the base context column sender row.
    function subParserSender(uint256, uint256, OperandV2) internal pure returns (bool, bytes memory, bytes32[] memory) {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_BASE_COLUMN, CONTEXT_BASE_ROW_SENDER);
    }

    /// @dev Maps the "order-clearer" word to the calling context column.
    function subParserOrderClearer(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_CALLING_CONTEXT_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER);
    }

    /// @dev Maps the "depositor" word to the deposit column.
    function subParserDepositor(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_DEPOSIT_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_DEPOSITOR);
    }

    /// @dev Maps the "withdrawer" word to the withdraw column.
    function subParserWithdrawer(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_WITHDRAW_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_WITHDRAWER);
    }

    /// @dev Maps the "calling-contract" word to the base context column.
    function subParserCallingContract(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_BASE_COLUMN, CONTEXT_BASE_ROW_CALLING_CONTRACT);
    }

    /// @dev Maps the "order-hash" word to the calling context column.
    function subParserOrderHash(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_CALLING_CONTEXT_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH);
    }

    /// @dev Maps the "order-owner" word to the calling context column.
    function subParserOrderOwner(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_CALLING_CONTEXT_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER);
    }

    /// @dev Maps the "order-counterparty" word to the calling context column.
    function subParserOrderCounterparty(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return
            LibSubParse.subParserContext(CONTEXT_CALLING_CONTEXT_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_ORDER_COUNTERPARTY);
    }

    /// @dev Maps the "max-output" word to the calculations context column.
    function subParserMaxOutput(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_CALCULATIONS_COLUMN, CONTEXT_CALCULATIONS_ROW_MAX_OUTPUT);
    }

    /// @dev Maps the "io-ratio" word to the calculations context column.
    function subParserIORatio(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_CALCULATIONS_COLUMN, CONTEXT_CALCULATIONS_ROW_IO_RATIO);
    }

    /// @dev Maps the "input-token" word to the vault inputs context column.
    function subParserInputToken(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_VAULT_INPUTS_COLUMN, CONTEXT_VAULT_IO_TOKEN);
    }

    /// @dev Maps the "input-token-decimals" word to the vault inputs context column.
    function subParserInputTokenDecimals(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_VAULT_INPUTS_COLUMN, CONTEXT_VAULT_IO_TOKEN_DECIMALS);
    }

    /// @dev Maps the "input-vault-id" word to the vault inputs context column.
    function subParserInputVaultId(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_VAULT_INPUTS_COLUMN, CONTEXT_VAULT_IO_VAULT_ID);
    }

    /// @dev Maps the "input-balance-before" word to the vault inputs context column.
    function subParserInputBalanceBefore(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_VAULT_INPUTS_COLUMN, CONTEXT_VAULT_IO_BALANCE_BEFORE);
    }

    /// @dev Maps the "input-balance-diff" word to the vault inputs context column.
    function subParserInputBalanceDiff(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_VAULT_INPUTS_COLUMN, CONTEXT_VAULT_IO_BALANCE_DIFF);
    }

    /// @dev Maps the "output-token" word to the vault outputs context column.
    function subParserOutputToken(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_VAULT_OUTPUTS_COLUMN, CONTEXT_VAULT_IO_TOKEN);
    }

    /// @dev Maps the "output-token-decimals" word to the vault outputs context column.
    function subParserOutputTokenDecimals(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_VAULT_OUTPUTS_COLUMN, CONTEXT_VAULT_IO_TOKEN_DECIMALS);
    }

    /// @dev Maps the "output-vault-id" word to the vault outputs context column.
    function subParserOutputVaultId(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_VAULT_OUTPUTS_COLUMN, CONTEXT_VAULT_IO_VAULT_ID);
    }

    /// @dev Maps the "output-balance-before" word to the vault outputs context column.
    function subParserOutputBalanceBefore(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_VAULT_OUTPUTS_COLUMN, CONTEXT_VAULT_IO_BALANCE_BEFORE);
    }

    /// @dev Maps the "output-balance-diff" word to the vault outputs context column.
    function subParserOutputBalanceDiff(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_VAULT_OUTPUTS_COLUMN, CONTEXT_VAULT_IO_BALANCE_DIFF);
    }

    /// @dev Maps the "signers" word to the signed context signers column.
    /// Uses the operand to select the row.
    function subParserSigners(uint256, uint256, OperandV2 operand)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN, uint256(OperandV2.unwrap(operand)));
    }

    /// @dev Maps the "signer-domain" word to the signed context domains column.
    /// Uses the operand to select the row.
    function subParserSignerDomains(uint256, uint256, OperandV2 operand)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_SIGNED_CONTEXT_DOMAINS_COLUMN, uint256(OperandV2.unwrap(operand)));
    }

    /// @dev Maps the "deposit-token" word to the deposit column.
    function subParserDepositToken(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_DEPOSIT_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_TOKEN);
    }

    /// @dev Maps the "deposit-vault-id" word to the deposit column.
    function subParserDepositVaultId(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_DEPOSIT_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_VAULT_ID);
    }

    /// @dev Maps the "deposit-vault-balance-before" word to the deposit column.
    function subParserDepositVaultBalanceBefore(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_DEPOSIT_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_VAULT_BEFORE);
    }

    /// @dev Maps the "deposit-vault-balance-after" word to the deposit column.
    function subParserDepositVaultBalanceAfter(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_DEPOSIT_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_VAULT_AFTER);
    }

    /// @dev Maps the "deposit-token-decimals" word to the deposit column.
    function subParserDepositTokenDecimals(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_DEPOSIT_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_TOKEN_DECIMALS);
    }

    /// @dev Maps the "withdraw-token" word to the withdraw column.
    function subParserWithdrawToken(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_WITHDRAW_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_TOKEN);
    }

    /// @dev Maps the "withdraw-vault-id" word to the withdraw column.
    function subParserWithdrawVaultId(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_WITHDRAW_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_VAULT_ID);
    }

    /// @dev Maps the "withdraw-vault-balance-before" word to the withdraw column.
    function subParserWithdrawVaultBalanceBefore(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_WITHDRAW_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_VAULT_BEFORE);
    }

    /// @dev Maps the "withdraw-vault-balance-after" word to the withdraw column.
    function subParserWithdrawVaultBalanceAfter(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_WITHDRAW_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_VAULT_AFTER);
    }

    /// @dev Maps the "withdraw-target-amount" word to the withdraw column.
    function subParserWithdrawTargetAmount(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_WITHDRAW_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_TARGET_AMOUNT);
    }

    /// @dev Maps the "withdraw-token-decimals" word to the withdraw column.
    function subParserWithdrawTokenDecimals(uint256, uint256, OperandV2)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        //slither-disable-next-line unused-return
        return
            LibSubParse.subParserContext(CONTEXT_WITHDRAW_COLUMN, CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_TOKEN_DECIMALS);
    }

    /// @dev Maps the "signed-context" word to a signed context column/row.
    /// The operand low byte selects the column offset, the next byte selects the row.
    function subParserSignedContext(uint256, uint256, OperandV2 operand)
        internal
        pure
        returns (bool, bytes memory, bytes32[] memory)
    {
        uint256 column = uint256(OperandV2.unwrap(operand)) & 0xFF;
        uint256 row = uint256(OperandV2.unwrap(operand)) >> 8;
        //slither-disable-next-line unused-return
        return LibSubParse.subParserContext(CONTEXT_SIGNED_CONTEXT_START_COLUMN + column, row);
    }

    /// @dev Builds the complete authoring metadata for all raindex context
    /// words. Returns ABI-encoded `AuthoringMetaV2[]` covering every context
    /// column (base, order, calculations, vault IO, deposit, withdraw, signers,
    /// signer domains, signed context). The inner arrays are flattened before
    /// encoding.
    //slither-disable-next-line dead-code
    function authoringMetaV2() internal pure returns (bytes memory) {
        AuthoringMetaV2[][] memory meta = new AuthoringMetaV2[][](CONTEXT_COLUMNS_EXTENDED);

        AuthoringMetaV2[] memory contextBaseMeta = new AuthoringMetaV2[](CONTEXT_BASE_ROWS);
        contextBaseMeta[CONTEXT_BASE_ROW_SENDER] =
        // constant WORD_SENDER defined above is less than 32 bytes, so this
        // conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_SENDER), "The address that called the raindex entrypoint.");
        contextBaseMeta[CONTEXT_BASE_ROW_CALLING_CONTRACT] =
        // constant WORD_RAINDEX defined above is less than 32 bytes, so
        // this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_RAINDEX), "The address of the raindex that the order is being run on.");

        AuthoringMetaV2[] memory contextCallingContextMeta = new AuthoringMetaV2[](CONTEXT_CALLING_CONTEXT_ROWS);
        contextCallingContextMeta[CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH] =
        // constant WORD_ORDER_HASH defined above is less than 32 bytes, so
        // this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_ORDER_HASH), "The hash of the order that is being cleared.");
        contextCallingContextMeta[CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER] =
        // constant  WORD_ORDER_OWNER defined above is less than 32 bytes, so
        // this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_ORDER_OWNER), "The address of the order owner.");
        contextCallingContextMeta[CONTEXT_CALLING_CONTEXT_ROW_ORDER_COUNTERPARTY] = AuthoringMetaV2(
            // constant WORD_ORDER_COUNTERPARTY defined above is less than 32
            // bytes, so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_ORDER_COUNTERPARTY),
            "The address of the owner of the counterparty order. Will be the order taker if there is no counterparty order."
        );
        contextCallingContextMeta[CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER] = AuthoringMetaV2(
            // constant WORD_ORDER_CLEARER defined above is less than 32 bytes,
            // so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_ORDER_CLEARER),
            "The address that called clear, matching this order against a counterparty order. Only available in clears; reverts when the order is taken or quoted."
        );

        AuthoringMetaV2[] memory contextCalculationsMeta = new AuthoringMetaV2[](CONTEXT_CALCULATIONS_ROWS);
        contextCalculationsMeta[CONTEXT_CALCULATIONS_ROW_MAX_OUTPUT] = AuthoringMetaV2(
            // constant WORD_CALCULATED_MAX_OUTPUT defined above is less than
            // 32 bytes, so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_CALCULATED_MAX_OUTPUT),
            "The maximum output of the order, i.e. the maximum amount of the output token that the order will send. This is 0 before calculations have been run."
        );
        contextCalculationsMeta[CONTEXT_CALCULATIONS_ROW_IO_RATIO] = AuthoringMetaV2(
            // constant WORD_CALCULATED_IO_RATIO defined above is less than
            // 32 bytes, so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_CALCULATED_IO_RATIO),
            "The ratio of the input to output token, i.e. the amount of the input token that the order will receive for each unit of the output token that it sends. This is 0 before calculations have been run."
        );

        AuthoringMetaV2[] memory contextVaultInputsMeta = new AuthoringMetaV2[](CONTEXT_VAULT_IO_ROWS);
        contextVaultInputsMeta[CONTEXT_VAULT_IO_TOKEN] =
        // constant WORD_INPUT_TOKEN defined above is less than 32 bytes, so this
        // conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_INPUT_TOKEN), "The address of the input token for the vault input.");
        contextVaultInputsMeta[CONTEXT_VAULT_IO_TOKEN_DECIMALS] =
        // constant WORD_INPUT_TOKEN_DECIMALS defined above is less than
        // 32 bytes, so this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_INPUT_TOKEN_DECIMALS), "The decimals of the input token for the vault input.");
        contextVaultInputsMeta[CONTEXT_VAULT_IO_VAULT_ID] = AuthoringMetaV2(
            // constant WORD_INPUT_VAULT_ID defined above is less than 32 bytes,
            // so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_INPUT_VAULT_ID),
            "The ID of the input vault that incoming tokens are received into."
        );
        contextVaultInputsMeta[CONTEXT_VAULT_IO_BALANCE_BEFORE] = AuthoringMetaV2(
            // constant WORD_INPUT_VAULT_BALANCE_BEFORE defined above is less
            // than 32 bytes, so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_INPUT_VAULT_BALANCE_BEFORE),
            "The balance of the input vault before the order is cleared as a uint256 value."
        );
        contextVaultInputsMeta[CONTEXT_VAULT_IO_BALANCE_DIFF] = AuthoringMetaV2(
            // constant WORD_INPUT_VAULT_BALANCE_INCREASE defined above is less
            // than 32 bytes, so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_INPUT_VAULT_BALANCE_INCREASE),
            "The difference in the balance of the input vault after the order is cleared as a uint256 value. This is always positive so it must be added to the input balance before to get the final vault balance. This is 0 before calculations have been run."
        );

        AuthoringMetaV2[] memory contextVaultOutputsMeta = new AuthoringMetaV2[](CONTEXT_VAULT_IO_ROWS);
        contextVaultOutputsMeta[CONTEXT_VAULT_IO_TOKEN] =
        // constant WORD_OUTPUT_TOKEN defined above is less than 32 bytes, so
        // this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_OUTPUT_TOKEN), "The address of the output token for the vault output.");
        contextVaultOutputsMeta[CONTEXT_VAULT_IO_TOKEN_DECIMALS] = AuthoringMetaV2(
            // constant WORD_OUTPUT_TOKEN_DECIMALS defined above is less than
            // 32 bytes, so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_OUTPUT_TOKEN_DECIMALS),
            "The decimals of the output token for the vault output."
        );
        contextVaultOutputsMeta[CONTEXT_VAULT_IO_VAULT_ID] = AuthoringMetaV2(
            // constant WORD_OUTPUT_VAULT_ID defined above is less than 32 bytes,
            // so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_OUTPUT_VAULT_ID),
            "The ID of the output vault that outgoing tokens are sent from."
        );
        contextVaultOutputsMeta[CONTEXT_VAULT_IO_BALANCE_BEFORE] = AuthoringMetaV2(
            // constant WORD_OUTPUT_VAULT_BALANCE_BEFORE defined above is less
            // than 32 bytes, so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_OUTPUT_VAULT_BALANCE_BEFORE),
            "The balance of the output vault before the order is cleared as a uint256 value."
        );
        contextVaultOutputsMeta[CONTEXT_VAULT_IO_BALANCE_DIFF] = AuthoringMetaV2(
            // constant WORD_OUTPUT_VAULT_BALANCE_DECREASE defined above is less
            // than 32 bytes, so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_OUTPUT_VAULT_BALANCE_DECREASE),
            "The difference in the balance of the output vault after the order is cleared as a uint256 value. This is always positive so it must be subtracted from the output balance before to get the final vault balance. This is 0 before calculations have been run."
        );

        AuthoringMetaV2[] memory contextSignersMeta = new AuthoringMetaV2[](CONTEXT_SIGNED_CONTEXT_SIGNERS_ROWS);
        contextSignersMeta[CONTEXT_SIGNED_CONTEXT_SIGNERS_ROW] = AuthoringMetaV2(
            // string literal "signer" is less than 32 bytes, so this
            // conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32("signer"),
            "The addresses of the signers of the signed context. The indexes of the signers matches the column they signed in the signed context grid."
        );

        AuthoringMetaV2[] memory contextSignerDomainsMeta = new AuthoringMetaV2[](CONTEXT_SIGNED_CONTEXT_DOMAINS_ROWS);
        contextSignerDomainsMeta[CONTEXT_SIGNED_CONTEXT_DOMAINS_ROW] = AuthoringMetaV2(
            // string literal "signer-domain" is less than 32 bytes, so this
            // conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32("signer-domain"),
            "The EIP-712 domain separators the signed contexts were signed under. The indexes of the domains match the signers. The signer chooses the domain, so the expression author must pin each signer's domain."
        );

        AuthoringMetaV2[] memory contextSignedMeta = new AuthoringMetaV2[](CONTEXT_SIGNED_CONTEXT_START_ROWS);
        contextSignedMeta[CONTEXT_SIGNED_CONTEXT_START_ROW] = AuthoringMetaV2(
            // string literal "signed-context" is less than 32 bytes, so this
            // conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32("signed-context"),
            "Signed context is provided by the order clearer/taker and can be signed by anyone. Raindex will check the signature, but the expression author must authorize the signer's public key."
        );

        meta[CONTEXT_BASE_COLUMN] = contextBaseMeta;
        meta[CONTEXT_CALLING_CONTEXT_COLUMN] = contextCallingContextMeta;
        meta[CONTEXT_CALCULATIONS_COLUMN] = contextCalculationsMeta;
        meta[CONTEXT_VAULT_INPUTS_COLUMN] = contextVaultInputsMeta;
        meta[CONTEXT_VAULT_OUTPUTS_COLUMN] = contextVaultOutputsMeta;
        meta[CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN] = contextSignersMeta;
        meta[CONTEXT_SIGNED_CONTEXT_DOMAINS_COLUMN] = contextSignerDomainsMeta;
        meta[CONTEXT_SIGNED_CONTEXT_START_COLUMN] = contextSignedMeta;

        AuthoringMetaV2[] memory depositMeta = new AuthoringMetaV2[](DEPOSIT_WORDS_LENGTH);
        depositMeta[DEPOSIT_WORD_DEPOSITOR] =
        // constant WORD_DEPOSITOR defined above is less than
        // 32 bytes, so this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_DEPOSITOR), "The address of the depositor that is depositing the token.");
        depositMeta[DEPOSIT_WORD_TOKEN] =
        // constant WORD_DEPOSIT_TOKEN defined above is less than
        // 32 bytes, so this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_DEPOSIT_TOKEN), "The address of the token that is being deposited.");
        depositMeta[DEPOSIT_WORD_VAULT_ID] = AuthoringMetaV2(
            // constant WORD_DEPOSIT_VAULT_ID defined above is less than
            // 32 bytes, so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_DEPOSIT_VAULT_ID),
            "The ID of the vault that the token is being deposited into."
        );
        depositMeta[DEPOSIT_WORD_VAULT_BEFORE] =
        // constant WORD_DEPOSIT_VAULT_BEFORE defined above is less than
        // 32 bytes, so this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_DEPOSIT_VAULT_BEFORE), "The balance of the vault before the deposit.");
        depositMeta[DEPOSIT_WORD_VAULT_AFTER] =
        // constant WORD_DEPOSIT_VAULT_AFTER defined above is less than
        // 32 bytes, so this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_DEPOSIT_VAULT_AFTER), "The balance of the vault after deposit.");
        depositMeta[DEPOSIT_WORD_TOKEN_DECIMALS] =
        // constant WORD_DEPOSIT_TOKEN_DECIMALS defined above is less than
        // 32 bytes, so this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_DEPOSIT_TOKEN_DECIMALS), "The decimals of the token that is being deposited.");

        meta[CONTEXT_DEPOSIT_COLUMN] = depositMeta;

        AuthoringMetaV2[] memory withdrawMeta = new AuthoringMetaV2[](WITHDRAW_WORDS_LENGTH);
        withdrawMeta[WITHDRAW_WORD_WITHDRAWER] =
        // constant WORD_WITHDRAWER defined above is less than
        // 32 bytes, so this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_WITHDRAWER), "The address of the withdrawer that is withdrawing the token.");
        withdrawMeta[WITHDRAW_WORD_TOKEN] =
        // constant WORD_WITHDRAW_TOKEN defined above is less than
        // 32 bytes, so this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_WITHDRAW_TOKEN), "The address of the token that is being withdrawn.");
        withdrawMeta[WITHDRAW_WORD_VAULT_ID] = AuthoringMetaV2(
            // constant WORD_WITHDRAW_VAULT_ID defined above is less than
            // 32 bytes, so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_WITHDRAW_VAULT_ID),
            "The ID of the vault that the token is being withdrawn from."
        );
        withdrawMeta[WITHDRAW_WORD_VAULT_BEFORE] =
        // constant WORD_WITHDRAW_VAULT_BEFORE defined above is less than
        // 32 bytes, so this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_WITHDRAW_VAULT_BEFORE), "The balance of the vault before the withdrawal.");
        withdrawMeta[WITHDRAW_WORD_VAULT_AFTER] =
        // constant WORD_WITHDRAW_VAULT_AFTER defined above is less than
        // 32 bytes, so this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_WITHDRAW_VAULT_AFTER), "The balance of the vault after withdrawal.");
        withdrawMeta[WITHDRAW_WORD_TARGET_AMOUNT] = AuthoringMetaV2(
            // constant WORD_WITHDRAW_TARGET_AMOUNT defined above is less than
            // 32 bytes, so this conversion is safe.
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes32(WORD_WITHDRAW_TARGET_AMOUNT),
            "The target amount of the token that the withdrawer is trying to withdraw. This is the amount that the withdrawer is trying to withdraw, but it MAY NOT be the amount that the withdrawer actually receives."
        );
        withdrawMeta[WITHDRAW_WORD_TOKEN_DECIMALS] =
        // constant WORD_WITHDRAW_TOKEN_DECIMALS defined above is less than
        // 32 bytes, so this conversion is safe.
        // forge-lint: disable-next-line(unsafe-typecast)
        AuthoringMetaV2(bytes32(WORD_WITHDRAW_TOKEN_DECIMALS), "The decimals of the token that is being withdrawn.");

        meta[CONTEXT_WITHDRAW_COLUMN] = withdrawMeta;

        uint256[][] memory metaUint256;
        assembly {
            metaUint256 := meta
        }
        uint256[] memory metaUint256Flattened = metaUint256.flatten();
        AuthoringMetaV2[] memory metaFlattened;
        assembly {
            metaFlattened := metaUint256Flattened
        }

        return abi.encode(metaFlattened);
    }
}
