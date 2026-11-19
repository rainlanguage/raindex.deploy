// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    IRaindexV7,
    OrderConfigV4,
    OrderV4,
    EvaluableV4,
    ClearConfigV2,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";

contract RaindexV7ClearSameTokenTest is RaindexV7ExternalRealTest {
    /// forge-config: default.fuzz.runs = 10
    function testClearSameToken(
        address alice,
        address bob,
        OrderConfigV4 memory configAlice,
        OrderConfigV4 memory configBob
    ) external {
        vm.assume(alice != bob);

        LibTestAddOrder.conformConfig(configAlice, iInterpreter, iStore);
        LibTestAddOrder.conformConfig(configBob, iInterpreter, iStore);
        configAlice.validInputs[0].token = address(0);
        configAlice.validOutputs[0].token = address(0);
        configBob.validInputs[0].token = address(0);
        configBob.validOutputs[0].token = address(0);

        OrderV4 memory orderAlice =
            OrderV4(alice, configAlice.evaluable, configAlice.validInputs, configAlice.validOutputs, configAlice.nonce);
        OrderV4 memory orderBob =
            OrderV4(bob, configBob.evaluable, configBob.validInputs, configBob.validOutputs, configBob.nonce);

        vm.prank(alice);
        iRaindex.addOrder5(configAlice, new TaskV3[](0));

        vm.prank(bob);
        iRaindex.addOrder5(configBob, new TaskV3[](0));

        vm.expectRevert(abi.encodeWithSelector(IRaindexV7.TokenSelfTrade.selector));
        iRaindex.clear4(
            orderAlice, orderBob, ClearConfigV2(0, 0, 0, 0, 0, 0), new SignedContextV2[](0), new SignedContextV2[](0)
        );
    }
}
