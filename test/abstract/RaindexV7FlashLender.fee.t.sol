// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalMockTest} from "test/util/abstract/RaindexV7ExternalMockTest.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {FlashLenderUnsupportedToken} from "src/abstract/RaindexV7FlashLender.sol";

/// @title RaindexV7FlashLenderFeeTest
/// Tests the fee charged by `RaindexV7FlashLender`.
contract RaindexV7FlashLenderFeeTest is RaindexV7ExternalMockTest {
    /// The fee is 0 for a supported token, i.e. one Raindex holds a balance of
    /// (`maxFlashLoan(token) > 0`), for any amount.
    function testFlashFeeZeroForSupportedToken(uint256 balance, uint256 amount) public {
        balance = bound(balance, 1, type(uint256).max);
        vm.mockCall(
            address(iToken0), abi.encodeWithSelector(IERC20.balanceOf.selector, address(iRaindex)), abi.encode(balance)
        );
        assertEq(iRaindex.flashFee(address(iToken0), amount), 0);
    }

    /// Per ERC-3156, `flashFee` reverts for an unsupported token — one Raindex
    /// holds no balance of (`maxFlashLoan(token) == 0`) — rather than returning
    /// a fee for a loan that could never settle.
    function testFlashFeeRevertsUnsupportedToken(uint256 amount) public {
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.balanceOf.selector, address(iRaindex)),
            abi.encode(uint256(0))
        );
        vm.expectRevert(abi.encodeWithSelector(FlashLenderUnsupportedToken.selector, address(iToken0)));
        iRaindex.flashFee(address(iToken0), amount);
    }

    /// Deterministic boundary of the strict-equality support guard: a balance
    /// of exactly 0 reverts `FlashLenderUnsupportedToken`, while a balance of
    /// exactly 1 (the smallest supported balance) returns fee 0 — pinning the
    /// `== 0` guard against off-by-one mutations (`<= 1`, `< 1`, etc).
    function testFlashFeeZeroBalanceBoundary(uint256 amount) public {
        // Balance 0: unsupported, reverts.
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.balanceOf.selector, address(iRaindex)),
            abi.encode(uint256(0))
        );
        vm.expectRevert(abi.encodeWithSelector(FlashLenderUnsupportedToken.selector, address(iToken0)));
        iRaindex.flashFee(address(iToken0), amount);

        // Balance 1: smallest supported balance, returns fee 0 (no revert).
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.balanceOf.selector, address(iRaindex)),
            abi.encode(uint256(1))
        );
        assertEq(iRaindex.flashFee(address(iToken0), amount), 0);
    }

    /// `flashFee` and `maxFlashLoan` agree on support per ERC-3156: a token is
    /// unsupported iff `maxFlashLoan == 0`, in which case `flashFee` reverts;
    /// otherwise `flashFee` returns 0.
    function testFlashFeeMatchesMaxFlashLoanSupport(uint256 balance, uint256 amount) public {
        vm.mockCall(
            address(iToken0), abi.encodeWithSelector(IERC20.balanceOf.selector, address(iRaindex)), abi.encode(balance)
        );
        if (balance == 0) {
            assertEq(iRaindex.maxFlashLoan(address(iToken0)), 0);
            vm.expectRevert(abi.encodeWithSelector(FlashLenderUnsupportedToken.selector, address(iToken0)));
            iRaindex.flashFee(address(iToken0), amount);
        } else {
            assertEq(iRaindex.maxFlashLoan(address(iToken0)), balance);
            assertEq(iRaindex.flashFee(address(iToken0), amount), 0);
        }
    }
}
