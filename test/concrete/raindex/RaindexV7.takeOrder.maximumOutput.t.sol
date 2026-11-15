// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest, Vm} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    OrderV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    SignedContextV2,
    IRaindexV7,
    OrderConfigV4,
    IOV2,
    EvaluableV4,
    TaskV3
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";
import {LibDecimalFloat, Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

contract RaindexV7TakeOrderMaximumOutputTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    /// It should be possible to take an order with zero maximum output.
    function testTakeOrderMaximumOutputZero(OrderV4 memory order, SignedContextV2 memory signedContext) external {
        vm.assume(order.validInputs.length > 0);
        vm.assume(order.validOutputs.length > 0);
        order.validInputs[0].token = address(iToken0);
        order.validOutputs[0].token = address(iToken1);
        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        SignedContextV2[] memory signedContexts = new SignedContextV2[](1);
        signedContexts[0] = signedContext;
        orders[0] = TakeOrderConfigV5({order: order, inputIOIndex: 0, outputIOIndex: 0, signedContext: signedContexts});
        TakeOrdersConfigV6 memory config = TakeOrdersConfigV6({
            orders: orders,
            minimumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIO: LibDecimalFloat.packLossless(0, 0),
            maximumIORatio: LibDecimalFloat.packLossless(1, 0),
            IOIsInput: false,
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

    function checkTakeOrderMaximumOutput(
        TestOrder[] memory testOrders,
        TestVault[] memory testVaults,
        Float maximumTakerOutput,
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
                    inputs[0] = IOV2({token: address(iToken0), vaultId: inputVaultId});
                    IOV2[] memory outputs = new IOV2[](1);
                    outputs[0] = IOV2({token: address(iToken1), vaultId: outputVaultId});
                    EvaluableV4 memory evaluable =
                        EvaluableV4({interpreter: iInterpreter, store: iStore, bytecode: bytecode});
                    orderConfig = OrderConfigV4({
                        evaluable: evaluable,
                        validInputs: inputs,
                        validOutputs: outputs,
                        nonce: bytes32(0),
                        secret: bytes32(0),
                        meta: ""
                    });
                }

                vm.prank(testOrders[i].owner);
                vm.recordLogs();
                iRaindex.addOrder5(orderConfig, new TaskV3[](0));
                Vm.Log[] memory logs = vm.getRecordedLogs();
                assertEq(logs.length, 1);
                orders[i] = LibTestTakeOrder.extractOrderFromLogs(logs);
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
                    // Deposit the amount of tokens required to take the order.
                    vm.mockCall(
                        address(iToken1),
                        abi.encodeWithSelector(
                            iToken1.transferFrom.selector, testVaults[i].owner, address(iRaindex), depositAmount18
                        ),
                        abi.encode(true)
                    );
                    vm.expectCall(
                        address(iToken1),
                        abi.encodeWithSelector(
                            iToken1.transferFrom.selector, testVaults[i].owner, address(iRaindex), depositAmount18
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

                    assertTrue(balanceAfter.eq(expectedBalance), "deposit");
                }
            }
        }

        TakeOrderConfigV5[] memory takeOrders = new TakeOrderConfigV5[](orders.length);
        for (uint256 i = 0; i < orders.length; i++) {
            takeOrders[i] = TakeOrderConfigV5({
                order: orders[i], inputIOIndex: 0, outputIOIndex: 0, signedContext: new SignedContextV2[](0)
            });
        }

        TakeOrdersConfigV6 memory config = LibTestTakeOrder.defaultTakeConfig(takeOrders);
        config.maximumIO = maximumTakerOutput;
        config.IOIsInput = false;

        {
            (uint256 expectedTakerInput18,) = LibDecimalFloat.toFixedDecimalLossy(expectedTakerInput, 18);
            (uint256 expectedTakerOutput18, bool losslessOutput) =
                LibDecimalFloat.toFixedDecimalLossy(expectedTakerOutput, 18);
            if (!losslessOutput) {
                expectedTakerOutput18 += 1;
            }
            // Mock and expect the token transfers.
            vm.mockCall(
                address(iToken0),
                abi.encodeWithSelector(iToken0.transferFrom.selector, bob, address(iRaindex), expectedTakerOutput18),
                abi.encode(true)
            );
            vm.expectCall(
                address(iToken0),
                abi.encodeWithSelector(iToken0.transferFrom.selector, bob, address(iRaindex), expectedTakerOutput18),
                expectedTakerOutput18 > 0 ? 1 : 0
            );
            vm.mockCall(
                address(iToken1),
                abi.encodeWithSelector(iToken1.transfer.selector, bob, expectedTakerInput18),
                abi.encode(true)
            );
            vm.expectCall(
                address(iToken1),
                abi.encodeWithSelector(iToken1.transfer.selector, bob, expectedTakerInput18),
                expectedTakerInput18 > 0 ? 1 : 0
            );
        }
        {
            vm.prank(bob);
            (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(config);

            assertTrue(totalTakerInput.eq(expectedTakerInput), "taker input");
            assertTrue(totalTakerOutput.eq(expectedTakerOutput), "taker output");
        }

        for (uint256 i = 0; i < testVaults.length; i++) {
            // Vault 0 balance falls back to the owner's token balance,
            // which is mocked and doesn't reflect post-trade state.
            // Correctness for vault 0 is covered by the taker input/output
            // assertions above.
            if (testVaults[i].vaultId != bytes32(0)) {
                Float finalBalance =
                    iRaindex.vaultBalance2(testVaults[i].owner, testVaults[i].token, testVaults[i].vaultId);
                Float expectedFinalBalance = testVaults[i].expect;
                assertTrue(finalBalance.eq(expectedFinalBalance), "final balance");
            }
        }
    }

    /// Add an order with unlimited maximum output and take it with a maximum
    /// taker output. Only the maximum taker output should be used.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumOutputSingleOrderUnlimitedMax(uint256 expectedTakerOutput18) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));

        expectedTakerOutput18 = bound(expectedTakerOutput18, 1, type(uint128).max);
        uint256 expectedTakerInput18 = expectedTakerOutput18 * 2;

        Float expectedTakerInput = LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerInput18, 18);
        Float expectedTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(expectedTakerOutput18, 18);

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder({owner: owner, orderString: "_ _: max-positive-value() 0.5;:;"});

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault({
            owner: owner,
            token: address(iToken1),
            vaultId: bytes32(uint256(0x01)),
            deposit: expectedTakerInput,
            expect: Float.wrap(0)
        });
        testVaults[1] = TestVault({
            owner: owner,
            token: address(iToken0),
            vaultId: bytes32(uint256(0x01)),
            deposit: Float.wrap(0),
            expect: expectedTakerOutput
        });

        checkTakeOrderMaximumOutput(
            testOrders,
            testVaults,
            expectedTakerInput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// Add an order with less than the maximum IO. Only the limit from the order
    /// should be used.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumOutputSingleOrderLimitedMax(uint256 maximumTakerOutput18) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        maximumTakerOutput18 = bound(maximumTakerOutput18, 1e18, uint256(int256(type(int224).max)));

        Float maximumTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerOutput18, 18);

        Float expectedTakerInput = LibDecimalFloat.packLossless(5, -2);
        Float expectedTakerOutput = expectedTakerInput.mul(LibDecimalFloat.packLossless(2, 0));

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder({owner: owner, orderString: "_ _: 0.05 2;:;"});

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault({
            owner: owner,
            token: address(iToken1),
            vaultId: bytes32(uint256(0x01)),
            deposit: expectedTakerInput,
            expect: Float.wrap(0)
        });
        testVaults[1] = TestVault({
            owner: owner,
            token: address(iToken0),
            vaultId: bytes32(uint256(0x01)),
            deposit: Float.wrap(0),
            expect: expectedTakerOutput
        });

        checkTakeOrderMaximumOutput(
            testOrders,
            testVaults,
            maximumTakerOutput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// If the vault balance is less than both the maximum output and the order
    /// limit, only the vault balance should be used.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumOutputSingleOrderLimitedByVault(
        uint256 ownerDepositAmount18,
        uint256 maximumTakerInput18
    ) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));

        uint256 orderLimit18 = 1000;
        ownerDepositAmount18 = bound(ownerDepositAmount18, 0, orderLimit18 - 1);
        maximumTakerInput18 = bound(maximumTakerInput18, orderLimit18, uint256(int256(type(int224).max)));

        Float ownerDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerDepositAmount18, 18);
        Float maximumTakerInput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerInput18, 18);

        Float orderIORatio = LibDecimalFloat.fromFixedDecimalLosslessPacked(2, 0);
        Float maximumTakerOutput = maximumTakerInput.mul(orderIORatio);

        Float expectedTakerInput = ownerDepositAmount;
        Float expectedTakerOutput = expectedTakerInput.mul(orderIORatio);

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder({owner: owner, orderString: "_ _: 1000e-18 2;:;"});

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault({
            owner: owner,
            token: address(iToken1),
            vaultId: bytes32(uint256(0x01)),
            deposit: ownerDepositAmount,
            expect: Float.wrap(0)
        });
        testVaults[1] = TestVault({
            owner: owner,
            token: address(iToken0),
            vaultId: bytes32(uint256(0x01)),
            deposit: Float.wrap(0),
            expect: expectedTakerOutput
        });
        checkTakeOrderMaximumOutput(
            testOrders,
            testVaults,
            maximumTakerOutput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// The deposit amount can be anything, the order taking should adjust
    /// accordingly, and leave any unspent deposited tokens in the vault.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumOutputSingleAnyDeposit(uint256 ownerDepositAmount18, uint256 maximumTakerOutput18)
        external
    {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        uint256 orderLimit18 = 1000;

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder({owner: owner, orderString: "_ _: 1000e-18 2;:;"});

        ownerDepositAmount18 = bound(ownerDepositAmount18, 0, uint256(int256(type(int224).max)));
        maximumTakerOutput18 = bound(maximumTakerOutput18, 1, uint256(int256(type(int224).max)));

        Float orderIO = LibDecimalFloat.fromFixedDecimalLosslessPacked(2, 0);
        Float orderLimit = LibDecimalFloat.fromFixedDecimalLosslessPacked(orderLimit18, 18);
        Float ownerDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerDepositAmount18, 18);
        Float maximumTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerOutput18, 18);
        Float maximumTakerInput = maximumTakerOutput.div(orderIO);

        Float expectedTakerInput = maximumTakerInput.min(ownerDepositAmount);
        expectedTakerInput = expectedTakerInput.min(orderLimit);

        Float expectedTakerOutput = expectedTakerInput.mul(orderIO);

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault({
            owner: owner,
            token: address(iToken1),
            vaultId: bytes32(uint256(0x01)),
            deposit: ownerDepositAmount,
            expect: ownerDepositAmount.sub(expectedTakerInput)
        });
        testVaults[1] = TestVault({
            owner: owner,
            token: address(iToken0),
            vaultId: bytes32(uint256(0x01)),
            deposit: Float.wrap(0),
            expect: expectedTakerOutput
        });
        checkTakeOrderMaximumOutput(
            testOrders,
            testVaults,
            maximumTakerOutput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// Same as testTakeOrderMaximumOutputSingleAnyDeposit but with output
    /// vault ID 0 (vaultless).
    function testTakeOrderMaximumOutputSingleAnyDepositOutputVaultIdZero(
        uint256 ownerDepositAmount18,
        uint256 maximumTakerOutput18
    ) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        uint256 orderLimit18 = 1000;

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder({owner: owner, orderString: "_ _: 1000e-18 2;:;"});

        ownerDepositAmount18 = bound(ownerDepositAmount18, 0, uint256(int256(type(int224).max)));
        maximumTakerOutput18 = bound(maximumTakerOutput18, 1, uint256(int256(type(int224).max)));

        Float orderIO = LibDecimalFloat.fromFixedDecimalLosslessPacked(2, 0);
        Float orderLimit = LibDecimalFloat.fromFixedDecimalLosslessPacked(orderLimit18, 18);
        Float ownerDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerDepositAmount18, 18);
        Float maximumTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerOutput18, 18);
        Float maximumTakerInput = maximumTakerOutput.div(orderIO);

        Float expectedTakerInput = maximumTakerInput.min(ownerDepositAmount);
        expectedTakerInput = expectedTakerInput.min(orderLimit);

        Float expectedTakerOutput = expectedTakerInput.mul(orderIO);

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault({
            owner: owner,
            token: address(iToken1),
            vaultId: bytes32(0),
            deposit: ownerDepositAmount,
            expect: ownerDepositAmount.sub(expectedTakerInput)
        });
        testVaults[1] = TestVault({
            owner: owner,
            token: address(iToken0),
            vaultId: bytes32(uint256(0x01)),
            deposit: Float.wrap(0),
            expect: expectedTakerOutput
        });
        checkTakeOrderMaximumOutput(
            testOrders,
            testVaults,
            maximumTakerOutput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(0),
            bytes32(uint256(0x01))
        );
    }

    /// Same as testTakeOrderMaximumOutputSingleAnyDeposit but with input
    /// vault ID 0 (vaultless). The order owner receives tokens directly to
    /// their wallet via pushTokens instead of internal vault accounting.
    function testTakeOrderMaximumOutputSingleAnyDepositInputVaultIdZero(
        uint256 ownerDepositAmount18,
        uint256 maximumTakerOutput18
    ) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        uint256 orderLimit18 = 1000;

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder({owner: owner, orderString: "_ _: 1000e-18 2;:;"});

        ownerDepositAmount18 = bound(ownerDepositAmount18, 0, uint256(int256(type(int224).max)));
        maximumTakerOutput18 = bound(maximumTakerOutput18, 1, uint256(int256(type(int224).max)));

        Float orderIO = LibDecimalFloat.fromFixedDecimalLosslessPacked(2, 0);
        Float orderLimit = LibDecimalFloat.fromFixedDecimalLosslessPacked(orderLimit18, 18);
        Float ownerDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerDepositAmount18, 18);
        Float maximumTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerOutput18, 18);
        Float maximumTakerInput = maximumTakerOutput.div(orderIO);

        Float expectedTakerInput = maximumTakerInput.min(ownerDepositAmount);
        expectedTakerInput = expectedTakerInput.min(orderLimit);

        Float expectedTakerOutput = expectedTakerInput.mul(orderIO);

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault({
            owner: owner,
            token: address(iToken1),
            vaultId: bytes32(uint256(0x01)),
            deposit: ownerDepositAmount,
            expect: ownerDepositAmount.sub(expectedTakerInput)
        });
        testVaults[1] = TestVault({
            owner: owner,
            token: address(iToken0),
            vaultId: bytes32(0),
            deposit: Float.wrap(0),
            expect: expectedTakerOutput
        });
        checkTakeOrderMaximumOutput(
            testOrders,
            testVaults,
            maximumTakerOutput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(0)
        );
    }

    /// Same as testTakeOrderMaximumOutputSingleAnyDeposit but with both
    /// input and output vault IDs set to 0 (fully vaultless). The order
    /// owner sells from and receives to their wallet directly.
    function testTakeOrderMaximumOutputSingleAnyDepositBothVaultIdZero(
        uint256 ownerDepositAmount18,
        uint256 maximumTakerOutput18
    ) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        uint256 orderLimit18 = 1000;

        TestOrder[] memory testOrders = new TestOrder[](1);
        testOrders[0] = TestOrder({owner: owner, orderString: "_ _: 1000e-18 2;:;"});

        ownerDepositAmount18 = bound(ownerDepositAmount18, 0, uint256(int256(type(int224).max)));
        maximumTakerOutput18 = bound(maximumTakerOutput18, 1, uint256(int256(type(int224).max)));

        Float orderIO = LibDecimalFloat.fromFixedDecimalLosslessPacked(2, 0);
        Float orderLimit = LibDecimalFloat.fromFixedDecimalLosslessPacked(orderLimit18, 18);
        Float ownerDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerDepositAmount18, 18);
        Float maximumTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerOutput18, 18);
        Float maximumTakerInput = maximumTakerOutput.div(orderIO);

        Float expectedTakerInput = maximumTakerInput.min(ownerDepositAmount);
        expectedTakerInput = expectedTakerInput.min(orderLimit);

        Float expectedTakerOutput = expectedTakerInput.mul(orderIO);

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault({
            owner: owner,
            token: address(iToken1),
            vaultId: bytes32(0),
            deposit: ownerDepositAmount,
            expect: ownerDepositAmount.sub(expectedTakerInput)
        });
        testVaults[1] = TestVault({
            owner: owner,
            token: address(iToken0),
            vaultId: bytes32(0),
            deposit: Float.wrap(0),
            expect: expectedTakerOutput
        });
        checkTakeOrderMaximumOutput(
            testOrders, testVaults, maximumTakerOutput, expectedTakerInput, expectedTakerOutput, bytes32(0), bytes32(0)
        );
    }

    /// Same as testTakeOrderMaximumOutputMultipleOrders but with both vault IDs
    /// set to 0 (fully vaultless). With vault 0 and constant balanceOf mock,
    /// each order independently sees the full deposit, so per-order amounts
    /// must be calculated separately.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumOutputMultipleOrdersBothVaultIdZero(
        uint256 ownerDepositAmount18,
        uint256 maximumTakerOutput18
    ) external {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));

        TestOrder[] memory testOrders = new TestOrder[](2);
        testOrders[0] = TestOrder({owner: owner, orderString: "_ _: 1000e-18 2;:;"});
        testOrders[1] = TestOrder({owner: owner, orderString: "_ _: 500e-18 2;:;"});

        ownerDepositAmount18 = bound(ownerDepositAmount18, 0, uint256(int256(type(int224).max)));
        maximumTakerOutput18 = bound(maximumTakerOutput18, 1, uint256(int256(type(int224).max)));

        Float orderIO = LibDecimalFloat.fromFixedDecimalLosslessPacked(2, 0);
        Float ownerDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerDepositAmount18, 18);
        Float maximumTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerOutput18, 18);

        // Same-owner vault-0 input credits are netted into a single push, so one
        // input vault mock covers both orders' input.
        TestVault[] memory testVaults = new TestVault[](2);
        Float expectedTakerInput;
        {
            Float maximumTakerInput = maximumTakerOutput.div(orderIO);

            // With vault 0 and constant balanceOf mock, each order
            // independently sees the full deposit as available balance.
            Float order0Take;
            {
                Float order0Limit = LibDecimalFloat.fromFixedDecimalLosslessPacked(1000, 18);
                order0Take = maximumTakerInput.min(order0Limit.min(ownerDepositAmount));
            }
            Float order1Take;
            {
                Float order1Limit = LibDecimalFloat.fromFixedDecimalLosslessPacked(500, 18);
                Float remaining = maximumTakerInput.sub(order0Take);
                order1Take = remaining.min(order1Limit.min(ownerDepositAmount));
            }

            expectedTakerInput = order0Take.add(order1Take);

            testVaults[0] = TestVault({
                owner: owner,
                token: address(iToken1),
                vaultId: bytes32(0),
                deposit: ownerDepositAmount,
                expect: ownerDepositAmount.sub(expectedTakerInput)
            });
            // Both orders share the owner and input token, so their vault-0 input
            // credits accrue together and settle as one netted push.
            testVaults[1] = TestVault({
                owner: owner,
                token: address(iToken0),
                vaultId: bytes32(0),
                deposit: Float.wrap(0),
                expect: order0Take.mul(orderIO).add(order1Take.mul(orderIO))
            });
        }

        Float expectedTakerOutput = expectedTakerInput.mul(orderIO);
        checkTakeOrderMaximumOutput(
            testOrders, testVaults, maximumTakerOutput, expectedTakerInput, expectedTakerOutput, bytes32(0), bytes32(0)
        );
    }

    /// The taker input can be sourced from multiple orders. Tests two orders
    /// that combined make up the maximum taker output. Both orders have the same
    /// owner for simplicity.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumOutputMultipleOrders(uint256 ownerDepositAmount18, uint256 maximumTakerOutput18)
        external
    {
        address owner = address(uint160(uint256(keccak256("owner.rain.test"))));
        uint256 orderLimit18 = 1500;

        TestOrder[] memory testOrders = new TestOrder[](2);
        testOrders[0] = TestOrder({owner: owner, orderString: "_ _: 1000e-18 2;:;"});
        testOrders[1] = TestOrder({owner: owner, orderString: "_ _: 500e-18 2;:;"});

        ownerDepositAmount18 = bound(ownerDepositAmount18, 0, uint256(int256(type(int224).max)));
        maximumTakerOutput18 = bound(maximumTakerOutput18, 1, uint256(int256(type(int224).max)));

        Float orderIO = LibDecimalFloat.fromFixedDecimalLosslessPacked(2, 0);
        Float orderLimit = LibDecimalFloat.fromFixedDecimalLosslessPacked(orderLimit18, 18);
        Float ownerDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerDepositAmount18, 18);
        Float maximumTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerOutput18, 18);
        Float maximumTakerInput = maximumTakerOutput.div(orderIO);

        Float expectedTakerInput = maximumTakerInput.min(ownerDepositAmount);
        expectedTakerInput = expectedTakerInput.min(orderLimit);
        Float expectedTakerOutput = expectedTakerInput.mul(orderIO);

        TestVault[] memory testVaults = new TestVault[](2);
        testVaults[0] = TestVault({
            owner: owner,
            token: address(iToken1),
            vaultId: bytes32(uint256(0x01)),
            deposit: ownerDepositAmount,
            expect: ownerDepositAmount.sub(expectedTakerInput)
        });
        testVaults[1] = TestVault({
            owner: owner,
            token: address(iToken0),
            vaultId: bytes32(uint256(0x01)),
            deposit: Float.wrap(0),
            expect: expectedTakerOutput
        });

        checkTakeOrderMaximumOutput(
            testOrders,
            testVaults,
            maximumTakerOutput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// The taker input can be sourced from multiple orders with different
    /// owners. Tests two orders that combined make up the maximum taker output.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumOutputMultipleOrdersDifferentOwners(
        uint256 ownerOneDepositAmount18,
        uint256 ownerTwoDepositAmount18,
        uint256 maximumTakerOutput18
    ) external {
        address ownerOne = address(uint160(uint256(keccak256("owner.one.rain.test"))));
        address ownerTwo = address(uint160(uint256(keccak256("owner.two.rain.test"))));

        ownerOneDepositAmount18 = bound(ownerOneDepositAmount18, 0, uint256(int256(type(int224).max)));
        ownerTwoDepositAmount18 =
            bound(ownerTwoDepositAmount18, 0, uint256(int256(type(int224).max)) - ownerOneDepositAmount18);
        maximumTakerOutput18 = bound(maximumTakerOutput18, 1, uint256(int256(type(int224).max)));

        TestOrder[] memory testOrders = new TestOrder[](2);
        testOrders[0] = TestOrder({owner: ownerOne, orderString: "_ _: 1000e-18 2;:;"});
        testOrders[1] = TestOrder({owner: ownerTwo, orderString: "_ _: 500e-18 2;:;"});

        // The first owner's deposit is fully used before the second owner's
        // deposit is used.
        TestVault[] memory testVaults = new TestVault[](2);

        Float ownerOneDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerOneDepositAmount18, 18);
        Float ownerTwoDepositAmount = LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerTwoDepositAmount18, 18);
        Float ownerTwoOrderLimit = LibDecimalFloat.fromFixedDecimalLosslessPacked(500, 18);
        Float orderIO = LibDecimalFloat.fromFixedDecimalLosslessPacked(2, 0);
        Float maximumTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerOutput18, 18);
        Float maximumTakerInput = maximumTakerOutput.div(orderIO);

        Float expectedTakerInput;
        Float ownerOneTakerInput;
        {
            Float ownerOneOrderLimit = LibDecimalFloat.fromFixedDecimalLosslessPacked(1000, 18);

            // Owner one can't pay more than either their deposit or their order
            // limit.
            Float ownerOneMaxPayment = ownerOneDepositAmount.min(ownerOneOrderLimit);
            // Taker input from owner one is either the maximum taker input or
            // what owner one can pay.
            ownerOneTakerInput = maximumTakerInput.min(ownerOneMaxPayment);
            expectedTakerInput = ownerOneTakerInput;
            testVaults[0] = TestVault({
                owner: ownerOne,
                token: address(iToken1),
                vaultId: bytes32(uint256(0x01)),
                deposit: ownerOneDepositAmount,
                expect: ownerOneDepositAmount.sub(ownerOneTakerInput)
            });
        }

        {
            // Owner two can't pay more than either their deposit or their order
            // limit.
            Float ownerTwoMaxPayment = ownerTwoDepositAmount.min(ownerTwoOrderLimit);
            // Taker input from owner two is whatever is remaining after owner
            // one, up to what owner two can pay.
            Float remainingTakerInput = maximumTakerInput.sub(ownerOneTakerInput);
            Float ownerTwoTakerInput = remainingTakerInput.min(ownerTwoMaxPayment);
            expectedTakerInput = expectedTakerInput.add(ownerTwoTakerInput);
            testVaults[1] = TestVault({
                owner: ownerTwo,
                token: address(iToken1),
                vaultId: bytes32(uint256(0x01)),
                deposit: ownerTwoDepositAmount,
                expect: ownerTwoDepositAmount.sub(ownerTwoTakerInput)
            });
        }
        Float expectedTakerOutput = expectedTakerInput.mul(orderIO);
        checkTakeOrderMaximumOutput(
            testOrders,
            testVaults,
            maximumTakerOutput,
            expectedTakerInput,
            expectedTakerOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    /// Same as testTakeOrderMaximumOutputMultipleOrdersDifferentOwners but with
    /// both vault IDs set to 0 (fully vaultless). Each owner sells from and
    /// receives to their wallet directly.
    /// forge-config: default.fuzz.runs = 100
    function testTakeOrderMaximumOutputMultipleOrdersDifferentOwnersBothVaultIdZero(
        uint256 ownerOneDepositAmount18,
        uint256 ownerTwoDepositAmount18,
        uint256 maximumTakerOutput18
    ) external {
        ownerOneDepositAmount18 = bound(ownerOneDepositAmount18, 0, uint256(int256(type(int224).max)));
        ownerTwoDepositAmount18 =
            bound(ownerTwoDepositAmount18, 0, uint256(int256(type(int224).max)) - ownerOneDepositAmount18);
        maximumTakerOutput18 = bound(maximumTakerOutput18, 1, uint256(int256(type(int224).max)));

        TestOrder[] memory testOrders = new TestOrder[](2);
        TestVault[] memory testVaults = new TestVault[](4);
        Float expectedTakerInput;
        Float expectedTakerOutput;

        {
            Float orderIO = LibDecimalFloat.fromFixedDecimalLosslessPacked(2, 0);
            Float maximumTakerInput =
                LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerOutput18, 18).div(orderIO);
            Float ownerOneTakerInput;

            {
                address ownerOne = address(uint160(uint256(keccak256("owner.one.rain.test"))));
                testOrders[0] = TestOrder({owner: ownerOne, orderString: "_ _: 1000e-18 2;:;"});
                Float ownerOneDepositAmount =
                    LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerOneDepositAmount18, 18);
                ownerOneTakerInput = maximumTakerInput.min(
                    ownerOneDepositAmount.min(LibDecimalFloat.fromFixedDecimalLosslessPacked(1000, 18))
                );
                expectedTakerInput = ownerOneTakerInput;
                testVaults[0] = TestVault({
                    owner: ownerOne,
                    token: address(iToken1),
                    vaultId: bytes32(0),
                    deposit: ownerOneDepositAmount,
                    expect: ownerOneDepositAmount.sub(ownerOneTakerInput)
                });
                testVaults[2] = TestVault({
                    owner: ownerOne,
                    token: address(iToken0),
                    vaultId: bytes32(0),
                    deposit: Float.wrap(0),
                    expect: ownerOneTakerInput.mul(orderIO)
                });
            }

            {
                address ownerTwo = address(uint160(uint256(keccak256("owner.two.rain.test"))));
                testOrders[1] = TestOrder({owner: ownerTwo, orderString: "_ _: 500e-18 2;:;"});
                Float ownerTwoDepositAmount =
                    LibDecimalFloat.fromFixedDecimalLosslessPacked(ownerTwoDepositAmount18, 18);
                Float ownerTwoTakerInput = (maximumTakerInput.sub(ownerOneTakerInput))
                .min(ownerTwoDepositAmount.min(LibDecimalFloat.fromFixedDecimalLosslessPacked(500, 18)));
                expectedTakerInput = expectedTakerInput.add(ownerTwoTakerInput);
                testVaults[1] = TestVault({
                    owner: ownerTwo,
                    token: address(iToken1),
                    vaultId: bytes32(0),
                    deposit: ownerTwoDepositAmount,
                    expect: ownerTwoDepositAmount.sub(ownerTwoTakerInput)
                });
                testVaults[3] = TestVault({
                    owner: ownerTwo,
                    token: address(iToken0),
                    vaultId: bytes32(0),
                    deposit: Float.wrap(0),
                    expect: ownerTwoTakerInput.mul(orderIO)
                });
            }

            expectedTakerOutput = expectedTakerInput.mul(orderIO);
        }

        Float maximumTakerOutput = LibDecimalFloat.fromFixedDecimalLosslessPacked(maximumTakerOutput18, 18);
        checkTakeOrderMaximumOutput(
            testOrders, testVaults, maximumTakerOutput, expectedTakerInput, expectedTakerOutput, bytes32(0), bytes32(0)
        );
    }
}
