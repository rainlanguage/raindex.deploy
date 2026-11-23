// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {
    CREATION_CODE as RP_ARB_OT_V6_CREATION_CODE
} from "../../generated/0_1_15/RouteProcessorRaindexV6ArbOrderTaker.sol";
import {DEPLOYED_ADDRESS as ROUTE_PROCESSOR_4_ADDRESS} from "../../generated/0_1_15/RouteProcessor4.sol";
import {DEPLOYED_ADDRESS as ROUTE_PROCESSOR_6_ADDRESS} from "../../generated/candidate/RouteProcessor6.sol";

/// Thrown when the creation code does not hold exactly one `PUSH20` of the
/// address to replace.
/// @param count The number of matches.
error AddressPushCount(uint256 count);

/// @title LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode
/// @notice The `RouteProcessorRaindexV6ArbOrderTaker` creation code of release
/// `0_1_15`, with the `RouteProcessor4` address it swaps through replaced by
/// the `RouteProcessor6` address. The code holds that address once, as a
/// `PUSH20` of the same length, so all other bytes stay the same.
library LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode {
    /// The `PUSH20` opcode.
    bytes1 constant PUSH20 = 0x73;

    /// @return The creation code.
    function creationCode() internal pure returns (bytes memory) {
        return replaceAddressPush(RP_ARB_OT_V6_CREATION_CODE, ROUTE_PROCESSOR_4_ADDRESS, ROUTE_PROCESSOR_6_ADDRESS);
    }

    /// Replaces the one `PUSH20 from` in `code` with `PUSH20 to`.
    /// @param code The code, mutated.
    /// @param from The address to replace.
    /// @param to The replacement address.
    /// @return `code`.
    function replaceAddressPush(bytes memory code, address from, address to) internal pure returns (bytes memory) {
        bytes20 search = bytes20(from);
        uint256 count = 0;
        uint256 at = 0;
        for (uint256 i = 0; i + 21 <= code.length; i++) {
            if (code[i] != PUSH20) {
                continue;
            }
            bool found = true;
            for (uint256 j = 0; j < 20; j++) {
                if (code[i + 1 + j] != search[j]) {
                    found = false;
                    break;
                }
            }
            if (found) {
                count++;
                at = i + 1;
            }
        }
        if (count != 1) {
            revert AddressPushCount(count);
        }
        bytes20 replacement = bytes20(to);
        for (uint256 j = 0; j < 20; j++) {
            code[at + j] = replacement[j];
        }
        return code;
    }
}
