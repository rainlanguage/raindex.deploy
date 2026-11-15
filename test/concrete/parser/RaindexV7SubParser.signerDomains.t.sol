// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {StackAllocationMismatch} from "rainlang-0.2.35/src/error/ErrIntegrity.sol";
import {ExpectedOperand, UnexpectedOperandValue} from "rainlang-0.2.35/src/error/ErrParse.sol";
import {OpTest, StackItem} from "rainlang-0.2.35/src/../test/abstract/OpTest.sol";
import {Strings} from "@openzeppelin-contracts-5.7.0/utils/Strings.sol";
import {LibRaindexSubParserContextFixture} from "test/util/fixture/LibRaindexSubParserContextFixture.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {LibEtchRaindex} from "test/util/lib/LibEtchRaindex.sol";

contract RaindexV7SubParserSignerDomainsTest is OpTest {
    using Strings for address;

    function setUp() public {
        LibEtchRaindex.etchRaindex(vm);
    }

    /// Test signer-domain-0
    function testSubParserContextSignerDomainHappy0() external view {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        StackItem[] memory expectedStack = new StackItem[](1);
        expectedStack[0] = StackItem.wrap(keccak256(bytes("signer-domain-0")));

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signer-domain<0>();"));

        checkHappy(rainlang, LibRaindexSubParserContextFixture.hashedNamesContext(), expectedStack, "signer-domain-0");
    }

    /// Test signer-domain-1
    function testSubParserContextSignerDomainHappy1() external view {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        StackItem[] memory expectedStack = new StackItem[](1);
        expectedStack[0] = StackItem.wrap(keccak256(bytes("signer-domain-1")));

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signer-domain<1>();"));

        checkHappy(rainlang, LibRaindexSubParserContextFixture.hashedNamesContext(), expectedStack, "signer-domain-1");
    }

    /// Test signer-domain without an operand errors.
    function testSubParserContextSignerDomainUnhappyNoOperand() external {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signer-domain();"));

        checkUnhappyParse(rainlang, abi.encodeWithSelector(ExpectedOperand.selector));
    }

    /// Test signer-domain with too many operands errors.
    function testSubParserContextSignerDomainUnhappyTooManyOperands() external {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signer-domain<0 1>();"));

        checkUnhappyParse(rainlang, abi.encodeWithSelector(UnexpectedOperandValue.selector));
    }

    /// Test signer-domain with an input errors.
    function testSubParserContextSignerDomainUnhappyInput() external {
        address subParserAddress = LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS;

        bytes memory rainlang =
            bytes(string.concat("using-words-from ", subParserAddress.toHexString(), " _: signer-domain<0>(0);"));

        checkUnhappyParse2(rainlang, abi.encodeWithSelector(StackAllocationMismatch.selector, 2, 1));
    }
}
