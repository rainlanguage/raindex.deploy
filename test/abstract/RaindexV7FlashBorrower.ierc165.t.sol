// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {IERC165} from "@openzeppelin-contracts-5.7.0/utils/introspection/IERC165.sol";
import {IERC3156FlashBorrower} from "../../src/abstract/RaindexV7FlashBorrower.sol";
import {ChildRaindexV7FlashBorrower} from "test/util/concrete/ChildRaindexV7FlashBorrower.sol";

contract RaindexV7FlashBorrowerIERC165Test is Test {
    /// Test that ERC165 and IERC3156FlashBorrower are supported interfaces
    /// as per ERC165.
    function testRaindexV7FlashBorrowerIERC165(bytes4 badInterfaceId) external {
        vm.assume(badInterfaceId != type(IERC165).interfaceId);
        vm.assume(badInterfaceId != type(IERC3156FlashBorrower).interfaceId);

        ChildRaindexV7FlashBorrower flashBorrower = new ChildRaindexV7FlashBorrower();
        assertTrue(flashBorrower.supportsInterface(type(IERC165).interfaceId));
        assertTrue(flashBorrower.supportsInterface(type(IERC3156FlashBorrower).interfaceId));
        assertFalse(flashBorrower.supportsInterface(badInterfaceId));
    }
}
