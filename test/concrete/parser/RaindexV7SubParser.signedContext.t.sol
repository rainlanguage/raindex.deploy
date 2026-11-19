// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {StackAllocationMismatch} from "rainlang-0.2.39/src/error/ErrIntegrity.sol";
import {ExpectedOperand, UnexpectedOperandValue} from "rainlang-0.2.39/src/error/ErrParse.sol";
import {Strings} from "@openzeppelin-contracts-5.7.0/utils/Strings.sol";
import {OpTest, StackItem} from "rainlang-0.2.39/src/../test/abstract/OpTest.sol";
import {LibRaindexSubParserContextFixture} from "test/util/fixture/LibRaindexSubParserContextFixture.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {LibEtchRaindex} from "test/util/lib/LibEtchRaindex.sol";

contract RaindexV7SubParserSignedContextTest is OpTest {
    using Strings for address;

    function setUp() public {
        LibEtchRaindex.etchRaindex(vm);
    }

    /// Test signed-context-0-0
    function testSubParserContextSignedContextHappy0() external view {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        StackItem[] memory expectedStack = new StackItem[](1);
        expectedStack[0] = StackItem.wrap(keccak256(bytes("signed-context-0-0")));

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signed-context<0 0>();"));

        checkHappy(
            rainlang, LibRaindexSubParserContextFixture.hashedNamesContext(), expectedStack, "signed-context-0-0"
        );
    }

    /// Test signed-context-0-1
    function testSubParserContextSignedContextHappy1() external view {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        StackItem[] memory expectedStack = new StackItem[](1);
        expectedStack[0] = StackItem.wrap(keccak256(bytes("signed-context-0-1")));

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signed-context<0 1>();"));

        checkHappy(
            rainlang, LibRaindexSubParserContextFixture.hashedNamesContext(), expectedStack, "signed-context-0-1"
        );
    }

    /// Test signed-context-1-0
    function testSubParserContextSignedContextHappy2() external view {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        StackItem[] memory expectedStack = new StackItem[](1);
        expectedStack[0] = StackItem.wrap(keccak256(bytes("signed-context-1-0")));

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signed-context<1 0>();"));

        checkHappy(
            rainlang, LibRaindexSubParserContextFixture.hashedNamesContext(), expectedStack, "signed-context-1-0"
        );
    }

    /// Test signed-context-1-1
    function testSubParserContextSignedContextHappy3() external view {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        StackItem[] memory expectedStack = new StackItem[](1);
        expectedStack[0] = StackItem.wrap(keccak256(bytes("signed-context-1-1")));

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signed-context<1 1>();"));

        checkHappy(
            rainlang, LibRaindexSubParserContextFixture.hashedNamesContext(), expectedStack, "signed-context-1-1"
        );
    }

    /// Test signed-context without an operand errors.
    function testSubParserContextSignedContextUnhappyNoOperand() external {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signed-context();"));

        checkUnhappyParse(rainlang, abi.encodeWithSelector(ExpectedOperand.selector));
    }

    /// Test signed-context with too many operands errors.
    function testSubParserContextSignedContextUnhappyTooManyOperands() external {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signed-context<0 0 0>();"));

        checkUnhappyParse(rainlang, abi.encodeWithSelector(UnexpectedOperandValue.selector));
    }

    /// Test signed-context with an input errors.
    function testSubParserContextSignedContextUnhappyInput() external {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signed-context<0 0>(0);"));

        checkUnhappyParse2(rainlang, abi.encodeWithSelector(StackAllocationMismatch.selector, 2, 1));
    }
}
