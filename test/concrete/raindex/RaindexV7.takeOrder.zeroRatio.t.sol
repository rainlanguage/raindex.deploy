// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {
    OrderV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    TaskV3,
    IRaindexV7
} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";
import {SignedContextV2} from "rainlang-0.2.36/src/interface/IInterpreterCallerV5.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";
import {MinimumIO} from "../../../src/error/ErrRaindexV7.sol";
import {RecordingOrderTaker} from "test/util/concrete/RecordingOrderTaker.sol";

/// A `takeOrders5` order whose IO ratio is 0 gives its output for nothing: the
/// taker receives the order's output (capped by `outputMax`, and in the
/// market-buy branch by the remaining taker IO) and pays 0.
contract RaindexV7TakeOrderZeroRatioTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    address internal alice = address(uint160(uint256(keccak256("alice.rain.test"))));
    address internal bob = address(uint160(uint256(keccak256("bob.rain.test"))));

    uint256 internal constant VAULT_DEPOSIT = 1e9;

    function _order(uint256 outputMax, bytes memory ratio) internal returns (OrderV4 memory) {
        return LibTestTakeOrder.addOrderWithExpression(
            vm,
            alice,
            bytes(string.concat("_ _:", vm.toString(outputMax), " ", string(ratio), ";:;")),
            address(iToken0),
            bytes32(uint256(0x01)),
            address(iToken1),
            bytes32(uint256(0x01))
        );
    }

    function _f(uint256 x) internal pure returns (Float) {
        // forge-lint: disable-next-line(unsafe-typecast)
        return LibDecimalFloat.packLossless(int256(x), 0);
    }

    function _setUpVaultAndTransfers() internal {
        vm.mockCall(
            address(iToken1),
            abi.encodeWithSelector(IERC20.transferFrom.selector, alice, address(iRaindex)),
            abi.encode(true)
        );
        vm.prank(alice);
        iRaindex.deposit5(address(iToken1), bytes32(uint256(0x01)), _f(VAULT_DEPOSIT), new TaskV3[](0));
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20.transfer.selector), abi.encode(true));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
    }

    function _two(OrderV4 memory a, OrderV4 memory b) internal pure returns (TakeOrderConfigV5[] memory) {
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](2);
        orders[0] = TakeOrderConfigV5(a, 0, 0, new SignedContextV2[](0));
        orders[1] = TakeOrderConfigV5(b, 0, 0, new SignedContextV2[](0));
        return orders;
    }

    function _assertAliceVaults(uint256 outputSpent, Float inputReceived) internal view {
        assertTrue(
            iRaindex.vaultBalance2(alice, address(iToken1), bytes32(uint256(0x01))).eq(_f(VAULT_DEPOSIT - outputSpent)),
            "alice output vault"
        );
        assertTrue(
            iRaindex.vaultBalance2(alice, address(iToken0), bytes32(uint256(0x01))).eq(inputReceived),
            "alice input vault"
        );
    }

    /// Market buy: the taker receives `min(outputMax, maximumIO)` and pays 0.
    function testZeroRatioMarketBuyTakenForNothing(uint256 outputMax, uint256 maximumIO) external {
        outputMax = bound(outputMax, 1, VAULT_DEPOSIT);
        maximumIO = bound(maximumIO, 1, VAULT_DEPOSIT * 2);
        _setUpVaultAndTransfers();
        OrderV4 memory order = _order(outputMax, "0");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        config.maximumIO = _f(maximumIO);
        uint256 expected = outputMax < maximumIO ? outputMax : maximumIO;

        vm.expectCall(address(iToken1), abi.encodeCall(IERC20.transfer, (bob, expected * 1e18)), 1);
        vm.expectCall(address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), 0);
        vm.prank(bob);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        assertTrue(totalTakerInput.eq(_f(expected)), "taker input");
        assertTrue(totalTakerOutput.isZero(), "taker output");
        _assertAliceVaults(expected, LibDecimalFloat.FLOAT_ZERO);
    }

    /// Market sell: the taker receives the full `outputMax`, whatever its
    /// `maximumIO`, and pays 0.
    function testZeroRatioMarketSellTakenForNothing(uint256 outputMax, uint256 maximumIO) external {
        outputMax = bound(outputMax, 1, VAULT_DEPOSIT);
        maximumIO = bound(maximumIO, 1, VAULT_DEPOSIT * 2);
        _setUpVaultAndTransfers();
        OrderV4 memory order = _order(outputMax, "0");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        config.IOIsInput = false;
        config.maximumIO = _f(maximumIO);

        vm.expectCall(address(iToken1), abi.encodeCall(IERC20.transfer, (bob, outputMax * 1e18)), 1);
        vm.expectCall(address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), 0);
        vm.prank(bob);
        vm.expectEmit(true, true, true, true);
        emit IRaindexV7.TakeOrderV4(bob, config.orders[0], _f(outputMax), LibDecimalFloat.FLOAT_ZERO);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        assertTrue(totalTakerInput.eq(_f(outputMax)), "taker input");
        assertTrue(totalTakerOutput.isZero(), "taker output");
        _assertAliceVaults(outputMax, LibDecimalFloat.FLOAT_ZERO);
    }

    /// A claim: a 0-ratio order with a positive `outputMax`, taken with no IO
    /// or ratio limit, gives the taker its full output for nothing in both
    /// branches.
    function testZeroRatioClaimTakenInFullForNothing(bool ioIsInput, uint256 outputMax) external {
        outputMax = bound(outputMax, 1, VAULT_DEPOSIT);
        _setUpVaultAndTransfers();
        OrderV4 memory order = _order(outputMax, "0");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        config.IOIsInput = ioIsInput;
        config.maximumIORatio = LibDecimalFloat.packLossless(type(int224).max, 0);
        config.maximumIO = LibDecimalFloat.packLossless(type(int224).max, 0);

        vm.expectCall(address(iToken1), abi.encodeCall(IERC20.transfer, (bob, outputMax * 1e18)), 1);
        vm.expectCall(address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), 0);
        vm.prank(bob);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        assertTrue(totalTakerInput.eq(_f(outputMax)), "taker input");
        assertTrue(totalTakerOutput.isZero(), "taker output");
        _assertAliceVaults(outputMax, LibDecimalFloat.FLOAT_ZERO);
    }

    /// Market sell: a 0-ratio order consumes none of the remaining taker IO,
    /// so a priced order after it still fills up to the full `maximumIO`.
    function testZeroRatioMarketSellBatchLeavesTakerIOForPricedOrder() external {
        _setUpVaultAndTransfers();
        OrderV4 memory free = _order(5, "0");
        OrderV4 memory priced = _order(10, "2");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(_two(free, priced));
        config.IOIsInput = false;
        config.maximumIO = _f(4);

        vm.expectCall(address(iToken1), abi.encodeCall(IERC20.transfer, (bob, 7e18)), 1);
        vm.expectCall(address(iToken0), abi.encodeCall(IERC20.transferFrom, (bob, address(iRaindex), 4e18)), 1);
        vm.prank(bob);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        // free: 5 for 0. priced: takerOutput min(20, 4) = 4, takerInput 4 / 2 = 2.
        assertTrue(totalTakerInput.eq(_f(7)), "taker input");
        assertTrue(totalTakerOutput.eq(_f(4)), "taker output");
        _assertAliceVaults(7, _f(4));
    }

    /// Market buy: a 0-ratio order consumes remaining taker IO by its output,
    /// and a priced order after it fills the rest.
    function testZeroRatioMarketBuyBatchWithPricedOrder() external {
        _setUpVaultAndTransfers();
        OrderV4 memory free = _order(5, "0");
        OrderV4 memory priced = _order(10, "2");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(_two(free, priced));
        config.maximumIO = _f(8);

        vm.expectCall(address(iToken1), abi.encodeCall(IERC20.transfer, (bob, 8e18)), 1);
        vm.expectCall(address(iToken0), abi.encodeCall(IERC20.transferFrom, (bob, address(iRaindex), 6e18)), 1);
        vm.prank(bob);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        // free: 5 for 0, remaining 3. priced: takerInput 3, takerOutput 3 * 2 = 6.
        assertTrue(totalTakerInput.eq(_f(8)), "taker input");
        assertTrue(totalTakerOutput.eq(_f(6)), "taker output");
        _assertAliceVaults(8, _f(6));
    }

    /// Market buy: `minimumIO` counts the 0-ratio order's output.
    function testZeroRatioMarketBuyMinimumIO(uint256 outputMax, uint256 minimumIO) external {
        outputMax = bound(outputMax, 1, VAULT_DEPOSIT);
        minimumIO = bound(minimumIO, 1, VAULT_DEPOSIT * 2);
        _setUpVaultAndTransfers();
        OrderV4 memory order = _order(outputMax, "0");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        config.minimumIO = _f(minimumIO);

        vm.prank(bob);
        if (minimumIO > outputMax) {
            vm.expectRevert(abi.encodeWithSelector(MinimumIO.selector, _f(minimumIO), _f(outputMax)));
        }
        iRaindex.takeOrders5(config);
    }

    /// Market sell: `minimumIO` counts taker output, to which a 0-ratio order
    /// adds nothing.
    function testZeroRatioMarketSellMinimumIOReverts(uint256 minimumIO) external {
        minimumIO = bound(minimumIO, 1, VAULT_DEPOSIT);
        _setUpVaultAndTransfers();
        OrderV4 memory order = _order(5, "0");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        config.IOIsInput = false;
        config.minimumIO = _f(minimumIO);

        vm.prank(bob);
        vm.expectRevert(abi.encodeWithSelector(MinimumIO.selector, _f(minimumIO), LibDecimalFloat.FLOAT_ZERO));
        iRaindex.takeOrders5(config);
    }

    /// A 0-ratio order with a zero `outputMax` is still skipped with
    /// `OrderZeroAmount` in both branches.
    function testZeroRatioZeroOutputMaxSkips(bool ioIsInput) external {
        _setUpVaultAndTransfers();
        OrderV4 memory order = _order(0, "0");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        config.IOIsInput = ioIsInput;

        vm.prank(bob);
        vm.expectEmit(true, true, true, true);
        emit IRaindexV7.OrderZeroAmount(bob, alice, LibOrder.hash(order));
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

        assertTrue(totalTakerInput.isZero(), "taker input");
        assertTrue(totalTakerOutput.isZero(), "taker output");
        _assertAliceVaults(0, LibDecimalFloat.FLOAT_ZERO);
    }

    /// A taken 0-ratio order fires `onTakeOrders2`.
    function testZeroRatioTakeFiresCallback(bool ioIsInput) external {
        _setUpVaultAndTransfers();
        RecordingOrderTaker taker = new RecordingOrderTaker(iRaindex);
        OrderV4 memory order = _order(5, "0");
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
        config.IOIsInput = ioIsInput;
        config.data = hex"01";

        taker.take(config);

        assertTrue(taker.called(), "onTakeOrders2 fires");
    }
}
