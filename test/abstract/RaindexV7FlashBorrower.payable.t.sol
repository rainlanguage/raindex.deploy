// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibTestFlashBorrowerArb, FlashBorrowerSetup} from "test/util/lib/LibTestFlashBorrowerArb.sol";
import {LibTestArb} from "test/util/lib/LibTestArb.sol";
import {AllowanceCheckingExchange} from "test/util/concrete/AllowanceCheckingExchange.sol";

/// arb5 is `payable`: it MUST accept native ETH sent as `msg.value` with the
/// call (the value is later forwarded into `_exchange` and swept out by
/// `finalizeArb`). This distinguishes `payable` runtime acceptance from the
/// pre-funded (`vm.deal`) ETH path covered by the ethForwarded test, which
/// sends `msg.value == 0`.
contract RaindexV7FlashBorrowerPayableTest is Test {
    receive() external payable {}

    function testArb4AcceptsMsgValueAndSweepsNetZero() external {
        AllowanceCheckingExchange exchange = new AllowanceCheckingExchange();
        FlashBorrowerSetup memory setup = LibTestFlashBorrowerArb.setup(vm, address(exchange), 100e18);

        // Fund this test contract so it can send ETH as msg.value with arb5.
        vm.deal(address(this), 1 ether);
        uint256 senderBalanceBefore = address(this).balance;

        // The arb contract starts with no ETH; all ETH arrives via msg.value.
        assertEq(address(setup.arb).balance, 0, "arb starts empty");

        // Send 1 ether AS msg.value with the arb5 call. If arb5 were not
        // payable (or rejected nonzero msg.value) this call would revert and
        // the test would fail before reaching any assertion.
        setup.arb.arb5{value: 1 ether}(setup.raindex, setup.takeOrdersConfig, setup.exchangeData, LibTestArb.noopTask());

        // The msg.value ETH was forwarded into the exchange during _exchange.
        assertEq(exchange.lastEthReceived(), 1 ether, "exchange received msg.value ETH");
        // The arb contract retains no ETH — it was swept to msg.sender by
        // finalizeArb.
        assertEq(address(setup.arb).balance, 0, "arb swept to zero");
        // Net-zero for the sender: the 1 ether came back via finalizeArb.
        assertEq(address(this).balance, senderBalanceBefore, "sender ETH net zero");
    }
}
