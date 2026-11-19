// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {RouteProcessorRaindexV7ArbOrderTaker} from "src/concrete/arb/RouteProcessorRaindexV7ArbOrderTaker.sol";

/// @title RouteProcessorRaindexV7ArbOrderTakerEthReceiptTest
/// @notice Audit A07-2 (#2534): the arb's payable `receive()` / `fallback()`
/// accept ETH (so a route processor can refund native gas; `finalizeArb` later
/// sweeps it to `msg.sender`). Nothing covered the bare ETH-receipt paths.
contract RouteProcessorRaindexV7ArbOrderTakerEthReceiptTest is Test {
    /// Empty calldata routes to `receive()`, which accepts the ETH.
    function testReceiveAcceptsEth(uint256 amount) external {
        amount = bound(amount, 0, 1e24);
        RouteProcessorRaindexV7ArbOrderTaker arb = new RouteProcessorRaindexV7ArbOrderTaker();
        vm.deal(address(this), amount);

        (bool ok,) = address(arb).call{value: amount}("");
        assertTrue(ok, "receive() accepted ETH");
        assertEq(address(arb).balance, amount, "arb holds the ETH");
    }

    /// Non-empty calldata matching no function selector routes to `fallback()`,
    /// which also accepts the ETH.
    function testFallbackAcceptsEth(uint256 amount) external {
        amount = bound(amount, 0, 1e24);
        RouteProcessorRaindexV7ArbOrderTaker arb = new RouteProcessorRaindexV7ArbOrderTaker();
        vm.deal(address(this), amount);

        // A one-byte payload can't match a 4-byte selector, so it hits fallback.
        (bool ok,) = address(arb).call{value: amount}(hex"01");
        assertTrue(ok, "fallback() accepted ETH");
        assertEq(address(arb).balance, amount, "arb holds the ETH");
    }
}
