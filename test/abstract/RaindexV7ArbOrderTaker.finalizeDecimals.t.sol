// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {ChildRaindexV7ArbOrderTaker} from "../util/concrete/ChildRaindexV7ArbOrderTaker.sol";
import {TaskV3, SignedContextV2, EvaluableV4} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {RaindexV7ExternalRealTest} from "../util/abstract/RaindexV7ExternalRealTest.sol";
import {
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    IOV2,
    OrderConfigV4,
    OrderV4,
    IInterpreterV4
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// @dev Discriminates that `arb6` passes the input token's OWN decimals and the
/// output token's OWN decimals to `finalizeArb` in the correct positions. The
/// pre-existing `context` test uses equal decimals (12/12) for both tokens, so a
/// mutation that swaps input-decimals for output-decimals (or vice versa)
/// survives there. Here the input token (token0) is 6-decimal and the output
/// token (token1) is 9-decimal, with balances chosen so the post-arb context
/// floats equal exactly 3 (input) and 4 (output) ONLY when each token is scaled
/// by its own decimals.
contract RaindexV7ArbOrderTakerFinalizeDecimalsTest is RaindexV7ExternalRealTest {
    function testRaindexV7ArbOrderTakerFinalizeUsesPerTokenDecimals() external {
        address alice = address(999999);
        address bob = address(999998);
        ChildRaindexV7ArbOrderTaker arbOrderTaker = new ChildRaindexV7ArbOrderTaker();

        OrderConfigV4 memory aliceOrderConfig;
        {
            IOV2[] memory aliceValidInputs = new IOV2[](1);
            aliceValidInputs[0] = IOV2({token: address(iToken0), vaultId: 0});

            IOV2[] memory aliceValidOutputs = new IOV2[](1);
            aliceValidOutputs[0] = IOV2({token: address(iToken1), vaultId: 0});

            aliceOrderConfig = OrderConfigV4({
                evaluable: EvaluableV4(iInterpreter, iStore, ""),
                validInputs: aliceValidInputs,
                validOutputs: aliceValidOutputs,
                nonce: 0,
                secret: 0,
                meta: ""
            });
        }

        OrderV4 memory aliceOrder = OrderV4({
            owner: alice,
            evaluable: aliceOrderConfig.evaluable,
            validInputs: aliceOrderConfig.validInputs,
            validOutputs: aliceOrderConfig.validOutputs,
            nonce: aliceOrderConfig.nonce
        });

        TakeOrderConfigV5 memory aliceTakeOrderConfig = TakeOrderConfigV5({
            order: aliceOrder, inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)
        });

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        orders[0] = aliceTakeOrderConfig;
        TakeOrdersConfigV6 memory takeOrdersConfig = TakeOrdersConfigV6({
            minimumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
            maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
            IOIsInput: true,
            orders: orders,
            data: ""
        });

        // Assert that the post-arb context column scaled each swept token balance
        // by its OWN token decimals: input float == 3, output float == 4, gas == 5.
        TaskV3 memory task = TaskV3({
            evaluable: EvaluableV4({
                interpreter: iInterpreter,
                store: iStore,
                bytecode: iParserV2.parse2(
                    bytes(
                        string.concat(
                            ":ensure(equal-to(context<1 0>() 3) \"input token\"),",
                            ":ensure(equal-to(context<1 1>() 4) \"output token\"),",
                            ":ensure(equal-to(context<1 2>() 5) \"gas balance\");"
                        )
                    )
                )
            }),
            signedContext: new SignedContextV2[](0)
        });

        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.approve.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.allowance.selector), abi.encode(0));
        // Input token (token0) is 6-decimal: 3e6 raw == 3.0 only when scaled by 6.
        vm.mockCall(
            address(iToken0), abi.encodeWithSelector(IERC20.balanceOf.selector, address(arbOrderTaker)), abi.encode(3e6)
        );
        // Output token (token1) is 9-decimal: 4e9 raw == 4.0 only when scaled by 9.
        vm.mockCall(
            address(iToken1), abi.encodeWithSelector(IERC20.balanceOf.selector, address(arbOrderTaker)), abi.encode(4e9)
        );
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(6));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(9));

        // 5e18 is 5 eth as wei is 18 decimals.
        vm.deal(address(arbOrderTaker), 5e18);
        vm.prank(bob);
        arbOrderTaker.arb6(iRaindex, takeOrdersConfig, task);
    }
}
