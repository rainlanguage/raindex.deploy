// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";
import {
    OrderConfigV4,
    OrderV4,
    TaskV3,
    ClearConfigV2,
    SignedContextV2,
    IOV2,
    EvaluableV4
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {REVERTING_MOCK_BYTECODE} from "test/util/lib/LibTestConstants.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";

import {Strings} from "@openzeppelin-contracts-5.7.0/utils/Strings.sol";
import {LibDecimalFloat, Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";

struct ClearContextVaultIds {
    bytes32 aliceInputVaultId;
    bytes32 aliceOutputVaultId;
    bytes32 bobInputVaultId;
    bytes32 bobOutputVaultId;
}

contract RaindexV7ClearOrderContextTest is RaindexV7ExternalRealTest {
    using Strings for address;
    using Strings for uint256;
    using LibDecimalFloat for Float;

    function buildContextRainStringIO(
        address inputToken,
        string memory inputDecimals,
        bytes32 inputVaultId,
        address outputToken,
        string memory outputDecimals,
        bytes32 outputVaultId,
        string memory maxOutput,
        string memory ioRatio
    ) internal view returns (string memory) {
        return string.concat(
            "using-words-from ",
            address(iSubParser).toHexString(),
            "\n_ _:",
            maxOutput,
            " ",
            ioRatio,
            ";",
            ":ensure(equal-to(input-token() ",
            inputToken.toHexString(),
            ") \"input token\"),",
            ":ensure(equal-to(input-token-decimals() ",
            inputDecimals,
            ") \"input token decimals\"),",
            ":ensure(equal-to(input-vault-id() ",
            uint256(inputVaultId).toHexString(),
            ") \"input token vault\"),",
            ":ensure(equal-to(input-vault-before() 0) \"input vault before\"),",
            ":ensure(equal-to(output-token() ",
            outputToken.toHexString(),
            ") \"output token\"),",
            ":ensure(equal-to(output-token-decimals() ",
            outputDecimals,
            ") \"output token decimals\"),",
            ":ensure(equal-to(output-vault-id() ",
            uint256(outputVaultId).toHexString(),
            ") \"output token vault\"),"
        );
    }

    function buildContextRainStringOrder(
        string memory ioPart,
        address owner,
        address counterparty,
        string memory maxOutput,
        string memory ioRatio,
        string memory outputVaultDecrease,
        string memory inputVaultIncrease
    ) internal view returns (bytes memory) {
        return bytes(
            string.concat(
                ioPart,
                ":ensure(equal-to(output-vault-before() 100) \"output vault before\"),",
                ":ensure(equal-to(raindex() ",
                address(iRaindex).toHexString(),
                ") \"Raindex\"),",
                ":ensure(equal-to(order-clearer() ",
                address(this).toHexString(),
                ") \"clearer\"),",
                ":ensure(equal-to(order-owner() ",
                owner.toHexString(),
                ") \"owner\"),",
                ":ensure(equal-to(order-counterparty() ",
                counterparty.toHexString(),
                ") \"counterparty\"),",
                ":ensure(equal-to(calculated-io-ratio() ",
                ioRatio,
                ") \"io ratio\"),",
                ":ensure(equal-to(output-vault-decrease() ",
                outputVaultDecrease,
                ") \"output vault decrease\"),",
                ":ensure(equal-to(input-vault-increase() ",
                inputVaultIncrease,
                ") \"input vault increase\"),",
                ":ensure(equal-to(calculated-max-output() ",
                maxOutput,
                ") \"max output\");"
            )
        );
    }

    function setupOutputVault(
        address token,
        address owner,
        bytes32 vaultId,
        int224 amount,
        int64 exponent,
        uint8 decimals
    ) internal {
        if (vaultId == bytes32(0)) {
            (uint256 absoluteAmount,) = LibDecimalFloat.packLossless(amount, exponent).toFixedDecimalLossy(decimals);
            mockVault0Output(token, owner, absoluteAmount);
        } else {
            vm.mockCall(
                token, abi.encodeWithSelector(IERC20.transferFrom.selector, owner, address(iRaindex)), abi.encode(true)
            );
            vm.prank(owner);
            iRaindex.deposit5(token, vaultId, LibDecimalFloat.packLossless(amount, exponent), new TaskV3[](0));
        }
    }

    function checkContextEmptyStack(address alice, address bob, ClearContextVaultIds memory vaultIds) internal {
        bytes memory rainStringAlice = buildContextRainStringOrder(
            buildContextRainStringIO(
                address(iToken0),
                "12",
                vaultIds.aliceInputVaultId,
                address(iToken1),
                "6",
                vaultIds.aliceOutputVaultId,
                "5",
                "2"
            ),
            alice,
            bob,
            "5",
            "2",
            "1.5",
            "3"
        );

        bytes memory rainStringBob = buildContextRainStringOrder(
            buildContextRainStringIO(
                address(iToken1),
                "6",
                vaultIds.bobInputVaultId,
                address(iToken0),
                "12",
                vaultIds.bobOutputVaultId,
                "3",
                "0.5"
            ),
            bob,
            alice,
            "3",
            "0.5",
            "3",
            "1.5"
        );

        OrderConfigV4 memory configAlice;
        {
            IOV2[] memory validInputsAlice = new IOV2[](1);
            validInputsAlice[0] = IOV2({token: address(iToken0), vaultId: vaultIds.aliceInputVaultId});
            IOV2[] memory validOutputsAlice = new IOV2[](1);
            validOutputsAlice[0] = IOV2({token: address(iToken1), vaultId: vaultIds.aliceOutputVaultId});
            configAlice = OrderConfigV4({
                evaluable: EvaluableV4({
                    bytecode: iParserV2.parse2(rainStringAlice), interpreter: iInterpreter, store: iStore
                }),
                validInputs: validInputsAlice,
                validOutputs: validOutputsAlice,
                nonce: 0,
                secret: 0,
                meta: ""
            });
        }
        OrderConfigV4 memory configBob;
        {
            IOV2[] memory validInputsBob = new IOV2[](1);
            validInputsBob[0] = IOV2({token: address(iToken1), vaultId: vaultIds.bobInputVaultId});

            IOV2[] memory validOutputsBob = new IOV2[](1);
            validOutputsBob[0] = IOV2({token: address(iToken0), vaultId: vaultIds.bobOutputVaultId});

            configBob = OrderConfigV4({
                evaluable: EvaluableV4({
                    bytecode: iParserV2.parse2(rainStringBob), interpreter: iInterpreter, store: iStore
                }),
                validInputs: validInputsBob,
                validOutputs: validOutputsBob,
                nonce: 0,
                secret: 0,
                meta: ""
            });
        }

        vm.mockCall(
            configAlice.validInputs[0].token, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(12)
        );
        vm.mockCall(
            configAlice.validOutputs[0].token, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(6)
        );
        vm.mockCall(
            configBob.validInputs[0].token, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(6)
        );
        vm.mockCall(
            configBob.validOutputs[0].token, abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(12)
        );

        OrderV4 memory orderAlice =
            OrderV4(alice, configAlice.evaluable, configAlice.validInputs, configAlice.validOutputs, configAlice.nonce);
        OrderV4 memory orderBob =
            OrderV4(bob, configBob.evaluable, configBob.validInputs, configBob.validOutputs, configBob.nonce);

        vm.prank(alice);
        iRaindex.addOrder5(configAlice, new TaskV3[](0));

        vm.prank(bob);
        iRaindex.addOrder5(configBob, new TaskV3[](0));

        setupOutputVault(address(iToken1), alice, vaultIds.aliceOutputVaultId, 100e6, -6, 6);
        setupOutputVault(address(iToken0), bob, vaultIds.bobOutputVaultId, 100e12, -12, 12);

        if (vaultIds.aliceOutputVaultId == bytes32(0)) {
            mockVault0Input(address(iToken1), bob, 1500000);
        }
        if (vaultIds.bobOutputVaultId == bytes32(0)) {
            mockVault0Input(address(iToken0), alice, 3e12);
        }

        iRaindex.clear4(
            orderAlice, orderBob, ClearConfigV2(0, 0, 0, 0, 0, 0), new SignedContextV2[](0), new SignedContextV2[](0)
        );
    }

    /// forge-config: default.fuzz.runs = 10
    function testContextEmptyStack(
        address alice,
        address bob,
        bytes32 aliceInputVaultId,
        bytes32 aliceOutputVaultId,
        bytes32 bobInputVaultId,
        bytes32 bobOutputVaultId
    ) external {
        vm.assume(alice != bob);
        vm.assume(aliceInputVaultId != bytes32(0));
        vm.assume(aliceOutputVaultId != bytes32(0));
        vm.assume(bobInputVaultId != bytes32(0));
        vm.assume(bobOutputVaultId != bytes32(0));
        checkContextEmptyStack(
            alice, bob, ClearContextVaultIds(aliceInputVaultId, aliceOutputVaultId, bobInputVaultId, bobOutputVaultId)
        );
    }

    /// forge-config: default.fuzz.runs = 10
    function testContextEmptyStackBothVaultIdZero(address alice, address bob) external {
        vm.assume(alice != bob);
        checkContextEmptyStack(alice, bob, ClearContextVaultIds(bytes32(0), bytes32(0), bytes32(0), bytes32(0)));
    }
}
