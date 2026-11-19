// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {
    LibParseOperand,
    BaseRainlangSubParser,
    OperandV2,
    IParserToolingV1
} from "rainlang-0.2.40/src/abstract/BaseRainlangSubParser.sol";
import {LibConvert} from "rain-lib-typecast-0.1.4/src/LibConvert.sol";
import {LibUint256Matrix} from "rain-solmem-0.1.28/src/lib/LibUint256Matrix.sol";

import {
    LibRaindexSubParser,
    SUB_PARSER_WORD_PARSERS_LENGTH,
    DEPOSIT_WORD_VAULT_ID,
    DEPOSIT_WORD_TOKEN,
    DEPOSIT_WORD_DEPOSITOR,
    DEPOSIT_WORD_VAULT_BEFORE,
    DEPOSIT_WORD_VAULT_AFTER,
    DEPOSIT_WORD_TOKEN_DECIMALS,
    DEPOSIT_WORDS_LENGTH,
    WITHDRAW_WORD_WITHDRAWER,
    WITHDRAW_WORD_TOKEN,
    WITHDRAW_WORD_VAULT_ID,
    WITHDRAW_WORD_VAULT_BEFORE,
    WITHDRAW_WORD_VAULT_AFTER,
    WITHDRAW_WORD_TARGET_AMOUNT,
    WITHDRAW_WORD_TOKEN_DECIMALS,
    WITHDRAW_WORDS_LENGTH
} from "../../lib/LibRaindexSubParser.sol";
import {
    CONTEXT_BASE_COLUMN,
    CONTEXT_BASE_ROW_SENDER,
    CONTEXT_BASE_ROW_CALLING_CONTRACT,
    CONTEXT_BASE_ROWS
} from "rainlang-0.2.40/src/lib/caller/LibContext.sol";
import {
    CONTEXT_COLUMNS_EXTENDED,
    CONTEXT_VAULT_OUTPUTS_COLUMN,
    CONTEXT_VAULT_INPUTS_COLUMN,
    CONTEXT_CALCULATIONS_COLUMN,
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
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER,
    CONTEXT_DEPOSIT_COLUMN,
    CONTEXT_WITHDRAW_COLUMN,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH,
    CONTEXT_CALLING_CONTEXT_ROWS,
    CONTEXT_CALLING_CONTEXT_COLUMN,
    CONTEXT_SIGNED_CONTEXT_START_COLUMN,
    CONTEXT_SIGNED_CONTEXT_START_ROW,
    CONTEXT_SIGNED_CONTEXT_START_ROWS,
    CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN,
    CONTEXT_SIGNED_CONTEXT_SIGNERS_ROW,
    CONTEXT_SIGNED_CONTEXT_SIGNERS_ROWS,
    CONTEXT_SIGNED_CONTEXT_DOMAINS_COLUMN,
    CONTEXT_SIGNED_CONTEXT_DOMAINS_ROW,
    CONTEXT_SIGNED_CONTEXT_DOMAINS_ROWS
} from "../../lib/LibRaindex.sol";
import {
    DESCRIBED_BY_META_HASH,
    PARSE_META as SUB_PARSER_PARSE_META,
    SUB_PARSER_WORD_PARSERS,
    OPERAND_HANDLER_FUNCTION_POINTERS as SUB_PARSER_OPERAND_HANDLERS
} from "../../generated/RaindexV7SubParserPointers.sol";
import {IDescribedByMetaV1} from "rain-metadata-0.1.46/src/interface/IDescribedByMetaV1.sol";
import {ISubParserToolingV1} from "rain-sol-codegen-0.1.39/src/interface/ISubParserToolingV1.sol";

/// @title RaindexV7SubParser
/// @notice Sub-parser that provides raindex-specific context words (sender,
/// order hash, vault IO, deposit/withdraw, signed context, etc.) to the
/// Rain interpreter.
contract RaindexV7SubParser is BaseRainlangSubParser {
    using LibUint256Matrix for uint256[][];

    /// @inheritdoc IDescribedByMetaV1
    function describedByMetaV1() external pure returns (bytes32) {
        return DESCRIBED_BY_META_HASH;
    }

    /// @inheritdoc BaseRainlangSubParser
    function subParserParseMeta() internal pure virtual override returns (bytes memory) {
        return SUB_PARSER_PARSE_META;
    }

    /// @inheritdoc BaseRainlangSubParser
    function subParserWordParsers() internal pure virtual override returns (bytes memory) {
        return SUB_PARSER_WORD_PARSERS;
    }

    /// @inheritdoc BaseRainlangSubParser
    function subParserOperandHandlers() internal pure virtual override returns (bytes memory) {
        return SUB_PARSER_OPERAND_HANDLERS;
    }

    /// @inheritdoc IParserToolingV1
    function buildLiteralParserFunctionPointers() external pure returns (bytes memory) {
        return "";
    }

    /// @inheritdoc IParserToolingV1
    function buildOperandHandlerFunctionPointers() external pure returns (bytes memory) {
        function(bytes32[] memory) internal pure returns (OperandV2)[][] memory handlers =
            new function(bytes32[] memory) internal pure returns (OperandV2)[][](CONTEXT_COLUMNS_EXTENDED);

        function(bytes32[] memory) internal pure returns (OperandV2)[] memory contextBaseHandlers =
            new function(bytes32[] memory) internal pure returns (OperandV2)[](CONTEXT_BASE_ROWS);
        contextBaseHandlers[CONTEXT_BASE_ROW_SENDER] = LibParseOperand.handleOperandDisallowed;
        contextBaseHandlers[CONTEXT_BASE_ROW_CALLING_CONTRACT] = LibParseOperand.handleOperandDisallowed;

        function(bytes32[] memory) internal pure returns (OperandV2)[] memory contextCallingContextHandlers =
            new function(bytes32[] memory) internal pure returns (OperandV2)[](CONTEXT_CALLING_CONTEXT_ROWS);
        contextCallingContextHandlers[CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH] = LibParseOperand.handleOperandDisallowed;
        contextCallingContextHandlers[CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER] = LibParseOperand.handleOperandDisallowed;
        contextCallingContextHandlers[CONTEXT_CALLING_CONTEXT_ROW_ORDER_COUNTERPARTY] =
        LibParseOperand.handleOperandDisallowed;
        contextCallingContextHandlers[CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER] =
        LibParseOperand.handleOperandDisallowed;

        function(bytes32[] memory) internal pure returns (OperandV2)[] memory contextCalculationsHandlers =
            new function(bytes32[] memory) internal pure returns (OperandV2)[](CONTEXT_CALCULATIONS_ROWS);
        contextCalculationsHandlers[CONTEXT_CALCULATIONS_ROW_MAX_OUTPUT] = LibParseOperand.handleOperandDisallowed;
        contextCalculationsHandlers[CONTEXT_CALCULATIONS_ROW_IO_RATIO] = LibParseOperand.handleOperandDisallowed;

        function(bytes32[] memory) internal pure returns (OperandV2)[] memory contextVaultInputsHandlers =
            new function(bytes32[] memory) internal pure returns (OperandV2)[](CONTEXT_VAULT_IO_ROWS);
        contextVaultInputsHandlers[CONTEXT_VAULT_IO_TOKEN] = LibParseOperand.handleOperandDisallowed;
        contextVaultInputsHandlers[CONTEXT_VAULT_IO_TOKEN_DECIMALS] = LibParseOperand.handleOperandDisallowed;
        contextVaultInputsHandlers[CONTEXT_VAULT_IO_VAULT_ID] = LibParseOperand.handleOperandDisallowed;
        contextVaultInputsHandlers[CONTEXT_VAULT_IO_BALANCE_BEFORE] = LibParseOperand.handleOperandDisallowed;
        contextVaultInputsHandlers[CONTEXT_VAULT_IO_BALANCE_DIFF] = LibParseOperand.handleOperandDisallowed;

        function(bytes32[] memory) internal pure returns (OperandV2)[] memory contextVaultOutputsHandlers =
            new function(bytes32[] memory) internal pure returns (OperandV2)[](CONTEXT_VAULT_IO_ROWS);
        contextVaultOutputsHandlers[CONTEXT_VAULT_IO_TOKEN] = LibParseOperand.handleOperandDisallowed;
        contextVaultOutputsHandlers[CONTEXT_VAULT_IO_TOKEN_DECIMALS] = LibParseOperand.handleOperandDisallowed;
        contextVaultOutputsHandlers[CONTEXT_VAULT_IO_VAULT_ID] = LibParseOperand.handleOperandDisallowed;
        contextVaultOutputsHandlers[CONTEXT_VAULT_IO_BALANCE_BEFORE] = LibParseOperand.handleOperandDisallowed;
        contextVaultOutputsHandlers[CONTEXT_VAULT_IO_BALANCE_DIFF] = LibParseOperand.handleOperandDisallowed;

        function(bytes32[] memory) internal pure returns (OperandV2)[] memory contextSignersHandlers =
            new function(bytes32[] memory) internal pure returns (OperandV2)[](CONTEXT_SIGNED_CONTEXT_SIGNERS_ROWS);
        contextSignersHandlers[CONTEXT_SIGNED_CONTEXT_SIGNERS_ROW] = LibParseOperand.handleOperandSingleFullNoDefault;

        function(bytes32[] memory) internal pure returns (OperandV2)[] memory contextSignerDomainsHandlers =
            new function(bytes32[] memory) internal pure returns (OperandV2)[](CONTEXT_SIGNED_CONTEXT_DOMAINS_ROWS);
        contextSignerDomainsHandlers[CONTEXT_SIGNED_CONTEXT_DOMAINS_ROW] =
        LibParseOperand.handleOperandSingleFullNoDefault;

        function(bytes32[] memory) internal pure returns (OperandV2)[] memory contextSignedContextHandlers =
            new function(bytes32[] memory) internal pure returns (OperandV2)[](CONTEXT_SIGNED_CONTEXT_START_ROWS);
        contextSignedContextHandlers[CONTEXT_SIGNED_CONTEXT_START_ROW] =
        LibParseOperand.handleOperandDoublePerByteNoDefault;

        handlers[CONTEXT_BASE_COLUMN] = contextBaseHandlers;
        handlers[CONTEXT_CALLING_CONTEXT_COLUMN] = contextCallingContextHandlers;
        handlers[CONTEXT_CALCULATIONS_COLUMN] = contextCalculationsHandlers;
        handlers[CONTEXT_VAULT_INPUTS_COLUMN] = contextVaultInputsHandlers;
        handlers[CONTEXT_VAULT_OUTPUTS_COLUMN] = contextVaultOutputsHandlers;
        handlers[CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN] = contextSignersHandlers;
        handlers[CONTEXT_SIGNED_CONTEXT_DOMAINS_COLUMN] = contextSignerDomainsHandlers;
        handlers[CONTEXT_SIGNED_CONTEXT_START_COLUMN] = contextSignedContextHandlers;

        function(bytes32[] memory) internal pure returns (OperandV2)[] memory contextDepositContextHandlers =
            new function(bytes32[] memory) internal pure returns (OperandV2)[](DEPOSIT_WORDS_LENGTH);
        contextDepositContextHandlers[DEPOSIT_WORD_DEPOSITOR] = LibParseOperand.handleOperandDisallowed;
        contextDepositContextHandlers[DEPOSIT_WORD_TOKEN] = LibParseOperand.handleOperandDisallowed;
        contextDepositContextHandlers[DEPOSIT_WORD_VAULT_ID] = LibParseOperand.handleOperandDisallowed;
        contextDepositContextHandlers[DEPOSIT_WORD_VAULT_BEFORE] = LibParseOperand.handleOperandDisallowed;
        contextDepositContextHandlers[DEPOSIT_WORD_VAULT_AFTER] = LibParseOperand.handleOperandDisallowed;
        contextDepositContextHandlers[DEPOSIT_WORD_TOKEN_DECIMALS] = LibParseOperand.handleOperandDisallowed;

        handlers[CONTEXT_DEPOSIT_COLUMN] = contextDepositContextHandlers;

        function(bytes32[] memory) internal pure returns (OperandV2)[] memory contextWithdrawContextHandlers =
            new function(bytes32[] memory) internal pure returns (OperandV2)[](WITHDRAW_WORDS_LENGTH);
        contextWithdrawContextHandlers[WITHDRAW_WORD_WITHDRAWER] = LibParseOperand.handleOperandDisallowed;
        contextWithdrawContextHandlers[WITHDRAW_WORD_TOKEN] = LibParseOperand.handleOperandDisallowed;
        contextWithdrawContextHandlers[WITHDRAW_WORD_VAULT_ID] = LibParseOperand.handleOperandDisallowed;
        contextWithdrawContextHandlers[WITHDRAW_WORD_VAULT_BEFORE] = LibParseOperand.handleOperandDisallowed;
        contextWithdrawContextHandlers[WITHDRAW_WORD_VAULT_AFTER] = LibParseOperand.handleOperandDisallowed;
        contextWithdrawContextHandlers[WITHDRAW_WORD_TARGET_AMOUNT] = LibParseOperand.handleOperandDisallowed;
        contextWithdrawContextHandlers[WITHDRAW_WORD_TOKEN_DECIMALS] = LibParseOperand.handleOperandDisallowed;

        handlers[CONTEXT_WITHDRAW_COLUMN] = contextWithdrawContextHandlers;

        uint256[][] memory handlersUint256;
        assembly ("memory-safe") {
            handlersUint256 := handlers
        }

        return LibConvert.unsafeTo16BitBytes(handlersUint256.flatten());
    }

    /// @inheritdoc ISubParserToolingV1
    function buildSubParserWordParsers() external pure returns (bytes memory) {
        function(uint256, uint256, OperandV2) internal view returns (bool, bytes memory, bytes32[] memory)[][] memory
            parsers =
            new function(uint256, uint256, OperandV2)
            internal
            view returns (bool, bytes memory, bytes32[] memory)[][](CONTEXT_COLUMNS_EXTENDED);

        function(uint256, uint256, OperandV2) internal view returns (bool, bytes memory, bytes32[] memory)[] memory
            contextBaseParsers =
            new function(uint256, uint256, OperandV2)
            internal
            view returns (bool, bytes memory, bytes32[] memory)[](CONTEXT_BASE_ROWS);
        contextBaseParsers[CONTEXT_BASE_ROW_SENDER] = LibRaindexSubParser.subParserSender;
        contextBaseParsers[CONTEXT_BASE_ROW_CALLING_CONTRACT] = LibRaindexSubParser.subParserCallingContract;

        function(uint256, uint256, OperandV2) internal view returns (bool, bytes memory, bytes32[] memory)[] memory
            contextCallingContextParsers =
            new function(uint256, uint256, OperandV2)
            internal
            view returns (bool, bytes memory, bytes32[] memory)[](CONTEXT_CALLING_CONTEXT_ROWS);
        contextCallingContextParsers[CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH] = LibRaindexSubParser.subParserOrderHash;
        contextCallingContextParsers[CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER] = LibRaindexSubParser.subParserOrderOwner;
        contextCallingContextParsers[CONTEXT_CALLING_CONTEXT_ROW_ORDER_COUNTERPARTY] =
        LibRaindexSubParser.subParserOrderCounterparty;
        contextCallingContextParsers[CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER] =
        LibRaindexSubParser.subParserOrderClearer;

        function(uint256, uint256, OperandV2) internal view returns (bool, bytes memory, bytes32[] memory)[] memory
            contextCalculationsParsers =
            new function(uint256, uint256, OperandV2)
            internal
            view returns (bool, bytes memory, bytes32[] memory)[](CONTEXT_CALCULATIONS_ROWS);
        contextCalculationsParsers[CONTEXT_CALCULATIONS_ROW_MAX_OUTPUT] = LibRaindexSubParser.subParserMaxOutput;
        contextCalculationsParsers[CONTEXT_CALCULATIONS_ROW_IO_RATIO] = LibRaindexSubParser.subParserIORatio;

        function(uint256, uint256, OperandV2) internal view returns (bool, bytes memory, bytes32[] memory)[] memory
            contextVaultInputsParsers =
            new function(uint256, uint256, OperandV2)
            internal
            view returns (bool, bytes memory, bytes32[] memory)[](CONTEXT_VAULT_IO_ROWS);
        contextVaultInputsParsers[CONTEXT_VAULT_IO_TOKEN] = LibRaindexSubParser.subParserInputToken;
        contextVaultInputsParsers[CONTEXT_VAULT_IO_TOKEN_DECIMALS] = LibRaindexSubParser.subParserInputTokenDecimals;
        contextVaultInputsParsers[CONTEXT_VAULT_IO_VAULT_ID] = LibRaindexSubParser.subParserInputVaultId;
        contextVaultInputsParsers[CONTEXT_VAULT_IO_BALANCE_BEFORE] = LibRaindexSubParser.subParserInputBalanceBefore;
        contextVaultInputsParsers[CONTEXT_VAULT_IO_BALANCE_DIFF] = LibRaindexSubParser.subParserInputBalanceDiff;

        function(uint256, uint256, OperandV2) internal view returns (bool, bytes memory, bytes32[] memory)[] memory
            contextVaultOutputsParsers =
            new function(uint256, uint256, OperandV2)
            internal
            view returns (bool, bytes memory, bytes32[] memory)[](CONTEXT_VAULT_IO_ROWS);
        contextVaultOutputsParsers[CONTEXT_VAULT_IO_TOKEN] = LibRaindexSubParser.subParserOutputToken;
        contextVaultOutputsParsers[CONTEXT_VAULT_IO_TOKEN_DECIMALS] = LibRaindexSubParser.subParserOutputTokenDecimals;
        contextVaultOutputsParsers[CONTEXT_VAULT_IO_VAULT_ID] = LibRaindexSubParser.subParserOutputVaultId;
        contextVaultOutputsParsers[CONTEXT_VAULT_IO_BALANCE_BEFORE] = LibRaindexSubParser.subParserOutputBalanceBefore;
        contextVaultOutputsParsers[CONTEXT_VAULT_IO_BALANCE_DIFF] = LibRaindexSubParser.subParserOutputBalanceDiff;

        function(uint256, uint256, OperandV2) internal view returns (bool, bytes memory, bytes32[] memory)[] memory
            contextSignersParsers =
            new function(uint256, uint256, OperandV2)
            internal
            view returns (bool, bytes memory, bytes32[] memory)[](CONTEXT_SIGNED_CONTEXT_SIGNERS_ROWS);
        contextSignersParsers[CONTEXT_SIGNED_CONTEXT_SIGNERS_ROW] = LibRaindexSubParser.subParserSigners;

        function(uint256, uint256, OperandV2) internal view returns (bool, bytes memory, bytes32[] memory)[] memory
            contextSignerDomainsParsers =
            new function(uint256, uint256, OperandV2)
            internal
            view returns (bool, bytes memory, bytes32[] memory)[](CONTEXT_SIGNED_CONTEXT_DOMAINS_ROWS);
        contextSignerDomainsParsers[CONTEXT_SIGNED_CONTEXT_DOMAINS_ROW] = LibRaindexSubParser.subParserSignerDomains;

        function(uint256, uint256, OperandV2) internal view returns (bool, bytes memory, bytes32[] memory)[] memory
            contextSignedContextParsers =
            new function(uint256, uint256, OperandV2)
            internal
            view returns (bool, bytes memory, bytes32[] memory)[](CONTEXT_SIGNED_CONTEXT_START_ROWS);
        contextSignedContextParsers[CONTEXT_SIGNED_CONTEXT_START_ROW] = LibRaindexSubParser.subParserSignedContext;

        parsers[CONTEXT_BASE_COLUMN] = contextBaseParsers;
        parsers[CONTEXT_CALLING_CONTEXT_COLUMN] = contextCallingContextParsers;
        parsers[CONTEXT_CALCULATIONS_COLUMN] = contextCalculationsParsers;
        parsers[CONTEXT_VAULT_INPUTS_COLUMN] = contextVaultInputsParsers;
        parsers[CONTEXT_VAULT_OUTPUTS_COLUMN] = contextVaultOutputsParsers;
        parsers[CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN] = contextSignersParsers;
        parsers[CONTEXT_SIGNED_CONTEXT_DOMAINS_COLUMN] = contextSignerDomainsParsers;
        parsers[CONTEXT_SIGNED_CONTEXT_START_COLUMN] = contextSignedContextParsers;

        // Deposits

        function(uint256, uint256, OperandV2) internal view returns (bool, bytes memory, bytes32[] memory)[] memory
            depositParsers =
            new function(uint256, uint256, OperandV2)
            internal
            view returns (bool, bytes memory, bytes32[] memory)[](DEPOSIT_WORDS_LENGTH);

        depositParsers[DEPOSIT_WORD_DEPOSITOR] = LibRaindexSubParser.subParserDepositor;
        depositParsers[DEPOSIT_WORD_TOKEN] = LibRaindexSubParser.subParserDepositToken;
        depositParsers[DEPOSIT_WORD_VAULT_ID] = LibRaindexSubParser.subParserDepositVaultId;
        depositParsers[DEPOSIT_WORD_VAULT_BEFORE] = LibRaindexSubParser.subParserDepositVaultBalanceBefore;
        depositParsers[DEPOSIT_WORD_VAULT_AFTER] = LibRaindexSubParser.subParserDepositVaultBalanceAfter;
        depositParsers[DEPOSIT_WORD_TOKEN_DECIMALS] = LibRaindexSubParser.subParserDepositTokenDecimals;

        parsers[CONTEXT_DEPOSIT_COLUMN] = depositParsers;

        // Withdraws

        function(uint256, uint256, OperandV2) internal view returns (bool, bytes memory, bytes32[] memory)[] memory
            withdrawParsers =
            new function(uint256, uint256, OperandV2)
            internal
            view returns (bool, bytes memory, bytes32[] memory)[](WITHDRAW_WORDS_LENGTH);

        withdrawParsers[WITHDRAW_WORD_WITHDRAWER] = LibRaindexSubParser.subParserWithdrawer;
        withdrawParsers[WITHDRAW_WORD_TOKEN] = LibRaindexSubParser.subParserWithdrawToken;
        withdrawParsers[WITHDRAW_WORD_VAULT_ID] = LibRaindexSubParser.subParserWithdrawVaultId;
        withdrawParsers[WITHDRAW_WORD_VAULT_BEFORE] = LibRaindexSubParser.subParserWithdrawVaultBalanceBefore;
        withdrawParsers[WITHDRAW_WORD_VAULT_AFTER] = LibRaindexSubParser.subParserWithdrawVaultBalanceAfter;
        withdrawParsers[WITHDRAW_WORD_TARGET_AMOUNT] = LibRaindexSubParser.subParserWithdrawTargetAmount;
        withdrawParsers[WITHDRAW_WORD_TOKEN_DECIMALS] = LibRaindexSubParser.subParserWithdrawTokenDecimals;

        parsers[CONTEXT_WITHDRAW_COLUMN] = withdrawParsers;

        uint256[][] memory parsersUint256;
        assembly ("memory-safe") {
            parsersUint256 := parsers
        }

        return LibConvert.unsafeTo16BitBytes(parsersUint256.flatten());
    }
}
