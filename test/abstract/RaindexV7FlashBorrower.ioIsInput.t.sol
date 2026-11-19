// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {ArbTest} from "test/util/abstract/ArbTest.sol";

import {GenericPoolRaindexV7FlashBorrower} from "../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";
import {ArbRequiresInputIO} from "../../src/abstract/RaindexV7FlashBorrower.sol";
import {
    IRaindexV7,
    EvaluableV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    IInterpreterV4,
    IInterpreterStoreV3,
    TaskV3,
    SignedContextV2
} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.39/test/lib/deploy/LibTestInterpreterDeploy.sol";

contract RaindexV7FlashBorrowerIOIsInputTest is ArbTest {
    function buildArb() internal override returns (address payable) {
        return payable(address(new GenericPoolRaindexV7FlashBorrower()));
    }

    constructor() ArbTest() {}

    /// arb5 derives the flash-loan amount from `minimumIO` as the order output the
    /// taker receives, which only matches when `minimumIO` is the taker input. It
    /// MUST revert with `ArbRequiresInputIO` when `IOIsInput == false`, before any
    /// flash loan is taken. The single order is never inspected (the guard reverts
    /// first), so a default entry suffices.
    function testArb4RevertsWhenIOIsInputFalse() external {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);

        vm.expectRevert(abi.encodeWithSelector(ArbRequiresInputIO.selector));
        GenericPoolRaindexV7FlashBorrower(iArb)
            .arb5(
                iRaindex,
                TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(0, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: false,
                orders: orders,
                data: ""
            }),
                "",
                TaskV3({
                evaluable: EvaluableV4(
                    IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), ""
                ),
                signedContext: new SignedContextV2[](0)
            })
            );
    }
}
