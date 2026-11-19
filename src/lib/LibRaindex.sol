// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.19;

import {TaskV3} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {SourceIndexV2, StateNamespace, StackItem, EvalV4} from "rainlang-0.2.39/src/interface/IInterpreterV4.sol";
import {LibNamespace, FullyQualifiedNamespace} from "rainlang-0.2.39/src/lib/ns/LibNamespace.sol";
import {LibContext} from "rainlang-0.2.39/src/lib/caller/LibContext.sol";

/// @dev Raindex context is actually fairly complex. The calling context column
/// is populated before calculate order, but the remaining columns are only
/// available to handle IO as they depend on the full evaluation of calculate
/// order, and cross referencing against the same from the counterparty, as well
/// as accounting limits such as current vault balances, etc.
/// The token address and decimals for vault inputs and outputs IS available to
/// the calculate order entrypoint, but not the final vault balances/diff.
/// Each entrypoint fills only its own columns and leaves the rest empty, so a
/// word read outside the entrypoints that fill its cell reverts out of bounds
/// instead of reading another entrypoint's value.
uint256 constant CALLING_CONTEXT_COLUMNS = 6;

uint256 constant CONTEXT_COLUMNS = CALLING_CONTEXT_COLUMNS + 1;

/// @dev Number of sub parser word tables, one per context column up to and
/// including the first signed context column.
uint256 constant CONTEXT_COLUMNS_EXTENDED = CONTEXT_SIGNED_CONTEXT_START_COLUMN + 1;

/// @dev Order column. Clear gets all rows; take and quote omit the clearer;
/// add and remove order tasks get only the order hash and owner. IMPORTANT NOTE
/// that the clearer will often be unrelated to the owner or counterparty.
uint256 constant CONTEXT_CALLING_CONTEXT_COLUMN = 1;
uint256 constant CONTEXT_CALLING_CONTEXT_ROWS = 4;

uint256 constant CONTEXT_CALLING_CONTEXT_ROW_ORDER_HASH = 0;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_ORDER_OWNER = 1;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_ORDER_COUNTERPARTY = 2;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_ORDER_CLEARER = 3;

/// @dev Deposit column. Filled only for deposit tasks.
uint256 constant CONTEXT_DEPOSIT_COLUMN = 5;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_TOKEN = 0;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_VAULT_ID = 1;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_VAULT_BEFORE = 2;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_VAULT_AFTER = 3;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_TOKEN_DECIMALS = 4;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_DEPOSIT_DEPOSITOR = 5;

/// @dev Withdraw column. Filled only for withdraw tasks.
uint256 constant CONTEXT_WITHDRAW_COLUMN = 6;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_TOKEN = 0;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_VAULT_ID = 1;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_VAULT_BEFORE = 2;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_VAULT_AFTER = 3;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_TARGET_AMOUNT = 4;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_TOKEN_DECIMALS = 5;
uint256 constant CONTEXT_CALLING_CONTEXT_ROW_WITHDRAW_WITHDRAWER = 6;

/// @dev Calculations column contains the DECIMAL RESCALED calculations but
/// otherwise provided as-is according to calculate order entrypoint
uint256 constant CONTEXT_CALCULATIONS_COLUMN = 2;
uint256 constant CONTEXT_CALCULATIONS_ROWS = 2;

uint256 constant CONTEXT_CALCULATIONS_ROW_MAX_OUTPUT = 0;
uint256 constant CONTEXT_CALCULATIONS_ROW_IO_RATIO = 1;

/// @dev Vault inputs are the literal token amounts and vault balances before and
/// after for the input token from the perspective of the order. MAY be
/// significantly different to the calculated amount due to insufficient vault
/// balances from either the owner or counterparty, etc.
uint256 constant CONTEXT_VAULT_INPUTS_COLUMN = 3;
/// @dev Vault outputs are the same as vault inputs but for the output token from
/// the perspective of the order.
uint256 constant CONTEXT_VAULT_OUTPUTS_COLUMN = 4;

/// @dev Row of the token address for vault inputs and outputs columns.
uint256 constant CONTEXT_VAULT_IO_TOKEN = 0;
/// @dev Row of the token decimals for vault inputs and outputs columns.
uint256 constant CONTEXT_VAULT_IO_TOKEN_DECIMALS = 1;
/// @dev Row of the vault ID for vault inputs and outputs columns.
uint256 constant CONTEXT_VAULT_IO_VAULT_ID = 2;
/// @dev Row of the vault balance before the order was cleared for vault inputs
/// and outputs columns.
uint256 constant CONTEXT_VAULT_IO_BALANCE_BEFORE = 3;
/// @dev Row of the vault balance difference after the order was cleared for
/// vault inputs and outputs columns. The diff is ALWAYS POSITIVE as it is a
/// `uint256` so it must be added to input balances and subtracted from output
/// balances.
uint256 constant CONTEXT_VAULT_IO_BALANCE_DIFF = 4;
/// @dev Length of a vault IO column.
uint256 constant CONTEXT_VAULT_IO_ROWS = 5;

/// @dev Signed context signer addresses occupy their own column so that each
/// signer can be matched to its corresponding signed context column.
uint256 constant CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN = CONTEXT_COLUMNS;
uint256 constant CONTEXT_SIGNED_CONTEXT_SIGNERS_ROWS = 1;
uint256 constant CONTEXT_SIGNED_CONTEXT_SIGNERS_ROW = 0;

/// @dev Signed context EIP-712 domain separators occupy their own column after
/// the signers column. Row i is the domain separator signed context i was
/// signed under, chosen by its signer.
uint256 constant CONTEXT_SIGNED_CONTEXT_DOMAINS_COLUMN = CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN + 1;
uint256 constant CONTEXT_SIGNED_CONTEXT_DOMAINS_ROWS = 1;
uint256 constant CONTEXT_SIGNED_CONTEXT_DOMAINS_ROW = 0;

/// @dev Signed context data columns start after the domains column. Each
/// signer's data is a separate column at this offset + signer index.
uint256 constant CONTEXT_SIGNED_CONTEXT_START_COLUMN = CONTEXT_SIGNED_CONTEXT_DOMAINS_COLUMN + 1;
uint256 constant CONTEXT_SIGNED_CONTEXT_START_ROWS = 1;
uint256 constant CONTEXT_SIGNED_CONTEXT_START_ROW = 0;

library LibRaindex {
    /// @dev Builds a calling context with `values` as context column `column`
    /// and every other calling context column empty.
    function callingContextColumn(uint256 column, bytes32[] memory values) internal pure returns (bytes32[][] memory) {
        bytes32[][] memory callingContext = new bytes32[][](CALLING_CONTEXT_COLUMNS);
        callingContext[column - 1] = values;
        return callingContext;
    }

    /// @dev Evaluates each task in `post` against `context` padded with empty
    /// columns to `CALLING_CONTEXT_COLUMNS`, so the signers, domains and signed
    /// context columns are at `CONTEXT_SIGNED_CONTEXT_SIGNERS_COLUMN`,
    /// `CONTEXT_SIGNED_CONTEXT_DOMAINS_COLUMN` and
    /// `CONTEXT_SIGNED_CONTEXT_START_COLUMN` as in order evaluation. Tasks with
    /// empty bytecode are skipped.
    function doPost(bytes32[][] memory context, TaskV3[] memory post) internal {
        bytes32[][] memory callingContext = new bytes32[][](CALLING_CONTEXT_COLUMNS);
        for (uint256 i = 0; i < context.length; ++i) {
            callingContext[i] = context[i];
        }
        StateNamespace namespace = StateNamespace.wrap(uint256(uint160(msg.sender)));
        FullyQualifiedNamespace qualifiedNamespace = LibNamespace.qualifyNamespace(namespace, address(this));
        TaskV3 memory task;
        StackItem[] memory emptyStack = new StackItem[](0);
        bytes32[] memory emptyStateOverlay = new bytes32[](0);
        for (uint256 i = 0; i < post.length; ++i) {
            task = post[i];
            if (task.evaluable.bytecode.length > 0) {
                //slither-disable-next-line unused-return
                (, bytes32[] memory writes) = task.evaluable.interpreter
                    .eval4(
                        EvalV4({
                            store: task.evaluable.store,
                            namespace: qualifiedNamespace,
                            bytecode: task.evaluable.bytecode,
                            sourceIndex: SourceIndexV2.wrap(0),
                            context: LibContext.buildV2(callingContext, task.signedContext),
                            inputs: emptyStack,
                            stateOverlay: emptyStateOverlay
                        })
                    );
                if (writes.length > 0) {
                    task.evaluable.store.set(namespace, writes);
                }
            }
        }
    }
}
