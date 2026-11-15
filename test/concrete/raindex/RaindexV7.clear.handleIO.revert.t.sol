// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Vm} from "forge-std-1.17.0/src/Vm.sol";
import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    ClearConfigV2,
    OrderV4,
    IOV2,
    OrderConfigV4,
    EvaluableV4,
    SignedContextV2,
    TaskV3,
    Float
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibTestTakeOrder} from "test/util/lib/LibTestTakeOrder.sol";

/// @title RaindexV7ClearHandleIORevertTest
/// @notice A test harness for testing the Raindex clear function will run
/// handle IO and revert if it fails.
contract RaindexV7ClearHandleIORevertTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    function userDeposit(bytes memory rainString, address owner, address inputToken, address outputToken)
        internal
        returns (OrderV4 memory)
    {
        return userDeposit(rainString, owner, inputToken, outputToken, bytes32(uint256(0x01)), bytes32(uint256(0x01)));
    }

    function userDeposit(
        bytes memory rainString,
        address owner,
        address inputToken,
        address outputToken,
        bytes32 outputVaultId,
        bytes32 inputVaultId
    ) internal returns (OrderV4 memory) {
        bytes32 vaultId = outputVaultId;

        OrderConfigV4 memory config;
        IOV2[] memory validOutputs;
        IOV2[] memory validInputs;
        {
            validInputs = new IOV2[](1);
            validInputs[0] = IOV2(inputToken, inputVaultId);
            validOutputs = new IOV2[](1);
            validOutputs[0] = IOV2(outputToken, outputVaultId);
            // Etch with invalid.
            vm.etch(inputToken, hex"fe");
            vm.etch(outputToken, hex"fe");
            // Mock every call to output as a success, so the raindex thinks it
            // is transferring tokens.
            vm.mockCall(inputToken, bytes(""), abi.encode(true));
            vm.mockCall(outputToken, bytes(""), abi.encode(true));
        }

        if (outputVaultId == bytes32(0)) {
            mockVault0Output(outputToken, owner, uint256(int256(type(int224).max)));
        } else {
            vm.prank(owner);
            iRaindex.deposit5(outputToken, vaultId, LibDecimalFloat.packLossless(type(int224).max, 0), new TaskV3[](0));
            Float balance = iRaindex.vaultBalance2(owner, outputToken, vaultId);
            assertTrue(balance.eq(LibDecimalFloat.packLossless(type(int224).max, 0)));
        }
        if (inputVaultId == bytes32(0)) {
            mockVault0Input(inputToken, owner, 0);
        }

        bytes memory bytecode = iParserV2.parse2(rainString);
        EvaluableV4 memory evaluable = EvaluableV4(iInterpreter, iStore, bytecode);
        config = OrderConfigV4(evaluable, validInputs, validOutputs, bytes32(0), bytes32(0), "");

        vm.prank(owner);
        vm.recordLogs();
        iRaindex.addOrder5(config, new TaskV3[](0));
        Vm.Log[] memory entries = vm.getRecordedLogs();
        assertEq(entries.length, 1);

        return LibTestTakeOrder.extractOrderFromLogs(entries);
    }

    function checkClearOrderHandleIO(
        bytes memory aliceString,
        bytes memory bobString,
        bytes memory aliceErr,
        bytes memory bobErr
    ) internal {
        checkClearOrderHandleIO(
            aliceString, bobString, aliceErr, bobErr, bytes32(uint256(0x01)), bytes32(uint256(0x01))
        );
    }

    function checkClearOrderHandleIO(
        bytes memory aliceString,
        bytes memory bobString,
        bytes memory aliceErr,
        bytes memory bobErr,
        bytes32 outputVaultId,
        bytes32 inputVaultId
    ) internal {
        address aliceInputToken = address(0x100);
        address aliceOutputToken = address(0x101);
        address alice = address(0x102);
        address bob = address(0x103);

        OrderV4 memory aliceOrder =
            userDeposit(aliceString, alice, aliceInputToken, aliceOutputToken, outputVaultId, inputVaultId);
        OrderV4 memory bobOrder =
            userDeposit(bobString, bob, aliceOutputToken, aliceInputToken, outputVaultId, inputVaultId);
        ClearConfigV2 memory clearConfig = ClearConfigV2(0, 0, 0, 0, 0, 0);
        if (aliceErr.length > 0) {
            vm.expectRevert(aliceErr);
        }
        iRaindex.clear4(aliceOrder, bobOrder, clearConfig, new SignedContextV2[](0), new SignedContextV2[](0));

        if (bobErr.length > 0) {
            vm.expectRevert(bobErr);
        }
        iRaindex.clear4(bobOrder, aliceOrder, clearConfig, new SignedContextV2[](0), new SignedContextV2[](0));
    }

    function testClearOrderHandleIO0() external {
        bytes memory aliceString = "_ _:max-positive-value() 1;:ensure(0 \"alice err\");";
        bytes memory bobString = "_ _:max-positive-value() 1;:ensure(0 \"bob err\");";

        checkClearOrderHandleIO(aliceString, bobString, "alice err", "bob err");
    }

    function testClearOrderHandleIO1() external {
        bytes memory aliceString = "_ _:max-positive-value() 1;:;";
        bytes memory bobString = "_ _:max-positive-value() 1;:ensure(0 \"bob err\");";

        checkClearOrderHandleIO(aliceString, bobString, "bob err", "bob err");
    }

    function testClearOrderHandleIO2() external {
        bytes memory aliceString = "_ _:max-positive-value() 1;:ensure(0 \"alice err\");";
        bytes memory bobString = "_ _:max-positive-value() 1;:;";

        checkClearOrderHandleIO(aliceString, bobString, "alice err", "alice err");
    }

    function testClearOrderHandleIO3() external {
        bytes memory aliceString = "_ _:max-positive-value() 1;:ensure(0 \"alice err\");";
        bytes memory bobString = "_ _:max-positive-value() 1;:ensure(0 \"bob err\");";

        checkClearOrderHandleIO(aliceString, bobString, "alice err", "bob err");
    }

    function testClearOrderHandleIO4() external {
        bytes memory aliceErr = "";
        bytes memory bobErr = "";

        bytes memory aliceString = "_ _:1000000 1;:ensure(1 \"alice err\");";
        bytes memory bobString = "_ _:1000000 1;:ensure(1 \"bob err\");";

        checkClearOrderHandleIO(aliceString, bobString, aliceErr, bobErr);
    }

    function testClearOrderHandleIO5() external {
        bytes memory aliceErr = "";
        bytes memory bobErr = "";

        bytes memory aliceString = "_ _:1000000 1;:;";
        bytes memory bobString = "_ _:1000000 1;:;";

        checkClearOrderHandleIO(aliceString, bobString, aliceErr, bobErr);
    }

    function testClearOrderHandleIO0BothVaultIdZero() external {
        bytes memory aliceString = "_ _:max-positive-value() 1;:ensure(0 \"alice err\");";
        bytes memory bobString = "_ _:max-positive-value() 1;:ensure(0 \"bob err\");";

        checkClearOrderHandleIO(aliceString, bobString, "alice err", "bob err", bytes32(0), bytes32(0));
    }
}
