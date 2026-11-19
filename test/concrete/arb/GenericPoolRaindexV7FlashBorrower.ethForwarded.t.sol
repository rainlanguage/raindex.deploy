// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibTestFlashBorrowerArb, FlashBorrowerSetup} from "test/util/lib/LibTestFlashBorrowerArb.sol";
import {LibTestArb} from "test/util/lib/LibTestArb.sol";
import {AllowanceCheckingExchange} from "test/util/concrete/AllowanceCheckingExchange.sol";

/// When the arb contract holds ETH, functionCallWithValue forwards it
/// to the pool during _exchange.
contract GenericPoolRaindexV7FlashBorrowerEthForwardedTest is Test {
    receive() external payable {}

    function testEthForwardedToExchangeDuringExchange() external {
        AllowanceCheckingExchange exchange = new AllowanceCheckingExchange();
        FlashBorrowerSetup memory setup = LibTestFlashBorrowerArb.setup(vm, address(exchange), 100e18);

        // Send ETH to the arb contract before calling arb5.
        vm.deal(address(setup.arb), 1 ether);

        setup.arb.arb5(setup.raindex, setup.takeOrdersConfig, setup.exchangeData, LibTestArb.noopTask());

        // The exchange received the ETH during the call.
        assertEq(exchange.lastEthReceived(), 1 ether);
        // The arb contract has no remaining ETH — it was swept to msg.sender
        // by finalizeArb.
        assertEq(address(setup.arb).balance, 0);
    }
}
