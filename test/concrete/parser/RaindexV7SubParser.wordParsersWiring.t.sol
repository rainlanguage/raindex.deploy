// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {RaindexV7SubParser} from "../../../src/concrete/parser/RaindexV7SubParser.sol";
import {DEPOSIT_WORDS_LENGTH, WITHDRAW_WORDS_LENGTH} from "../../../src/lib/LibRaindexSubParser.sol";
import {
    CONTEXT_CALLING_CONTEXT_ROWS,
    CONTEXT_CALCULATIONS_ROWS,
    CONTEXT_VAULT_IO_ROWS,
    CONTEXT_SIGNED_CONTEXT_SIGNERS_ROWS,
    CONTEXT_SIGNED_CONTEXT_DOMAINS_ROWS,
    CONTEXT_SIGNED_CONTEXT_START_ROWS
} from "../../../src/lib/LibRaindex.sol";
import {CONTEXT_BASE_ROWS} from "rainlang-0.2.36/src/lib/caller/LibContext.sol";

/// @title RaindexV7SubParserWordParsersWiringTest
/// @notice Every word reads its own context cell, so no two word parsers can
/// be the same function: the flat 16-bit pointer table holds one slot per word
/// and no two slots share a pointer.
contract RaindexV7SubParserWordParsersWiringTest is Test {
    function wordCount() internal pure returns (uint256) {
        return CONTEXT_BASE_ROWS + CONTEXT_CALLING_CONTEXT_ROWS + CONTEXT_CALCULATIONS_ROWS + CONTEXT_VAULT_IO_ROWS * 2
            + DEPOSIT_WORDS_LENGTH + WITHDRAW_WORDS_LENGTH + CONTEXT_SIGNED_CONTEXT_SIGNERS_ROWS
            + CONTEXT_SIGNED_CONTEXT_DOMAINS_ROWS + CONTEXT_SIGNED_CONTEXT_START_ROWS;
    }

    function testWordParsersTableHasOneSlotPerWord() external {
        assertEq(new RaindexV7SubParser().buildSubParserWordParsers().length, wordCount() * 2);
    }

    function testOperandHandlersTableHasOneSlotPerWord() external {
        assertEq(new RaindexV7SubParser().buildOperandHandlerFunctionPointers().length, wordCount() * 2);
    }

    function testWordParsersPairwiseDistinct() external {
        bytes memory table = new RaindexV7SubParser().buildSubParserWordParsers();
        for (uint256 i = 0; i < table.length; i += 2) {
            for (uint256 j = i + 2; j < table.length; j += 2) {
                assertTrue(table[i] != table[j] || table[i + 1] != table[j + 1], "two words share a parser");
            }
        }
    }
}
