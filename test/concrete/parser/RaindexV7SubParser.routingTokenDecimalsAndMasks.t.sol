// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {OperandV2} from "rainlang-0.2.35/src/lib/parse/LibParse.sol";
import {OPCODE_CONTEXT} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {ContextGridOverflow} from "rainlang-0.2.35/src/error/ErrSubParse.sol";
import {LibRaindexSubParser} from "../../../src/lib/LibRaindexSubParser.sol";
import {CONTEXT_SIGNED_CONTEXT_START_COLUMN} from "../../../src/lib/LibRaindex.sol";

/// @title RaindexV7SubParserRoutingTokenDecimalsAndMasksTest
/// @notice Pins the absolute (column, row) of the deposit and withdraw token
/// decimals words, and that a signed-context row operand wider than a byte
/// reverts instead of wrapping.
contract RaindexV7SubParserRoutingTokenDecimalsAndMasksTest is Test {
    /// @dev Decode the 4-byte context bytecode blob returned by a parser:
    /// byte 0 = opcode index, byte 1 = io byte, byte 2 = row, byte 3 = column.
    function decode(bytes memory bytecode) internal pure returns (uint8 row, uint8 column) {
        assertEq(bytecode.length, 4, "context bytecode must be 4 bytes");
        assertEq(uint8(bytecode[0]), OPCODE_CONTEXT, "opcode index");
        assertEq(uint8(bytecode[1]), 0x10, "io byte (0 in, 1 out)");
        row = uint8(bytecode[2]);
        column = uint8(bytecode[3]);
    }

    /// withdraw-token-decimals routes to the withdraw column (6), row 5.
    /// A mutation of either the column or the row constant lands a different
    /// byte in the decoded bytecode and fails here.
    function testRouteWithdrawTokenDecimalsAbsolute() external pure {
        (bool success, bytes memory bytecode, bytes32[] memory constants) =
            LibRaindexSubParser.subParserWithdrawTokenDecimals(0, 0, OperandV2.wrap(0));
        assertTrue(success, "parser must succeed");
        assertEq(constants.length, 0, "context has no constants");
        (uint8 row, uint8 column) = decode(bytecode);
        assertEq(column, 6, "withdraw-token-decimals column is withdraw (6)");
        assertEq(row, 5, "withdraw-token-decimals row is 5");
    }

    /// deposit-token-decimals routes to the deposit column (5), row 4.
    /// The wiring suite pins this via pointer equality to withdraw-target-amount;
    /// this is the direct (column, row) assertion as a self-contained anchor.
    function testRouteDepositTokenDecimalsAbsolute() external pure {
        (bool success, bytes memory bytecode,) =
            LibRaindexSubParser.subParserDepositTokenDecimals(0, 0, OperandV2.wrap(0));
        assertTrue(success, "parser must succeed");
        (uint8 row, uint8 column) = decode(bytecode);
        assertEq(column, 5, "deposit-token-decimals column is deposit (5)");
        assertEq(row, 4, "deposit-token-decimals row is 4");
    }

    /// A signed-context row wider than a byte reverts rather than wrapping onto
    /// another row.
    function testSignedContextWideRowReverts() external {
        vm.expectRevert(abi.encodeWithSelector(ContextGridOverflow.selector, CONTEXT_SIGNED_CONTEXT_START_COLUMN, 256));
        this.callSignedContext(OperandV2.wrap(bytes32(uint256(0x010000))));
    }

    /// @dev External wrapper so `vm.expectRevert` can catch the library revert.
    function callSignedContext(OperandV2 operand) external pure returns (bool, bytes memory, bytes32[] memory) {
        return LibRaindexSubParser.subParserSignedContext(0, 0, operand);
    }
}
