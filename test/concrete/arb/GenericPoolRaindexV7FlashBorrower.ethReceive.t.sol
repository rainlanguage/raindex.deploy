// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {GenericPoolRaindexV7FlashBorrower} from "../../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";

/// The flash-borrower arb must accept ETH transfers without reverting so that
/// any ETH received mid-arb (e.g. returned by the pool during `_exchange`) can
/// be swept to msg.sender by finalizeArb. ETH sent with empty calldata hits
/// `receive()`; ETH (or any call) with non-matching calldata hits the payable
/// `fallback()`. Both must succeed and the value must land in the contract
/// balance.
contract GenericPoolRaindexV7FlashBorrowerEthReceiveTest is Test {
    function testReceiveAcceptsEthEmptyCalldata() external {
        GenericPoolRaindexV7FlashBorrower arb = new GenericPoolRaindexV7FlashBorrower();
        vm.deal(address(this), 5 ether);

        // Empty calldata + value routes to `receive()`.
        (bool ok,) = address(arb).call{value: 3 ether}("");
        assertTrue(ok, "receive() must accept ETH with empty calldata");
        assertEq(address(arb).balance, 3 ether, "ETH must land in the arb balance");
    }

    function testFallbackAcceptsEthAndArbitraryCalldata() external {
        GenericPoolRaindexV7FlashBorrower arb = new GenericPoolRaindexV7FlashBorrower();
        vm.deal(address(this), 5 ether);

        // Non-matching selector forces the payable `fallback()` rather than
        // `receive()`; it must accept both the call and the ETH.
        (bool ok,) = address(arb).call{value: 2 ether}(hex"deadbeef");
        assertTrue(ok, "fallback() must accept arbitrary calldata with ETH");
        assertEq(address(arb).balance, 2 ether, "ETH must land in the arb balance via fallback");
    }

    function testFallbackAcceptsZeroValueArbitraryCalldata() external {
        GenericPoolRaindexV7FlashBorrower arb = new GenericPoolRaindexV7FlashBorrower();

        // A zero-value arbitrary call must still succeed via fallback.
        (bool ok,) = address(arb).call(hex"0badf00d1234");
        assertTrue(ok, "fallback() must accept arbitrary zero-value calls");
    }
}
