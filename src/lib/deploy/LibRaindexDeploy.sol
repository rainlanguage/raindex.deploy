// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {BYTECODE_HASH as RAINDEX_HASH, DEPLOYED_ADDRESS as RAINDEX_ADDR} from "../../generated/candidate/RaindexV7.sol";
import {
    BYTECODE_HASH as SUB_PARSER_HASH,
    DEPLOYED_ADDRESS as SUB_PARSER_ADDR
} from "../../generated/candidate/RaindexV7SubParser.sol";
import {
    BYTECODE_HASH as ROUTE_PROCESSOR_HASH,
    DEPLOYED_ADDRESS as ROUTE_PROCESSOR_ADDR
} from "../../generated/candidate/RouteProcessor4.sol";
import {
    BYTECODE_HASH as GENERIC_POOL_ARB_OT_HASH,
    DEPLOYED_ADDRESS as GENERIC_POOL_ARB_OT_ADDR
} from "../../generated/candidate/GenericPoolRaindexV7ArbOrderTaker.sol";
import {
    BYTECODE_HASH as RP_ARB_OT_HASH,
    DEPLOYED_ADDRESS as RP_ARB_OT_ADDR
} from "../../generated/candidate/RouteProcessorRaindexV7ArbOrderTaker.sol";
import {
    BYTECODE_HASH as GENERIC_POOL_FB_HASH,
    DEPLOYED_ADDRESS as GENERIC_POOL_FB_ADDR
} from "../../generated/candidate/GenericPoolRaindexV7FlashBorrower.sol";

/// @title LibRaindexDeploy
/// @notice A library containing the deployed address and code hash of the
/// Raindex contracts when deployed with the rain standard zoltu deployer.
/// This allows idempotent deployments against precommitted addresses and hashes
/// that can be easily verified automatically in tests and scripts rather than
/// relying on registries or manual verification.
library LibRaindexDeploy {
    /// The address of the `RaindexV7` contract when deployed with the rain
    /// standard zoltu deployer.
    address constant RAINDEX_DEPLOYED_ADDRESS = RAINDEX_ADDR;

    /// The code hash of the `RaindexV7` contract when deployed with the rain
    /// standard zoltu deployer.
    bytes32 constant RAINDEX_DEPLOYED_CODEHASH = RAINDEX_HASH;

    /// The address of the `RaindexV7SubParser` contract when deployed with
    /// the rain standard zoltu deployer.
    address constant SUB_PARSER_DEPLOYED_ADDRESS = SUB_PARSER_ADDR;

    /// The code hash of the `RaindexV7SubParser` contract when deployed with
    /// the rain standard zoltu deployer.
    bytes32 constant SUB_PARSER_DEPLOYED_CODEHASH = SUB_PARSER_HASH;

    /// The address of the `RouteProcessor4` contract when deployed with the
    /// rain standard zoltu deployer.
    address constant ROUTE_PROCESSOR_DEPLOYED_ADDRESS = ROUTE_PROCESSOR_ADDR;

    /// The code hash of the `RouteProcessor4` contract when deployed with the
    /// rain standard zoltu deployer.
    bytes32 constant ROUTE_PROCESSOR_DEPLOYED_CODEHASH = ROUTE_PROCESSOR_HASH;

    /// The address of the `GenericPoolRaindexV7ArbOrderTaker` contract when
    /// deployed with the rain standard zoltu deployer.
    address constant GENERIC_POOL_ARB_ORDER_TAKER_DEPLOYED_ADDRESS = GENERIC_POOL_ARB_OT_ADDR;

    /// The code hash of the `GenericPoolRaindexV7ArbOrderTaker` contract when
    /// deployed with the rain standard zoltu deployer.
    bytes32 constant GENERIC_POOL_ARB_ORDER_TAKER_DEPLOYED_CODEHASH = GENERIC_POOL_ARB_OT_HASH;

    /// The address of the `RouteProcessorRaindexV7ArbOrderTaker` contract
    /// when deployed with the rain standard zoltu deployer.
    address constant ROUTE_PROCESSOR_ARB_ORDER_TAKER_DEPLOYED_ADDRESS = RP_ARB_OT_ADDR;

    /// The code hash of the `RouteProcessorRaindexV7ArbOrderTaker` contract
    /// when deployed with the rain standard zoltu deployer.
    bytes32 constant ROUTE_PROCESSOR_ARB_ORDER_TAKER_DEPLOYED_CODEHASH = RP_ARB_OT_HASH;

    /// The address of the `GenericPoolRaindexV7FlashBorrower` contract when
    /// deployed with the rain standard zoltu deployer.
    address constant GENERIC_POOL_FLASH_BORROWER_DEPLOYED_ADDRESS = GENERIC_POOL_FB_ADDR;

    /// The code hash of the `GenericPoolRaindexV7FlashBorrower` contract when
    /// deployed with the rain standard zoltu deployer.
    bytes32 constant GENERIC_POOL_FLASH_BORROWER_DEPLOYED_CODEHASH = GENERIC_POOL_FB_HASH;
}
