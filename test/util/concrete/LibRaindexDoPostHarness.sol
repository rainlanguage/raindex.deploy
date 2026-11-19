// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {LibRaindex} from "src/lib/LibRaindex.sol";
import {TaskV3} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";

/// @dev Exposes the internal `LibRaindex.doPost` for direct testing.
contract LibRaindexDoPostHarness {
    function doPost(bytes32[][] memory context, TaskV3[] memory post) external {
        LibRaindex.doPost(context, post);
    }
}
