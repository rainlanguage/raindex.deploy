// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {StackAllocationMismatch} from "rainlang-0.2.35/src/error/ErrIntegrity.sol";
import {ExpectedOperand, UnexpectedOperandValue} from "rainlang-0.2.35/src/error/ErrParse.sol";
import {ContextGridOverflow} from "rainlang-0.2.35/src/error/ErrSubParse.sol";
import {OpTest, StackItem} from "rainlang-0.2.35/src/../test/abstract/OpTest.sol";
import {Strings} from "@openzeppelin-contracts-5.7.0/utils/Strings.sol";
import {LibRaindexSubParserContextFixture} from "test/util/fixture/LibRaindexSubParserContextFixture.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN} from "../../../src/lib/LibRaindex.sol";
import {LibEtchRaindex} from "test/util/lib/LibEtchRaindex.sol";

contract RaindexV7SubParserSignersTest is OpTest {
    using Strings for address;

    function setUp() public {
        LibEtchRaindex.etchRaindex(vm);
    }

    /// Test signer-0
    function testSubParserContextSignerHappy0() external view {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        StackItem[] memory expectedStack = new StackItem[](1);
        expectedStack[0] = StackItem.wrap(keccak256(bytes("signer-0")));

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signer<0>();"));

        checkHappy(rainlang, LibRaindexSubParserContextFixture.hashedNamesContext(), expectedStack, "signer-0");
    }

    /// Test signer-1
    function testSubParserContextSignerHappy1() external view {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        StackItem[] memory expectedStack = new StackItem[](1);
        expectedStack[0] = StackItem.wrap(keccak256(bytes("signer-1")));

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signer<1>();"));

        checkHappy(rainlang, LibRaindexSubParserContextFixture.hashedNamesContext(), expectedStack, "signer-1");
    }

    /// Test signer without an operand errors.
    function testSubParserContextSignerUnhappyNoOperand() external {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signer();"));

        checkUnhappyParse(rainlang, abi.encodeWithSelector(ExpectedOperand.selector));
    }

    /// Test signer with too many operands errors.
    function testSubParserContextSignerUnhappyTooManyOperands() external {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signer<0 1>();"));

        checkUnhappyParse(rainlang, abi.encodeWithSelector(UnexpectedOperandValue.selector));
    }

    /// Test signer with an input errors.
    function testSubParserContextSignerUnhappyInput() external {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signer<0>(0);"));

        checkUnhappyParse2(rainlang, abi.encodeWithSelector(StackAllocationMismatch.selector, 2, 1));
    }

    /// A signer index past the last context row reverts rather than wrapping
    /// onto another signer.
    function testSubParserContextSignerUnhappyIndexOverflow() external {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signer<256>();"));

        checkUnhappyParse(
            rainlang, abi.encodeWithSelector(ContextGridOverflow.selector, CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN, 256)
        );
    }
}
