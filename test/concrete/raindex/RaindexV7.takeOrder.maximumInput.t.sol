// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest, Vm} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    OrderV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    IOV2,
    EvaluableV4,
    OrderConfigV4,
    TaskV3,
    IRaindexV7
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {SignedContextV2} from "rainlang-0.2.35/src/interface/IInterpreterCallerV5.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";

import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibFormatDecimalFloat} from "rain-math-float-0.3.11/src/lib/format/LibFormatDecimalFloat.sol";

contract RaindexV7TakeOrderMaximumInputTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;
    using LibFormatDecimalFloat for Float;

    /// If there is some live order(s) but the maxTakerInput is zero we error as
    /// the caller has full control over this, and it would cause none of the
    /// orders to be taken.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderNoopZeroMaxTakerInput(OrderV4 memory order, SignedContextV2 memory signedContext) external {
        vm.assume(order.validInputs.length > 0);
        vm.assume(order.validOutputs.length > 0);
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        SignedContextV2[] memory signedContexts = new SignedContextV2[](1);
        signedContexts[0] = signedContext;
        orders[0] = TakeOrderConfigV5(order, 0, 0, signedContexts);
        TakeOrdersConfigV6 memory config = TakeOrdersConfigV6({
            minimumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
            IOIsInput: true,
            orders: orders,
            data: ""
        });
        vm.expectRevert(IRaindexV7.ZeroMaximumIO.selector);
        (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);
        (totalTakerInput, totalTakerOutput);
    }

    struct TestOrder {
        address owner;
        bytes orderString;
    }

    struct TestVault {
        address owner;
        address token;
        bytes32 vaultId;
        Float deposit;
        Float expect;
    }

    function checkTakeOrderMaximumInput(
        TestOrder[] memory testOrders,
        TestVault[] memory testVaults,
        Float maximumTakerInput,
        Float expectedTakerInput,
        Float expectedTakerOutput,
        bytes32 outputVaultId,
        bytes32 inputVaultId
    ) internal {
        address bob = address(uint160(uint256(keccak256("bob.rain.test"))));

        OrderV4[] memory orders = new OrderV4[](testOrders.length);

        for (uint256 i = 0; i < testOrders.length; i++) {
            {
                OrderConfigV4 memory orderConfig;
                {
                    bytes memory bytecode = iParserV2.parse2(testOrders[i].orderString);
                    IOV2[] memory inputs = new IOV2[](1);
                    inputs[0] = IOV2(address(iToken0), inputVaultId);
                    IOV2[] memory outputs = new IOV2[](1);
                    outputs[0] = IOV2(address(iToken1), outputVaultId);
                    EvaluableV4 memory evaluable = EvaluableV4(iInterpreter, iStore, bytecode);
                    orderConfig = OrderConfigV4(evaluable, inputs, outputs, bytes32(0), bytes32(0), "");
                }

                vm.prank(testOrders[i].owner);
                vm.recordLogs();
                iRaindex.addOrder5(orderConfig, new TaskV3[](0));
                Vm.Log[] memory entries = vm.getRecordedLogs();
                assertEq(entries.length, 1);
                orders[i] = LibTestTakeOrder.extractOrderFromLogs(entries);
            }
        }

        for (uint256 i = 0; i < testVaults.length; i++) {
            if (testVaults[i].vaultId == bytes32(0)) {
                if (testVaults[i].deposit.gt(Float.wrap(0))) {
                    uint256 depositAmount18 = LibDecimalFloat.toFixedDecimalLossless(testVaults[i].deposit, 18);
                    mockVault0Output(testVaults[i].token, testVaults[i].owner, depositAmount18);
                } else {
                    (uint256 expectAmount18,) = LibDecimalFloat.toFixedDecimalLossy(testVaults[i].expect, 18);
                    mockVault0Input(testVaults[i].token, testVaults[i].owner, expectAmount18);
                }
            }
            if (testVaults[i].deposit.gt(Float.wrap(0))) {
                uint256 depositAmount18 = LibDecimalFloat.toFixedDecimalLossless(testVaults[i].deposit, 18);

                if (testVaults[i].vaultId != bytes32(0)) {
                    vm.mockCall(
                        address(iToken1),
                        abi.encodeWithSelector(
                            IERC20.transferFrom.selector, testVaults[i].owner, address(iRaindex), depositAmount18
                        ),
                        abi.encode(true)
                    );
                    vm.expectCall(
                        address(iToken1),
                        abi.encodeWithSelector(
                            IERC20.transferFrom.selector, testVaults[i].owner, address(iRaindex), depositAmount18
                        ),
                        1
                    );
                    Float balanceBefore =
                        iRaindex.vaultBalance2(testVaults[i].owner, testVaults[i].token, testVaults[i].vaultId);
                    vm.prank(testVaults[i].owner);
                    iRaindex.deposit5(
                        testVaults[i].token, testVaults[i].vaultId, testVaults[i].deposit, new TaskV3[](0)
                    );

                    Float balanceAfter =
                        iRaindex.vaultBalance2(testVaults[i].owner, testVaults[i].token, testVaults[i].vaultId);
                    Float expectedBalance = testVaults[i].deposit.add(balanceBefore);

                    assertTrue(balanceAfter.eq(expectedBalance), "vaultBalance after");
                }
            }
        }

        TakeOrderConfigV5[] memory takeOrders = new TakeOrderConfigV5[](orders.length);
        for (uint256 i = 0; i < orders.length; i++) {
            takeOrders[i] = TakeOrderConfigV5(orders[i], 0, 0, new SignedContextV2[](0));
        }
        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(takeOrders);
        config.maximumIO = maximumTakerInput;

        {
            (uint256 expectedTakerInput18,) = LibDecimalFloat.toFixedDecimalLossy(expectedTakerInput, 18);
            (uint256 expectedTakerOutput18, bool losslessOutput) =
                LibDecimalFloat.toFixedDecimalLossy(expectedTakerOutput, 18);
            if (!losslessOutput) {
                expectedTakerOutput18 += 1;
            }
            // Mock and expect the token transfers.
            vm.mockCall(
                address(iToken1),
                abi.encodeWithSelector(IERC20.transfer.selector, bob, expectedTakerInput18),
                abi.encode(true)
            );
            vm.expectCall(
                address(iToken1),
                abi.encodeWithSelector(IERC20.transfer.selector, bob, expectedTakerInput18),
                expectedTakerInput18 > 0 ? 1 : 0
            );
            vm.mockCall(
                address(iToken0),
                abi.encodeWithSelector(IERC20.transferFrom.selector, bob, address(iRaindex), expectedTakerOutput18),
                abi.encode(true)
            );
            vm.expectCall(
                address(iToken0),
                abi.encodeWithSelector(IERC20.transferFrom.selector, bob, address(iRaindex), expectedTakerOutput18),
                expectedTakerOutput18 > 0 ? 1 : 0
            );
        }
        {
            vm.prank(bob);
            (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);
            assertTrue(totalTakerInput.eq(expectedTakerInput), "totalTakerInput");
            assertTrue(totalTakerOutput.eq(expectedTakerOutput), "totalTakerOutput");
        }

        for (uint256 i = 0; i < testVaults.length; i++) {
            if (testVaults[i].vaultId != bytes32(0)) {
                Float vaultBalance =
                    iRaindex.vaultBalance2(testVaults[i].owner, testVaults[i].token, testVaults[i].vaultId);
                assertTrue(vaultBalance.eq(testVaults[i].expect), "vaultBalance");
            }
        }
    }

    /// Add an order with unlimited maximum output and take it with a maximum
    /// input. Only the maximum input should be taken.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumInputSingleOrderUnlimitedMax(uint256 expectedTakerInput18) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));

        expectedTakerInput18 = bound(expectedTakerInput18, 1, type(uint128).max);
        uint256 expectedTakerOutput18 = expectedTakerInput18 * 2;

        Float expectedTakerInput = LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerInput18, 18);
        Float expectedTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerOutput18, 18);

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder(owner, "_ _:max-positive-value() 2;:;");

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault(owner, address(iToken1), bytes32(uint256(0x01)), expectedTakerInput, Float.wrap(0));
        testVaults[1] = TestVault(owner, address(iToken0), bytes32(uint256(0x01)), Float.wrap(0), expectedTakerOutput);

        checkTakeOrderMaximumInput(
            testOrders,
            testVaults,
            expectedTakerInput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// Add an order with less than the maximum output. Only the limit from the
    /// order should be taken.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumInputSingleOrderLessThanMaximumOutput(uint256 maximumTakerInput18) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        maximumTakerInput18 = bound(maximumTakerInput18, 1000, uint256(int256(type(int224).max)));

        Float maximumTakerInput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerInput18, 18);

        Float expectedTakerInput = LibDecimalFloat.packLossless(1, -15);
        Float expectedTakerOutput = expectedTakerInput.mul(LibDecimalFloat.packLossless(2, 0));

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder(owner, "_ _:1e-15 2;:;");

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault(owner, address(iToken1), bytes32(uint256(0x01)), expectedTakerInput, Float.wrap(0));
        testVaults[1] = TestVault(owner, address(iToken0), bytes32(uint256(0x01)), Float.wrap(0), expectedTakerOutput);

        checkTakeOrderMaximumInput(
            testOrders,
            testVaults,
            maximumTakerInput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// If the vault balance is less than both the maximum input and the order
    /// limit, the vault balance should be taken.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumInputSingleOrderLessThanMaximumInput(
        uint256 ownerDepositAmount18,
        uint256 maximumTakerInput18
    ) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        uint256 orderLimit = 1000;
        ownerDepositAmount18 = bound(ownerDepositAmount18, 0, orderLimit - 1);
        maximumTakerInput18 = bound(maximumTakerInput18, orderLimit, uint256(int256(type(int224).max)));
        Float ownerDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerDepositAmount18, 18);
        Float maximumTakerInput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerInput18, 18);
        Float expectedTakerInput = ownerDepositAmount;
        Float expectedTakerOutput = expectedTakerInput.mul(LibDecimalFloat.packLossless(2, 0));

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder(owner, "_ _:1e-15 2;:;");

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault(owner, address(iToken1), bytes32(uint256(0x01)), ownerDepositAmount, Float.wrap(0));
        testVaults[1] = TestVault(owner, address(iToken0), bytes32(uint256(0x01)), Float.wrap(0), expectedTakerOutput);

        checkTakeOrderMaximumInput(
            testOrders,
            testVaults,
            maximumTakerInput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// The deposit amount can be anything actually, the order taking should
    /// adjust accordingly, and leave any unspent deposited tokens in the vault.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumInputSingleAnyDeposit(uint256 ownerDepositAmount18, uint256 maximumTakerInput18)
        external
    {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        uint256 orderLimit18 = 1000;

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder(owner, "_ _:1e-15 2;:;");

        ownerDepositAmount18 = bound(ownerDepositAmount18, 0, uint256(int256(type(int224).max)));
        maximumTakerInput18 = bound(maximumTakerInput18, 1, uint256(int256(type(int224).max)));
        // The expected input is the minimum of the maximum input and the order
        // limit.
        uint256 expectedTakerInput18 = maximumTakerInput18 < orderLimit18 ? maximumTakerInput18 : orderLimit18;

        expectedTakerInput18 = expectedTakerInput18 < ownerDepositAmount18 ? expectedTakerInput18 : ownerDepositAmount18;
        uint256 expectedTakerOutput18 = expectedTakerInput18 * 2;

        Float ownerDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerDepositAmount18, 18);
        Float maximumTakerInput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerInput18, 18);
        Float expectedTakerInput = LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerInput18, 18);
        Float expectedTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerOutput18, 18);

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault(
            owner,
            address(iToken1),
            bytes32(uint256(0x01)),
            ownerDepositAmount,
            ownerDepositAmount.sub(expectedTakerInput)
        );
        testVaults[1] = TestVault(
            owner, address(iToken0), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(0, 0), expectedTakerOutput
        );

        checkTakeOrderMaximumInput(
            testOrders,
            testVaults,
            maximumTakerInput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// The taker input can be sourced from multiple orders. Tests two orders
    /// that combined make up the maximum taker input. Both orders have the
    /// same owner.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumInputMultipleOrdersSingleOwner(
        uint256 ownerDepositAmount18,
        uint256 maximumTakerInput18
    ) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        uint256 orderLimit18 = 1500;

        TestOrder[] memory testOrders = new TestOrder[](2);
        testOrders[0] = TestOrder(owner, "_ _:1e-15 2;:;");
        testOrders[1] = TestOrder(owner, "_ _:5e-16 2;:;");

        ownerDepositAmount18 = bound(ownerDepositAmount18, 0, uint256(int256(type(int224).max)));
        maximumTakerInput18 = bound(maximumTakerInput18, 1, uint256(int256(type(int224).max)));
        // The expected input is the minimum of the maximum input and the order
        // limit.
        uint256 expectedTakerInput18 = maximumTakerInput18 < orderLimit18 ? maximumTakerInput18 : orderLimit18;

        expectedTakerInput18 = expectedTakerInput18 < ownerDepositAmount18 ? expectedTakerInput18 : ownerDepositAmount18;
        uint256 expectedTakerOutput18 = expectedTakerInput18 * 2;

        Float ownerDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerDepositAmount18, 18);
        Float maximumTakerInput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerInput18, 18);
        Float expectedTakerInput = LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerInput18, 18);
        Float expectedTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerOutput18, 18);

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault(
            owner,
            address(iToken1),
            bytes32(uint256(0x01)),
            ownerDepositAmount,
            ownerDepositAmount.sub(expectedTakerInput)
        );
        testVaults[1] = TestVault(
            owner, address(iToken0), bytes32(uint256(0x01)), LibDecimalFloat.packLossless(0, 0), expectedTakerOutput
        );

        checkTakeOrderMaximumInput(
            testOrders,
            testVaults,
            maximumTakerInput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// The taker input can be source from multiple orders with multiple owners.
    /// Tests two orders that combined make up the maximum taker input. Both
    /// orders have different owners.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumInputMultipleOrdersMultipleOwners(
        uint256 ownerOneDepositAmount18,
        uint256 ownerTwoDepositAmount18,
        uint256 maximumTakerInput18
    ) external {
        ownerOneDepositAmount18 = bound(ownerOneDepositAmount18, 0, uint256(int256(type(int224).max)) / 10);
        // Avoid information free overflow.
        ownerTwoDepositAmount18 =
            bound(ownerTwoDepositAmount18, 0, uint256(int256(type(int224).max)) / 10 - ownerOneDepositAmount18);
        maximumTakerInput18 = bound(maximumTakerInput18, 1, uint256(int256(type(int224).max)));

        address ownerOne = address(uint160(uint256(keccak256("ownerOne.rain.test"))));
        address ownerTwo = address(uint160(uint256(keccak256("ownerTwo.rain.test"))));

        TestOrder[] memory testOrders = new TestOrder[](2);
        testOrders[0] = TestOrder(ownerOne, "_ _:1e-15 2;:;");
        testOrders[1] = TestOrder(ownerTwo, "_ _:5e-16 2;:;");

        // The first owner's deposit is fully used before the second owner's
        // deposit is used.
        TestVault[] memory testVaults = new TestVault[](2);

        uint256 expectedTakerInput18;
        uint256 ownerOneTakerInput18;
        {
            // Owner one can't pay more than either their deposit or 1000 set in
            // the order.
            uint256 ownerOneMaxPayment18 = ownerOneDepositAmount18 < 1000 ? ownerOneDepositAmount18 : 1000;
            // taker input from owner one is either the maximum taker input if
            // it is less than the max owner one payment, or the max owner one
            // payment.
            ownerOneTakerInput18 =
                maximumTakerInput18 < ownerOneMaxPayment18 ? maximumTakerInput18 : ownerOneMaxPayment18;
            testVaults[0] = TestVault({
                owner: ownerOne,
                token: address(iToken1),
                vaultId: bytes32(uint256(0x01)),
                deposit: LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerOneDepositAmount18, 18),
                expect: LibDecimalFloat.fromFixedDecimalLosslessPacked(
                    ownerOneDepositAmount18 - ownerOneTakerInput18, 18
                )
            });
        }

        {
            // Owner two can't pay more than either their deposit or 500 set in
            // the order.
            uint256 ownerTwoMaxPayment18 = ownerTwoDepositAmount18 < 500 ? ownerTwoDepositAmount18 : 500;
            // Taker input from owner two is either whatever is remaining after
            // owner one's payment, or the max owner two payment.
            uint256 ownerTwoTakerInput18 =
                ownerOneTakerInput18 < maximumTakerInput18 ? maximumTakerInput18 - ownerOneTakerInput18 : 0;
            ownerTwoTakerInput18 =
                ownerTwoTakerInput18 < ownerTwoMaxPayment18 ? ownerTwoTakerInput18 : ownerTwoMaxPayment18;
            testVaults[1] = TestVault(
                ownerTwo,
                address(iToken1),
                bytes32(uint256(0x01)),
                LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerTwoDepositAmount18, 18),
                LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerTwoDepositAmount18 - ownerTwoTakerInput18, 18)
            );
            expectedTakerInput18 = ownerOneTakerInput18 + ownerTwoTakerInput18;
        }
        uint256 expectedTakerOutput18 = expectedTakerInput18 * 2;

        checkTakeOrderMaximumInput(
            testOrders,
            testVaults,
            LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerInput18, 18),
            LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerInput18, 18),
            LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerOutput18, 18),
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// Same as testTakeOrderMaximumInputSingleAnyDeposit but with both vault
    /// IDs set to 0 (fully vaultless).
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumInputSingleAnyDepositBothVaultIdZero(
        uint256 ownerDepositAmount18,
        uint256 maximumTakerInput18
    ) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        uint256 orderLimit18 = 1000;

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder(owner, "_ _:1e-15 2;:;");

        ownerDepositAmount18 = bound(ownerDepositAmount18, 0, uint256(int256(type(int224).max)));
        maximumTakerInput18 = bound(maximumTakerInput18, 1, uint256(int256(type(int224).max)));

        uint256 expectedTakerInput18 = maximumTakerInput18 < orderLimit18 ? maximumTakerInput18 : orderLimit18;
        expectedTakerInput18 = expectedTakerInput18 < ownerDepositAmount18 ? expectedTakerInput18 : ownerDepositAmount18;
        uint256 expectedTakerOutput18 = expectedTakerInput18 * 2;

        Float ownerDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerDepositAmount18, 18);
        Float maximumTakerInput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerInput18, 18);
        Float expectedTakerInput = LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerInput18, 18);
        Float expectedTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerOutput18, 18);

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault(
            owner, address(iToken1), bytes32(0), ownerDepositAmount, ownerDepositAmount.sub(expectedTakerInput)
        );
        testVaults[1] = TestVault(owner, address(iToken0), bytes32(0), Float.wrap(0), expectedTakerOutput);

        checkTakeOrderMaximumInput(
            testOrders, testVaults, maximumTakerInput, expectedTakerInput, expectedTakerOutput, bytes32(0), bytes32(0)
        );
    }
}
