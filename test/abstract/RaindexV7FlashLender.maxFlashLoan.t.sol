// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalMockTest} from "test/util/abstract/RaindexV7ExternalMockTest.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

/// @title RaindexV7FlashLenderMaxFlashLoanTest
/// Tests the maximum flash loan amount for `RaindexV7FlashLender`.
contract RaindexV7FlashLenderMaxFlashLoanTest is RaindexV7ExternalMockTest {
    /// Tests that the maximum flash loan amount for `RaindexV7FlashLender` is
    /// the balance of the token in the raindex.
    function testFlashMaxLoan(uint256 amount) public {
        vm.mockCall(
            address(iToken0), abi.encodeWithSelector(IERC20.balanceOf.selector, address(iRaindex)), abi.encode(amount)
        );
        assertEq(iRaindex.maxFlashLoan(address(iToken0)), amount);
    }
}
