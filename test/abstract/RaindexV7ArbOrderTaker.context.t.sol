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
import {StateNamespace} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {LibNamespace} from "rainlang-0.2.35/src/lib/ns/LibNamespace.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

contract RaindexV7ArbOrderTakerContextTest is RaindexV7ExternalRealTest {
    function testRaindexV7ArbOrderTakerContext() external {
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
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.balanceOf.selector, address(arbOrderTaker)),
            abi.encode(3e12)
        );
        vm.mockCall(
            address(iToken1),
            abi.encodeWithSelector(IERC20.balanceOf.selector, address(arbOrderTaker)),
            abi.encode(4e12)
        );
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(12));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(12));

        // 5e18 is 5 eth as wei is 18 decimals.
        vm.deal(address(arbOrderTaker), 5e18);
        vm.prank(bob);
        arbOrderTaker.arb6(iRaindex, takeOrdersConfig, task);
    }
}
