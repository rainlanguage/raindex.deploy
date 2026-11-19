// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7} from "src/concrete/raindex/RaindexV7.sol";

/// @dev RaindexV7 whose `nonReentrant` entry fills the memory past the free
/// memory pointer with nonzero words, so allocations inside guarded functions
/// cannot rely on that memory being zero.
contract RaindexV7DirtyFreeMemoryHarness is RaindexV7 {
    uint256 internal constant DIRTY_WORDS = 0x100;
    uint256 internal constant DIRTY_WORD = 1;

    function _reentrancyGuardStorageSlot() internal pure override returns (bytes32) {
        assembly ("memory-safe") {
            let start := mload(0x40)
            let end := add(start, mul(DIRTY_WORDS, 0x20))
            for { let cursor := start } lt(cursor, end) { cursor := add(cursor, 0x20) } { mstore(cursor, DIRTY_WORD) }
        }
        return super._reentrancyGuardStorageSlot();
    }
}
