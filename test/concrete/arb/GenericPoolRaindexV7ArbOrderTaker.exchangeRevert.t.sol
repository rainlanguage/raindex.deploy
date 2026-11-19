// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibTestArb, OrderTakerSetup} from "test/util/lib/LibTestArb.sol";
import {RevertingExchange} from "test/util/concrete/RevertingExchange.sol";

/// If the exchange call reverts, the entire arb6 reverts with the
/// exchange's revert reason bubbled up.
contract GenericPoolRaindexV7ArbOrderTakerExchangeRevertTest is Test {
    function testExchangeRevertPropagates() external {
        RevertingExchange exchange = new RevertingExchange();
        OrderTakerSetup memory setup = LibTestArb.setup(vm, address(exchange), 100e18);

        vm.expectRevert("exchange failed");
        setup.arb.arb6(setup.raindex, setup.takeOrdersConfig, LibTestArb.noopTask());
    }
}
