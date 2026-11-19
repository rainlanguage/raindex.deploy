// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";
import {FallbackPool} from "test/util/concrete/FallbackPool.sol";
import {ReentrantOnTakeOrdersStore} from "test/util/concrete/ReentrantOnTakeOrdersStore.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {LibTestArb} from "test/util/lib/LibTestArb.sol";
import {GenericPoolRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol";
import {UnexpectedOnTakeOrdersCaller} from "../../../src/abstract/RaindexV7ArbOrderTaker.sol";
import {
    OrderV4,
    OrderConfigV4,
    IOV2,
    EvaluableV4,
    TaskV3,
    TakeOrdersConfigV6
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {TEST_INTERPRETER_ADDRESS} from "rainlang-0.2.40/test/lib/deploy/LibTestInterpreterDeploy.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";

contract GenericPoolRaindexV7ArbOrderTakerReentrantCallbackTest is RaindexV7ExternalRealTest {
    address internal owner = address(uint160(uint256(keccak256("owner.reentrant.callback"))));
    address internal bot = address(uint160(uint256(keccak256("bot.reentrant.callback"))));

    /// An order's store re-entering `onTakeOrders2` during `arb6` is rejected,
    /// the arb completes and the caller's ETH is refunded to the caller.
    function testOrderStoreReentrantCallbackCannotTakeArbEth() external {
        GenericPoolRaindexV7ArbOrderTaker arb = new GenericPoolRaindexV7ArbOrderTaker();
        MockToken token0 = new MockToken("T0", "T0", 18);
        MockToken token1 = new MockToken("T1", "T1", 18);
        ReentrantOnTakeOrdersStore store = new ReentrantOnTakeOrdersStore(arb, token0);
        MockExchange pool = new MockExchange();
        token0.mint(address(pool), 1e18);

        token1.mint(owner, 10e18);
        vm.startPrank(owner);
        token1.approve(address(iRaindex), 10e18);
        iRaindex.deposit5(address(token1), bytes32(uint256(1)), LibDecimalFloat.packLossless(10, 0), new TaskV3[](0));
        vm.stopPrank();

        IOV2[] memory ins = new IOV2[](1);
        ins[0] = IOV2(address(token0), bytes32(uint256(1)));
        IOV2[] memory outs = new IOV2[](1);
        outs[0] = IOV2(address(token1), bytes32(uint256(1)));
        OrderConfigV4 memory cfg = OrderConfigV4(
            EvaluableV4(
                IInterpreterV4(TEST_INTERPRETER_ADDRESS),
                IInterpreterStoreV3(address(store)),
                iParserV2.parse2(":set(1 2),\n_ _: 1 1;:;")
            ),
            ins,
            outs,
            bytes32(0),
            bytes32(0),
            ""
        );
        vm.recordLogs();
        vm.prank(owner);
        iRaindex.addOrder5(cfg, new TaskV3[](0));
        OrderV4 memory order = LibTestTakeOrder.extractOrderFromLogs(vm.getRecordedLogs());

        TakeOrdersConfigV6 memory take = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        take.maximumIO = LibDecimalFloat.packLossless(1, 0);
        take.data = abi.encode(address(pool), address(pool), abi.encodeCall(MockExchange.swap, (token1, token0, 1e18)));

        vm.deal(bot, 1 ether);
        vm.prank(bot);
        arb.arb6{value: 1 ether}(iRaindex, take, LibTestArb.noopTask());

        assertEq(store.revertData(), abi.encodeWithSelector(UnexpectedOnTakeOrdersCaller.selector, address(store)));
        assertEq(address(store).balance, 0);
        assertEq(bot.balance, 1 ether);
        assertEq(token1.balanceOf(address(pool)), 1e18);
        assertEq(address(arb).balance, 0);
    }

    /// Outside an arb, anyone may call `onTakeOrders2` and it forwards a stray
    /// ETH balance to the named pool.
    function testOnTakeOrders2OutsideArbSweepsStrayEth(address caller) external {
        GenericPoolRaindexV7ArbOrderTaker arb = new GenericPoolRaindexV7ArbOrderTaker();
        MockToken token = new MockToken("T", "T", 18);
        FallbackPool sink = new FallbackPool();
        vm.deal(address(arb), 1 ether);

        vm.prank(caller);
        arb.onTakeOrders2(
            address(token), address(token), Float.wrap(0), Float.wrap(0), abi.encode(address(sink), address(sink), "")
        );

        assertEq(address(sink).balance, 1 ether);
        assertEq(address(arb).balance, 0);
    }
}
