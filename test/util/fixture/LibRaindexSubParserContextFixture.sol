// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {
    WORD_SENDER,
    WORD_RAINDEX,
    WORD_ORDER_CLEARER,
    WORD_ORDER_HASH,
    WORD_ORDER_OWNER,
    WORD_ORDER_COUNTERPARTY,
    CONTEXT_CALLING_CONTEXT_ROWS,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_COUNTERPARTY,
    CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER,
    CONTEXT_CALLING_CONTEXT_COLUMN,
    CONTEXT_CALCULATIONS_COLUMN,
    CONTEXT_CALCULATIONS_ROWS,
    CONTEXT_CALCULATIONS_ROW_MAX_OUTPUT,
    CONTEXT_CALCULATIONS_ROW_IO_RATIO,
    WORD_CALCULATED_MAX_OUTPUT,
    WORD_CALCULATED_IO_RATIO,
    CONTEXT_VAULT_IO_ROWS,
    CONTEXT_VAULT_INPUTS_COLUMN,
    CONTEXT_VAULT_OUTPUTS_COLUMN,
    CONTEXT_VAULT_IO_TOKEN,
    CONTEXT_VAULT_IO_TOKEN_DECIMALS,
    CONTEXT_VAULT_IO_VAULT_ID,
    CONTEXT_VAULT_IO_BALANCE_BEFORE,
    CONTEXT_VAULT_IO_BALANCE_DIFF,
    WORD_INPUT_TOKEN,
    WORD_INPUT_TOKEN_DECIMALS,
    WORD_INPUT_VAULT_ID,
    WORD_INPUT_VAULT_BALANCE_BEFORE,
    WORD_INPUT_VAULT_BALANCE_INCREASE,
    WORD_OUTPUT_TOKEN,
    WORD_OUTPUT_TOKEN_DECIMALS,
    WORD_OUTPUT_VAULT_ID,
    WORD_OUTPUT_VAULT_BALANCE_BEFORE,
    WORD_OUTPUT_VAULT_BALANCE_DECREASE,
    CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN,
    CONTEXT_SIGNED_CONTEXT_DOMAINS_COLUMN,
    CONTEXT_SIGNED_CONTEXT_START_COLUMN
} from "../../../src/lib/LibRaindexSubParser.sol";
import {
    CONTEXT_BASE_COLUMN,
    CONTEXT_BASE_ROWS,
    CONTEXT_BASE_ROW_SENDER,
    CONTEXT_BASE_ROW_CALLING_CONTRACT
} from "rainlang-0.2.35/src/lib/caller/LibContext.sol";

library LibRaindexSubParserContextFixture {
    function hashedNamesContext() internal pure returns (bytes32[][] memory) {
        bytes32[][] memory context = new bytes32[][](CONTEXT_SIGNED_CONTEXT_START_COLUMN + 2);

        bytes32[] memory contextBase = new bytes32[](CONTEXT_BASE_ROWS);
        contextBase[CONTEXT_BASE_ROW_SENDER] = keccak256(WORD_SENDER);
        contextBase[CONTEXT_BASE_ROW_CALLING_CONTRACT] = keccak256(WORD_RAINDEX);

        bytes32[] memory contextCallingContext = new bytes32[](CONTEXT_CALLING_CONTEXT_ROWS);
        contextCallingContext[CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH] = keccak256(WORD_ORDER_HASH);
        contextCallingContext[CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER] = keccak256(WORD_ORDER_OWNER);
        contextCallingContext[CONTEXT_CALLING_CONTEXT_ROW_ORDER_COUNTERPARTY] = keccak256(WORD_ORDER_COUNTERPARTY);
        contextCallingContext[CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER] = keccak256(WORD_ORDER_CLEARER);

        bytes32[] memory contextCalculations = new bytes32[](CONTEXT_CALCULATIONS_ROWS);
        contextCalculations[CONTEXT_CALCULATIONS_ROW_MAX_OUTPUT] = keccak256(WORD_CALCULATED_MAX_OUTPUT);
        contextCalculations[CONTEXT_CALCULATIONS_ROW_IO_RATIO] = keccak256(WORD_CALCULATED_IO_RATIO);

        bytes32[] memory contextVaultInputs = new bytes32[](CONTEXT_VAULT_IO_ROWS);
        contextVaultInputs[CONTEXT_VAULT_IO_TOKEN] = keccak256(WORD_INPUT_TOKEN);
        contextVaultInputs[CONTEXT_VAULT_IO_TOKEN_DECIMALS] = keccak256(WORD_INPUT_TOKEN_DECIMALS);
        contextVaultInputs[CONTEXT_VAULT_IO_VAULT_ID] = keccak256(WORD_INPUT_VAULT_ID);
        contextVaultInputs[CONTEXT_VAULT_IO_BALANCE_BEFORE] = keccak256(WORD_INPUT_VAULT_BALANCE_BEFORE);
        contextVaultInputs[CONTEXT_VAULT_IO_BALANCE_DIFF] = keccak256(WORD_INPUT_VAULT_BALANCE_INCREASE);

        bytes32[] memory contextVaultOutputs = new bytes32[](CONTEXT_VAULT_IO_ROWS);
        contextVaultOutputs[CONTEXT_VAULT_IO_TOKEN] = keccak256(WORD_OUTPUT_TOKEN);
        contextVaultOutputs[CONTEXT_VAULT_IO_TOKEN_DECIMALS] = keccak256(WORD_OUTPUT_TOKEN_DECIMALS);
        contextVaultOutputs[CONTEXT_VAULT_IO_VAULT_ID] = keccak256(WORD_OUTPUT_VAULT_ID);
        contextVaultOutputs[CONTEXT_VAULT_IO_BALANCE_BEFORE] = keccak256(WORD_OUTPUT_VAULT_BALANCE_BEFORE);
        contextVaultOutputs[CONTEXT_VAULT_IO_BALANCE_DIFF] = keccak256(WORD_OUTPUT_VAULT_BALANCE_DECREASE);

        bytes32[] memory contextSigners = new bytes32[](2);
        contextSigners[0] = keccak256("signer-0");
        contextSigners[1] = keccak256("signer-1");

        bytes32[] memory contextSignerDomains = new bytes32[](2);
        contextSignerDomains[0] = keccak256("signer-domain-0");
        contextSignerDomains[1] = keccak256("signer-domain-1");

        bytes32[] memory contextSignedContext0 = new bytes32[](2);
        contextSignedContext0[0] = keccak256("signed-context-0-0");
        contextSignedContext0[1] = keccak256("signed-context-0-1");

        bytes32[] memory contextSignedContext1 = new bytes32[](2);
        contextSignedContext1[0] = keccak256("signed-context-1-0");
        contextSignedContext1[1] = keccak256("signed-context-1-1");

        context[CONTEXT_BASE_COLUMN] = contextBase;
        context[CONTEXT_CALLING_CONTEXT_COLUMN] = contextCallingContext;
        context[CONTEXT_CALCULATIONS_COLUMN] = contextCalculations;
        context[CONTEXT_VAULT_INPUTS_COLUMN] = contextVaultInputs;
        context[CONTEXT_VAULT_OUTPUTS_COLUMN] = contextVaultOutputs;
        context[CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN] = contextSigners;
        context[CONTEXT_SIGNED_CONTEXT_DOMAINS_COLUMN] = contextSignerDomains;
        context[CONTEXT_SIGNED_CONTEXT_START_COLUMN] = contextSignedContext0;
        context[CONTEXT_SIGNED_CONTEXT_START_COLUMN + 1] = contextSignedContext1;

        return context;
    }
}
