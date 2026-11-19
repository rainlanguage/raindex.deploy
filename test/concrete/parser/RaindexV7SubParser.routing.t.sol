// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {OperandV2} from "rainlang-0.2.39/src/lib/parse/LibParse.sol";
import {OPCODE_CONTEXT} from "rainlang-0.2.39/src/interface/IInterpreterV4.sol";
import {LibRaindexSubParser} from "../../../src/lib/LibRaindexSubParser.sol";

/// @title RaindexV7SubParserRoutingTest
/// @notice Pins the exact (column, row) routing of every context word parser in
/// `LibRaindexSubParser`. Each `subParser*` function returns the bytecode blob
/// produced by `LibSubParse.subParserContext(column, row)`, whose 4 bytes are
/// `[opIndex, ioByte, row, column]`. By decoding bytes 2 and 3 we assert the
/// column AND row independently, so a swap of either constant in the source is
/// caught here directly (no etched pointers / regen required). This complements
/// the per-word happy tests (which run the etched parser and assert the keccak
/// of the resolved context value) by isolating the col-vs-row decision and by
/// covering the deposit/withdraw words that have no fixture context columns.
contract RaindexV7SubParserRoutingTest is Test {
    /// @dev Assert that a parser function routes to the exact (column, row).
    /// Decodes the 4-byte context bytecode blob returned by the parser:
    /// byte 0 = opcode index, byte 1 = io byte (1 output), byte 2 = row,
    /// byte 3 = column.
    function checkRoute(
        function(uint256, uint256, OperandV2) internal pure returns (bool, bytes memory, bytes32[] memory) parser,
        uint8 expectedColumn,
        uint8 expectedRow
    ) internal pure {
        (bool success, bytes memory bytecode, bytes32[] memory constants) = parser(0, 0, OperandV2.wrap(0));
        assertTrue(success, "parser must succeed");
        assertEq(bytecode.length, 4, "context bytecode must be 4 bytes");
        assertEq(constants.length, 0, "context has no constants");
        assertEq(uint8(bytecode[0]), OPCODE_CONTEXT, "opcode index");
        // 0 inputs, 1 output is encoded as the high nibble of byte 1.
        assertEq(uint8(bytecode[1]), 0x10, "io byte (0 in, 1 out)");
        assertEq(uint8(bytecode[2]), expectedRow, "row");
        assertEq(uint8(bytecode[3]), expectedColumn, "column");
    }

    /// @dev Same as `checkRoute` but the parser consumes the operand to select
    /// the row/column, so an explicit operand is supplied.
    function checkRouteOperand(
        function(uint256, uint256, OperandV2) internal pure returns (bool, bytes memory, bytes32[] memory) parser,
        OperandV2 operand,
        uint8 expectedColumn,
        uint8 expectedRow
    ) internal pure {
        (bool success, bytes memory bytecode,) = parser(0, 0, operand);
        assertTrue(success, "parser must succeed");
        assertEq(bytecode.length, 4, "context bytecode must be 4 bytes");
        assertEq(uint8(bytecode[2]), expectedRow, "row");
        assertEq(uint8(bytecode[3]), expectedColumn, "column");
    }

    // Base column (column 0).

    function testRouteSender() external pure {
        checkRoute(LibRaindexSubParser.subParserSender, 0, 0);
    }

    function testRouteCallingContract() external pure {
        // raindex -> base column 0, calling-contract row 1.
        checkRoute(LibRaindexSubParser.subParserCallingContract, 0, 1);
    }

    // Order column (column 1).

    function testRouteOrderHash() external pure {
        checkRoute(LibRaindexSubParser.subParserOrderHash, 1, 0);
    }

    function testRouteOrderOwner() external pure {
        checkRoute(LibRaindexSubParser.subParserOrderOwner, 1, 1);
    }

    function testRouteOrderCounterparty() external pure {
        checkRoute(LibRaindexSubParser.subParserOrderCounterparty, 1, 2);
    }

    function testRouteOrderClearer() external pure {
        checkRoute(LibRaindexSubParser.subParserOrderClearer, 1, 3);
    }

    // Calculations column (column 2).

    function testRouteMaxOutput() external pure {
        checkRoute(LibRaindexSubParser.subParserMaxOutput, 2, 0);
    }

    function testRouteIORatio() external pure {
        checkRoute(LibRaindexSubParser.subParserIORatio, 2, 1);
    }

    // Vault inputs column (column 3).

    function testRouteInputToken() external pure {
        checkRoute(LibRaindexSubParser.subParserInputToken, 3, 0);
    }

    function testRouteInputTokenDecimals() external pure {
        checkRoute(LibRaindexSubParser.subParserInputTokenDecimals, 3, 1);
    }

    function testRouteInputVaultId() external pure {
        checkRoute(LibRaindexSubParser.subParserInputVaultId, 3, 2);
    }

    function testRouteInputBalanceBefore() external pure {
        checkRoute(LibRaindexSubParser.subParserInputBalanceBefore, 3, 3);
    }

    function testRouteInputBalanceDiff() external pure {
        checkRoute(LibRaindexSubParser.subParserInputBalanceDiff, 3, 4);
    }

    // Vault outputs column (column 4) — same rows as inputs but different
    // column, so this pins input-vs-output column isolation.

    function testRouteOutputToken() external pure {
        checkRoute(LibRaindexSubParser.subParserOutputToken, 4, 0);
    }

    function testRouteOutputTokenDecimals() external pure {
        checkRoute(LibRaindexSubParser.subParserOutputTokenDecimals, 4, 1);
    }

    function testRouteOutputVaultId() external pure {
        checkRoute(LibRaindexSubParser.subParserOutputVaultId, 4, 2);
    }

    function testRouteOutputBalanceBefore() external pure {
        checkRoute(LibRaindexSubParser.subParserOutputBalanceBefore, 4, 3);
    }

    function testRouteOutputBalanceDiff() external pure {
        checkRoute(LibRaindexSubParser.subParserOutputBalanceDiff, 4, 4);
    }

    // Deposit column (column 5).

    function testRouteDepositor() external pure {
        checkRoute(LibRaindexSubParser.subParserDepositor, 5, 5);
    }

    function testRouteDepositToken() external pure {
        checkRoute(LibRaindexSubParser.subParserDepositToken, 5, 0);
    }

    function testRouteDepositVaultId() external pure {
        checkRoute(LibRaindexSubParser.subParserDepositVaultId, 5, 1);
    }

    function testRouteDepositVaultBalanceBefore() external pure {
        checkRoute(LibRaindexSubParser.subParserDepositVaultBalanceBefore, 5, 2);
    }

    function testRouteDepositVaultBalanceAfter() external pure {
        checkRoute(LibRaindexSubParser.subParserDepositVaultBalanceAfter, 5, 3);
    }

    // Withdraw column (column 6).

    function testRouteWithdrawer() external pure {
        checkRoute(LibRaindexSubParser.subParserWithdrawer, 6, 6);
    }

    function testRouteWithdrawToken() external pure {
        checkRoute(LibRaindexSubParser.subParserWithdrawToken, 6, 0);
    }

    function testRouteWithdrawVaultId() external pure {
        checkRoute(LibRaindexSubParser.subParserWithdrawVaultId, 6, 1);
    }

    function testRouteWithdrawVaultBalanceBefore() external pure {
        checkRoute(LibRaindexSubParser.subParserWithdrawVaultBalanceBefore, 6, 2);
    }

    function testRouteWithdrawVaultBalanceAfter() external pure {
        checkRoute(LibRaindexSubParser.subParserWithdrawVaultBalanceAfter, 6, 3);
    }

    function testRouteWithdrawTargetAmount() external pure {
        checkRoute(LibRaindexSubParser.subParserWithdrawTargetAmount, 6, 4);
    }

    // Operand-driven words.

    /// signers routes to the signers column (column 7) and uses the raw
    /// operand as the row.
    function testRouteSignersRowFromOperand() external pure {
        checkRouteOperand(LibRaindexSubParser.subParserSigners, OperandV2.wrap(bytes32(uint256(0))), 7, 0);
        checkRouteOperand(LibRaindexSubParser.subParserSigners, OperandV2.wrap(bytes32(uint256(1))), 7, 1);
        checkRouteOperand(LibRaindexSubParser.subParserSigners, OperandV2.wrap(bytes32(uint256(7))), 7, 7);
    }

    /// signer-domain routes to the signer domains column (column 8) and uses
    /// the raw operand as the row.
    function testRouteSignerDomainsRowFromOperand() external pure {
        checkRouteOperand(LibRaindexSubParser.subParserSignerDomains, OperandV2.wrap(bytes32(uint256(0))), 8, 0);
        checkRouteOperand(LibRaindexSubParser.subParserSignerDomains, OperandV2.wrap(bytes32(uint256(1))), 8, 1);
        checkRouteOperand(LibRaindexSubParser.subParserSignerDomains, OperandV2.wrap(bytes32(uint256(7))), 8, 7);
    }

    /// signed-context low operand byte selects a column offset added to the
    /// signed context start column (column 9); the second byte selects the row.
    function testRouteSignedContextColumnMaskAndRowShift() external pure {
        // operand 0x0000 -> column 9 + 0, row 0.
        checkRouteOperand(LibRaindexSubParser.subParserSignedContext, OperandV2.wrap(bytes32(uint256(0x0000))), 9, 0);
        // operand 0x0001 -> low byte 1 -> column 9 + 1 = 10, row 0.
        checkRouteOperand(LibRaindexSubParser.subParserSignedContext, OperandV2.wrap(bytes32(uint256(0x0001))), 10, 0);
        // operand 0x0100 -> low byte 0 -> column 9, second byte 1 -> row 1.
        checkRouteOperand(LibRaindexSubParser.subParserSignedContext, OperandV2.wrap(bytes32(uint256(0x0100))), 9, 1);
        // operand 0x0201 -> column 9 + 1 = 10, row 2. Proves column and row are
        // taken from distinct operand bytes (mask vs shift not swapped).
        checkRouteOperand(LibRaindexSubParser.subParserSignedContext, OperandV2.wrap(bytes32(uint256(0x0201))), 10, 2);
    }
}
