// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Vm} from "forge-std-1.17.0/src/Test.sol";
import {RaindexV7ExternalRealTest, IERC20} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    OrderV4,
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    IOV2,
    OrderConfigV4,
    EvaluableV4,
    TaskV3
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";

/// @title RaindexV7TakeOrderPrecisionTest
/// @notice A test harness for testing the Raindex takeOrder function.
contract RaindexV7TakeOrderPrecisionTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    function checkPrecision(
        bytes memory rainString,
        uint8 outputTokenDecimals,
        uint8 inputTokenDecimals,
        Float expectedTakerTotalInput,
        Float expectedTakerTotalOutput
    ) internal {
        checkPrecision(
            rainString,
            outputTokenDecimals,
            inputTokenDecimals,
            expectedTakerTotalInput,
            expectedTakerTotalOutput,
            bytes32(uint256(0x01)),
            bytes32(uint256(0x01))
        );
    }

    function checkPrecision(
        bytes memory rainString,
        uint8 outputTokenDecimals,
        uint8 inputTokenDecimals,
        Float expectedTakerTotalInput,
        Float expectedTakerTotalOutput,
        bytes32 outputVaultId,
        bytes32 inputVaultId
    ) internal {
        address inputToken = address(0x100);
        address outputToken = address(0x101);

        // Etch with invalid.
        vm.etch(outputToken, hex"fe");
        vm.etch(inputToken, hex"fe");

        vm.mockCall(
            inputToken, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(inputTokenDecimals)
        );
        vm.mockCall(
            outputToken, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(outputTokenDecimals)
        );

        {
            (uint256 absoluteDepositAmount, bool lossless) =
                expectedTakerTotalInput.toFixedDecimalLossy(outputTokenDecimals);
            if (!lossless) {
                ++absoluteDepositAmount;
            }
            if (outputVaultId == bytes32(0)) {
                mockVault0Output(outputToken, address(this), absoluteDepositAmount);
            } else {
                vm.mockCall(
                    outputToken,
                    abi.encodeWithSelector(
                        IERC20.transferFrom.selector, address(this), address(iRaindex), absoluteDepositAmount
                    ),
                    abi.encode(true)
                );
            }
        }

        {
            (uint256 absoluteTakerInputAmount, bool lossless) =
                expectedTakerTotalInput.toFixedDecimalLossy(outputTokenDecimals);
            (lossless);
            vm.mockCall(
                outputToken,
                abi.encodeWithSelector(IERC20.transfer.selector, address(this), absoluteTakerInputAmount),
                abi.encode(true)
            );
        }

        {
            (uint256 absoluteTakerOutputAmount, bool lossless) =
                expectedTakerTotalOutput.toFixedDecimalLossy(inputTokenDecimals);
            if (!lossless) {
                ++absoluteTakerOutputAmount;
            }
            vm.mockCall(
                inputToken,
                abi.encodeWithSelector(
                    IERC20.transferFrom.selector, address(this), address(iRaindex), absoluteTakerOutputAmount
                ),
                abi.encode(true)
            );
        }

        if (inputVaultId == bytes32(0)) {
            (uint256 absoluteInputAmount,) = expectedTakerTotalOutput.toFixedDecimalLossy(inputTokenDecimals);
            mockVault0Input(inputToken, address(this), absoluteInputAmount);
        }

        OrderConfigV4 memory config;
        {
            IOV2[] memory validInputs = new IOV2[](1);
            validInputs[0] = IOV2(inputToken, inputVaultId);
            IOV2[] memory validOutputs = new IOV2[](1);
            validOutputs[0] = IOV2(outputToken, outputVaultId);
            bytes memory bytecode = iParserV2.parse2(rainString);
            EvaluableV4 memory evaluable = EvaluableV4(iInterpreter, iStore, bytecode);
            config = OrderConfigV4(evaluable, validInputs, validOutputs, bytes32(0), bytes32(0), "");
        }

        {
            if (outputVaultId != bytes32(0) && expectedTakerTotalInput.gt(LibDecimalFloat.packLossless(0, 0))) {
                iRaindex.deposit5(outputToken, outputVaultId, expectedTakerTotalInput, new TaskV3[](0));
            }
            if (outputVaultId != bytes32(0)) {
                assertTrue(
                    iRaindex.vaultBalance2(address(this), outputToken, outputVaultId).eq(expectedTakerTotalInput)
                );
            }
            vm.recordLogs();
            iRaindex.addOrder5(config, new TaskV3[](0));
            Vm.Log[] memory entries = vm.getRecordedLogs();
            assertEq(entries.length, 1);
            OrderV4 memory order = LibTestTakeOrder.extractOrderFromLogs(entries);

            TakeOrdersConfigV6 memory takeOrdersConfig =
                LibTestTakeOrder.defaultTakeConfig(LibTestTakeOrder.wrapSingle(order));
            (Float totalTakerInput, Float totalTakerOutput) = iRaindex.takeOrders5(takeOrdersConfig);
            assertTrue(totalTakerInput.eq(expectedTakerTotalInput), "input");
            assertTrue(totalTakerOutput.eq(expectedTakerTotalOutput), "output");
        }

        if (outputVaultId != bytes32(0)) {
            assertTrue(iRaindex.vaultBalance2(address(this), outputToken, outputVaultId).isZero(), "vault balance");
        }
    }

    // Older versions of Raindex had precision issues with this IO setup.
    // bytes memory knownBad = "output-max io-ratio:157116.36568049186712991 0.000318235466963885;:;";
    bytes constant KNOWN_BAD = bytes("output-max io-ratio:157116.36568049186712991 0.000318235466963885;:;");

    function testTakeOrderPrecisionKnownBad01() public {
        checkPrecision(
            KNOWN_BAD,
            18,
            18,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35)
        );
    }

    function testTakeOrderPrecisionKnownBad02() public {
        checkPrecision(
            KNOWN_BAD,
            18,
            6,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35)
        );
    }

    function testTakeOrderPrecisionKnownBad03() public {
        checkPrecision(
            KNOWN_BAD,
            19,
            6,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35)
        );
    }

    function testTakeOrderPrecisionKnownBad04() public {
        checkPrecision(
            KNOWN_BAD,
            20,
            6,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35)
        );
    }

    function testTakeOrderPrecisionKnownBad05() public {
        checkPrecision(
            KNOWN_BAD,
            21,
            6,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35)
        );
    }

    function testTakeOrderPrecisionKnownBad06() public {
        checkPrecision(
            KNOWN_BAD,
            50,
            6,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35)
        );
    }

    function testTakeOrderPrecisionKnownBad07() public {
        checkPrecision(
            KNOWN_BAD,
            6,
            18,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35)
        );
    }

    function testTakeOrderPrecisionKnownBad08() public {
        checkPrecision(
            KNOWN_BAD,
            6,
            19,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35)
        );
    }

    function testTakeOrderPrecisionKnownBad09() public {
        checkPrecision(
            KNOWN_BAD,
            6,
            20,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35)
        );
    }

    function testTakeOrderPrecisionKnownBad10() public {
        checkPrecision(
            KNOWN_BAD,
            6,
            21,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35)
        );
    }

    function testTakeOrderPrecisionKnownBad11() public {
        checkPrecision(
            KNOWN_BAD,
            6,
            50,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35)
        );
    }

    function testTakeOrderPrecisionKnownBad01BothVaultIdZero() public {
        checkPrecision(
            KNOWN_BAD,
            18,
            18,
            LibDecimalFloat.packLossless(157116365680491867129910, -18),
            LibDecimalFloat.packLossless(4999999999999984457923789473657330035, -35),
            bytes32(0),
            bytes32(0)
        );
    }
}
