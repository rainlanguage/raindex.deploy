// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {GenericPoolRaindexV7FlashBorrower} from "../../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";
import {
    IRaindexV7,
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    OrderV4,
    IOV2,
    EvaluableV4,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.39/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.39/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";

/// @dev Exchange that re-enters arb5 when called during _exchange.
contract ReentrantExchange {
    GenericPoolRaindexV7FlashBorrower internal immutable iArb;
    IRaindexV7 internal immutable iRaindex;

    constructor(GenericPoolRaindexV7FlashBorrower arb, IRaindexV7 raindex) {
        iArb = arb;
        iRaindex = raindex;
    }

    /// Called by pool.functionCallWithValue during _exchange. Re-enters arb5.
    fallback() external payable {
        // Build minimal valid-looking args. The reentrancy guard will revert
        // before any of this is actually processed.
        IOV2[] memory ios = new IOV2[](1);
        ios[0] = IOV2(address(0x1), bytes32(0));

        OrderV4 memory order = OrderV4({
            owner: address(0x1234),
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            validInputs: ios,
            validOutputs: ios,
            nonce: bytes32(0)
        });

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        orders[0] = TakeOrderConfigV5(order, 0, 0, new SignedContextV2[](0));

        iArb.arb5(
            iRaindex,
            TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(1, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: ""
            }),
            hex"",
            TaskV3({
                evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
                signedContext: new SignedContextV2[](0)
            })
        );
    }
}
