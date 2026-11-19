// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {ROUTE_PROCESSOR_4_CREATION_CODE} from "../../../src/lib/deploy/LibRouteProcessor4CreationCode.sol";

/// @dev Known codehash of the RouteProcessor4 runtime bytecode, verified
/// against the Sushi deployment on Ethereum mainnet.
bytes32 constant KNOWN_ROUTE_PROCESSOR_4_CODEHASH =
    bytes32(0xeb3745a79c6ba48e8767b9c355b8e7b79f9d6edeca004e4bb91be4de515a7eeb);

/// @dev Known keccak256 of the exact RouteProcessor4 creation code bytes taken
/// from the Sushi deployment. This pins the full creation-code identity, not
/// just the runtime result, so that a change to any creation-code byte — even
/// one in init-only code that is never executed on deployment (e.g. the
/// privileged-user loop body, dead when the constructor's user array is empty)
/// and therefore leaves the deployed runtime bytecode unchanged — is caught.
bytes32 constant KNOWN_ROUTE_PROCESSOR_4_CREATION_CODEHASH =
    bytes32(0x79d8a59bafcb6b1d413ed7ed81896ecbd18ceacf3da75541e8dff26990afcbb1);

/// @title LibRouteProcessor4CreationCodeTest
/// @notice Deploys RouteProcessor4 from the stored creation code and verifies
/// that the resulting runtime bytecode hash matches the known Sushi deployment.
contract LibRouteProcessor4CreationCodeTest is Test {
    /// The stored creation code bytes MUST hash to the known creation-code hash
    /// of the Sushi RouteProcessor4 deployment. This anchors the full creation
    /// code, catching any byte change — including init-only code that does not
    /// alter the deployed runtime and so slips past the runtime codehash check.
    function testRouteProcessor4CreationCodehash() external pure {
        assertEq(keccak256(ROUTE_PROCESSOR_4_CREATION_CODE), KNOWN_ROUTE_PROCESSOR_4_CREATION_CODEHASH);
    }

    /// Deploying the stored creation code MUST produce runtime bytecode whose
    /// hash matches the known Sushi RouteProcessor4 codehash.
    function testRouteProcessor4Codehash() external {
        bytes memory creationCode = ROUTE_PROCESSOR_4_CREATION_CODE;
        address deployed;
        assembly ("memory-safe") {
            deployed := create(0, add(creationCode, 0x20), mload(creationCode))
        }
        assertTrue(deployed != address(0), "RouteProcessor4 deployment failed");
        assertEq(deployed.codehash, KNOWN_ROUTE_PROCESSOR_4_CODEHASH);
    }
}
