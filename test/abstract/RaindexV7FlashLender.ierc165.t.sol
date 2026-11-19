// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {IERC165} from "@openzeppelin-contracts-5.7.0/utils/introspection/IERC165.sol";
import {IERC3156FlashLender} from "../../src/abstract/RaindexV7FlashLender.sol";
import {ChildRaindexV7FlashLender} from "test/util/concrete/ChildRaindexV7FlashLender.sol";

contract RaindexV7FlashLenderIERC165Test is Test {
    /// Test that ERC165 and IERC3156FlashLender are supported interfaces
    /// as per ERC165.
    function testRaindexV7FlashLenderIERC165(bytes4 badInterfaceId) external {
        vm.assume(badInterfaceId != type(IERC165).interfaceId);
        vm.assume(badInterfaceId != type(IERC3156FlashLender).interfaceId);

        ChildRaindexV7FlashLender flashLender = new ChildRaindexV7FlashLender();
        assertTrue(flashLender.supportsInterface(type(IERC165).interfaceId));
        assertTrue(flashLender.supportsInterface(type(IERC3156FlashLender).interfaceId));
        assertFalse(flashLender.supportsInterface(badInterfaceId));
    }
}
