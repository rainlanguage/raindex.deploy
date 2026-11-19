// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {
    LibRaindexSubParser,
    AuthoringMetaV2,
    EXTERN_PARSE_META_BUILD_DEPTH
} from "../../../src/lib/LibRaindexSubParser.sol";
import {
    SUB_PARSER_PARSE_META,
    SUB_PARSER_WORD_PARSERS,
    SUB_PARSER_OPERAND_HANDLERS,
    RaindexV7SubParser
} from "../../../src/concrete/parser/RaindexV7SubParser.sol";
import {LibGenParseMeta} from "rainlang-0.2.40/src/lib/codegen/LibGenParseMeta.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {LibEtchRaindex} from "test/util/lib/LibEtchRaindex.sol";

contract RaindexV7SubParserPointersTest is Test {
    function setUp() public {
        LibEtchRaindex.etchRaindex(vm);
    }

    function testSubParserParseMeta() external pure {
        bytes memory authoringMetaBytes = LibRaindexSubParser.authoringMetaV2();
        AuthoringMetaV2[] memory authoringMeta = abi.decode(authoringMetaBytes, (AuthoringMetaV2[]));
        bytes memory expected = LibGenParseMeta.buildParseMetaV2(authoringMeta, EXTERN_PARSE_META_BUILD_DEPTH);
        bytes memory actual = SUB_PARSER_PARSE_META;
        assertEq(actual, expected);
    }

    /// `authoringMetaV2` is the single source of truth for every context word's
    /// name AND its human-readable description. The committed authoring meta
    /// file is produced verbatim from `authoringMetaV2()` (see
    /// `script/BuildAuthoringMeta.sol`), so the function's full byte output must
    /// equal that file. The parse-meta path above only encodes the words, so it
    /// is blind to the descriptions; pinning the raw bytes here additionally
    /// binds every description string and the ordering of all 30 entries.
    function testSubParserAuthoringMetaV2MatchesFile() external view {
        bytes memory expected = vm.readFileBinary("meta/RaindexV7SubParserAuthoringMeta.rain.meta");
        bytes memory actual = LibRaindexSubParser.authoringMetaV2();
        assertEq(actual, expected);
    }

    function testSubParserFunctionPointers() external pure {
        RaindexV7SubParser extern = RaindexV7SubParser(LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS);
        bytes memory expected = extern.buildSubParserWordParsers();
        bytes memory actual = SUB_PARSER_WORD_PARSERS;
        assertEq(actual, expected);
    }

    function testSubParserOperandParsers() external pure {
        RaindexV7SubParser extern = RaindexV7SubParser(LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS);
        bytes memory expected = extern.buildOperandHandlerFunctionPointers();
        bytes memory actual = SUB_PARSER_OPERAND_HANDLERS;
        assertEq(actual, expected);
    }

    function testWordOperandLengthEquivalence() external pure {
        assertEq(SUB_PARSER_WORD_PARSERS.length, SUB_PARSER_OPERAND_HANDLERS.length);
        assertEq(
            SUB_PARSER_PARSE_META.length,
            // 4 bytes per word + 32 byte expansion + 1 byte seed + 1 byte depth
            (SUB_PARSER_WORD_PARSERS.length * 2) + 32 + 1 + 1
        );
    }
}
