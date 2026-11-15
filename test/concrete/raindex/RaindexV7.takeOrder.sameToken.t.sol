// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    IRaindexV7,
    OrderConfigV4,
    OrderV4,
    TaskV3,
    TakeOrderConfigV5,
    SignedContextV2,
    TakeOrdersConfigV6
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

contract RaindexV7TakeOrderSameTokenTest is RaindexV7ExternalRealTest {
    /// forge-config: default.fuzz.runs = 10
    function testTakeOrderSameToken(address alice, OrderConfigV4 memory configAlice) external {
        LibTestAddOrder.conformConfig(configAlice, iInterpreter, iStore);
        configAlice.validInputs[0].token = address(0);
        configAlice.validOutputs[0].token = address(0);

        OrderV4 memory orderAlice =
            OrderV4(alice, configAlice.evaluable, configAlice.validInputs, configAlice.validOutputs, configAlice.nonce);

        vm.prank(alice);
        iRaindex.addOrder5(configAlice, new TaskV3[](0));

        TakeOrderConfigV5[] memory takeOrders = new TakeOrderConfigV5[](1);
        takeOrders[0] = TakeOrderConfigV5({
            order: orderAlice, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)
        });

        TakeOrdersConfigV6 memory takeOrdersConfig = TakeOrdersConfigV6({
            minimumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
            maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
            IOIsInput: true,
            orders: takeOrders,
            data: ""
        });

        vm.expectRevert(abi.encodeWithSelector(IRaindexV7.TokenSelfTrade.selector));
        iRaindex.takeOrders5(takeOrdersConfig);
    }
}
