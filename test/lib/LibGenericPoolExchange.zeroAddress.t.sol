// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {ZeroExchangeSpender, ZeroExchangePool} from "src/lib/LibGenericPoolExchange.sol";
import {LibGenericPoolExchangeHarness} from "test/util/concrete/LibGenericPoolExchangeHarness.sol";

/// @title LibGenericPoolExchangeZeroAddressTest
/// @notice `exchange` decodes a caller-controlled `(spender, pool, ...)` from
/// `data`; a zero spender or pool must revert loudly rather than silently
/// no-op'ing the approval or making a value call to the zero address.
contract LibGenericPoolExchangeZeroAddressTest is Test {
    LibGenericPoolExchangeHarness internal immutable iHarness = new LibGenericPoolExchangeHarness();

    /// A zero spender reverts before any approval or call (token untouched).
    function testExchangeZeroSpenderReverts(address token, address pool, bytes memory call) external {
        vm.assume(pool != address(0));
        vm.expectRevert(ZeroExchangeSpender.selector);
        iHarness.exchange(token, abi.encode(address(0), pool, call));
    }

    /// A non-zero spender with a zero pool reverts before the call.
    function testExchangeZeroPoolReverts(address token, address spender, bytes memory call) external {
        vm.assume(spender != address(0));
        vm.expectRevert(ZeroExchangePool.selector);
        iHarness.exchange(token, abi.encode(spender, address(0), call));
    }

    /// When both are zero the spender guard (checked first) is the one that trips.
    function testExchangeBothZeroRevertsOnSpender(address token, bytes memory call) external {
        vm.expectRevert(ZeroExchangeSpender.selector);
        iHarness.exchange(token, abi.encode(address(0), address(0), call));
    }
}
