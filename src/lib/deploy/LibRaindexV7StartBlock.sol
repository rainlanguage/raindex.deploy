// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

/// @title LibRaindexV7StartBlock
/// @notice The block the newest released RaindexV7 was deployed at on each
/// chain the subgraph indexes.
library LibRaindexV7StartBlock {
    uint256 constant RAINDEX_START_BLOCK_ARBITRUM = 518606700;
    uint256 constant RAINDEX_START_BLOCK_BASE = 53137846;
    uint256 constant RAINDEX_START_BLOCK_FLARE = 73060354;
    uint256 constant RAINDEX_START_BLOCK_POLYGON = 96239809;
}
