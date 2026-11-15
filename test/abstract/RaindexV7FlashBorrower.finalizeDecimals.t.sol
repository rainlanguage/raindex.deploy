// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "../util/abstract/RaindexV7ExternalRealTest.sol";
import {NoExchangeFlashBorrower} from "test/util/concrete/NoExchangeFlashBorrower.sol";
import {LeftoverFlashLendingMockRaindex} from "test/util/concrete/LeftoverFlashLendingMockRaindex.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {LibRaindexDeploy} from "../../src/lib/deploy/LibRaindexDeploy.sol";
import {
    IRaindexV7,
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    OrderV4,
    IOV2,
    EvaluableV4,
    SignedContextV2,
    TaskV3,
    IInterpreterV4
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.35/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";

/// @dev Discriminates that `arb5` passes the INPUT token's OWN decimals and the
/// OUTPUT token's OWN decimals to `finalizeArb` in the correct positions. The
/// pre-existing `mixedDecimals` / `realTokenTransfers` tests leave the arb with
/// a zero leftover balance and supply an EMPTY post-arb task, so a mutation that
/// swaps input-decimals for output-decimals in the `finalizeArb` call survives
/// there (the input float is always zero, the empty task never runs).
///
/// Here the input token is 6-decimal and the output token is 9-decimal, the arb
/// is pre-funded with a deterministic leftover of each token (3e6 input, 4e9
/// output) that the mock raindex's no-op `takeOrders5` leaves untouched, and a
/// REAL post-arb task asserts the swept context floats. context<1 0>() == 3 and
/// context<1 1>() == 4 hold ONLY when each leftover is scaled by its OWN
/// decimals (3e6 / 1e6 == 3; 4e9 / 1e9 == 4). Swapping input-decimals for
/// output-decimals makes context<1 0>() == 0.003 (reverts "input token"); the
/// reverse makes context<1 1>() == 4000 (reverts "output token").
contract RaindexV7FlashBorrowerFinalizeDecimalsTest is RaindexV7ExternalRealTest {
    function testRaindexV7FlashBorrowerFinalizeUsesPerTokenDecimals() external {
        // 6-decimal input token, 9-decimal output token.
        MockToken inputToken = new MockToken("Input", "IN", 6);
        MockToken outputToken = new MockToken("Output", "OUT", 9);

        // Override the etched raindex with the leftover-preserving mock.
        LeftoverFlashLendingMockRaindex mockRaindex = new LeftoverFlashLendingMockRaindex();
        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(mockRaindex).code);
        LeftoverFlashLendingMockRaindex raindex =
            LeftoverFlashLendingMockRaindex(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);

        NoExchangeFlashBorrower arb = new NoExchangeFlashBorrower();

        // Pre-fund the arb with the exact leftovers we will assert on. The no-op
        // takeOrders5 leaves these untouched; the flash loan of the output token
        // is net-neutral so the 4e9 output leftover survives.
        inputToken.mint(address(arb), 3e6);
        outputToken.mint(address(arb), 4e9);

        // The mock raindex needs the output token to satisfy the flash loan it
        // makes (minimumIO == 1.0 -> 1e9 at 9 decimals).
        outputToken.mint(address(raindex), 100e9);

        IOV2[] memory validInputs = new IOV2[](1);
        validInputs[0] = IOV2(address(inputToken), bytes32(0));
        IOV2[] memory validOutputs = new IOV2[](1);
        validOutputs[0] = IOV2(address(outputToken), bytes32(0));

        OrderV4 memory order = OrderV4({
            owner: address(0x1234),
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            validInputs: validInputs,
            validOutputs: validOutputs,
            nonce: bytes32(0)
        });

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        orders[0] = TakeOrderConfigV5(order, 0, 0, new SignedContextV2[](0));

        // Real post-arb task: assert each swept leftover was scaled by its OWN
        // decimals. context<1 0>() is the input float, context<1 1>() the output.
        TaskV3 memory task = TaskV3({
            evaluable: EvaluableV4({
                interpreter: iInterpreter,
                store: iStore,
                bytecode: iParserV2.parse2(
                    bytes(
                        string.concat(
                            ":ensure(equal-to(context<1 0>() 3) \"input token\"),",
                            ":ensure(equal-to(context<1 1>() 4) \"output token\");"
                        )
                    )
                )
            }),
            signedContext: new SignedContextV2[](0)
        });

        arb.arb5(
            IRaindexV7(address(raindex)),
            TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(1, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: ""
            }),
            "",
            task
        );
    }
}
