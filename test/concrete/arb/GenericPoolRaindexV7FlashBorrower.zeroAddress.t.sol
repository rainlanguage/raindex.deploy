// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibTestFlashBorrowerArb, FlashBorrowerSetup} from "test/util/lib/LibTestFlashBorrowerArb.sol";
import {LibTestArb} from "test/util/lib/LibTestArb.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";
import {ZeroExchangeSpender, ZeroExchangePool} from "src/lib/LibGenericPoolExchange.sol";

/// A zero spender/pool in the caller-supplied exchange data reverts through the
/// real flash-borrower arb path (the guards fire after the flash loan + take
/// orders, before any approval/call), not just in the library in isolation.
contract GenericPoolRaindexV7FlashBorrowerZeroAddressTest is Test {
    function testArbZeroSpenderReverts() external {
        MockExchange exchange = new MockExchange();
        FlashBorrowerSetup memory setup = LibTestFlashBorrowerArb.setup(vm, address(exchange), 100e18);

        bytes memory badExchangeData = abi.encode(address(0), address(exchange), bytes(""));
        vm.expectRevert(ZeroExchangeSpender.selector);
        setup.arb.arb5(setup.raindex, setup.takeOrdersConfig, badExchangeData, LibTestArb.noopTask());
    }

    function testArbZeroPoolReverts() external {
        MockExchange exchange = new MockExchange();
        FlashBorrowerSetup memory setup = LibTestFlashBorrowerArb.setup(vm, address(exchange), 100e18);

        bytes memory badExchangeData = abi.encode(address(exchange), address(0), bytes(""));
        vm.expectRevert(ZeroExchangePool.selector);
        setup.arb.arb5(setup.raindex, setup.takeOrdersConfig, badExchangeData, LibTestArb.noopTask());
    }
}
