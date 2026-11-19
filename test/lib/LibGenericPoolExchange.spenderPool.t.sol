// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {LibGenericPoolExchangeHarness} from "test/util/concrete/LibGenericPoolExchangeHarness.sol";
import {AllowanceProbePool} from "test/util/concrete/AllowanceProbePool.sol";
import {FallbackPool} from "test/util/concrete/FallbackPool.sol";

/// @title LibGenericPoolExchangeSpenderPoolTest
/// @notice Audit A06-1/A06-2 (#2533): existing tests always set spender == pool.
/// Covers the spender != pool path (approve `spender`, call `pool`, revoke) and
/// the empty-encoded-call decode edge.
contract LibGenericPoolExchangeSpenderPoolTest is Test {
    LibGenericPoolExchangeHarness internal immutable iHarness = new LibGenericPoolExchangeHarness();

    /// A06-1: with spender != pool, the approval is granted to `spender` (not the
    /// pool) for the duration of the call to `pool`, then revoked.
    function testSpenderNotPoolApprovesSpenderCallsPool(address spender) external {
        MockToken token = new MockToken("Token", "TKN", 18);
        AllowanceProbePool pool = new AllowanceProbePool();
        vm.assume(spender != address(0));
        vm.assume(spender != address(pool));
        vm.assume(spender != address(token));

        bytes memory call =
            abi.encodeCall(AllowanceProbePool.probe, (IERC20(address(token)), address(iHarness), spender));
        iHarness.exchange(address(token), abi.encode(spender, address(pool), call));

        assertEq(pool.caller(), address(iHarness), "call routed to pool from the exchanger");
        assertEq(pool.spenderAllowance(), type(uint256).max, "spender approved for the call");
        assertEq(pool.poolAllowance(), 0, "pool itself NOT approved (spender != pool)");
        assertEq(token.allowance(address(iHarness), spender), 0, "approval revoked after the call");
    }

    /// A06-2: an empty `encodedFunctionCall` is still forwarded to the pool
    /// (hitting its receive/fallback) and the approve/revoke still runs.
    function testEmptyEncodedCallForwardedToPool(address spender) external {
        MockToken token = new MockToken("Token", "TKN", 18);
        FallbackPool pool = new FallbackPool();
        vm.assume(spender != address(0));

        iHarness.exchange(address(token), abi.encode(spender, address(pool), bytes("")));

        assertTrue(pool.called(), "empty call still reaches the pool");
        assertEq(token.allowance(address(iHarness), spender), 0, "approval revoked after the call");
    }

    /// The call to the pool forwards the exchanger's *entire* ETH balance, not a
    /// fixed or zero amount. Fund the harness with a fuzzed balance and assert
    /// the whole balance lands at the pool and none is left behind.
    function testForwardsEntireEthBalanceToPool(address spender, uint256 balance) external {
        MockToken token = new MockToken("Token", "TKN", 18);
        FallbackPool pool = new FallbackPool();
        vm.assume(spender != address(0));
        vm.deal(address(iHarness), balance);

        iHarness.exchange(address(token), abi.encode(spender, address(pool), bytes("")));

        assertEq(address(pool).balance, balance, "entire balance forwarded to pool");
        assertEq(address(iHarness).balance, 0, "no ETH left at the exchanger");
    }
}
