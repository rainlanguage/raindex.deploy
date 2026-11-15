// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    OrderConfigV4,
    OrderV4,
    TaskV3,
    ClearConfigV2,
    SignedContextV2,
    IOV2,
    EvaluableV4
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IRaindexV6, SignedContextV1} from "raindex-interface-0.1.13/src/interface/deprecated/v6/IRaindexV6.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {Strings} from "@openzeppelin-contracts-5.7.0/utils/Strings.sol";
import {LibDecimalFloat, Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibTestSignedContext} from "test/util/lib/LibTestSignedContext.sol";

/// @title RaindexV7ClearSignedContextV2Test
/// @notice `clear4` evaluates orders with `SignedContextV2`: each order's
/// expression asserts the signer, domain and signed content it sees. Runs
/// against fresh-compiled RaindexV7 so source mutations are observable.
contract RaindexV7ClearSignedContextV2Test is RaindexV7ExternalRealTest {
    using Strings for address;
    using Strings for uint256;
    using LibDecimalFloat for Float;

    bytes32 internal constant ALICE_IN = bytes32(uint256(0xa1));
    bytes32 internal constant ALICE_OUT = bytes32(uint256(0xa2));
    bytes32 internal constant BOB_IN = bytes32(uint256(0xb1));
    bytes32 internal constant BOB_OUT = bytes32(uint256(0xb2));

    struct Expected {
        uint256 pk;
        bytes32 domain;
        bytes32 content;
    }

    function setUp() external {
        vm.etch(address(iRaindex), vm.getDeployedCode("RaindexV7.sol:RaindexV7"));
    }

    function signedContext(Expected memory expected) internal pure returns (SignedContextV2[] memory) {
        bytes32[] memory context = new bytes32[](1);
        context[0] = expected.content;
        SignedContextV2[] memory signedContexts = new SignedContextV2[](1);
        signedContexts[0] = LibTestSignedContext.sign(vm, expected.pk, expected.domain, context);
        return signedContexts;
    }

    function expectingBytecode(string memory name, Expected memory expected) internal view returns (bytes memory) {
        return iParserV2.parse2(
            bytes(
                string.concat(
                    "using-words-from ",
                    address(iSubParser).toHexString(),
                    "\n:ensure(equal-to(signer<0>() ",
                    vm.addr(expected.pk).toHexString(),
                    ") \"",
                    name,
                    " signer\"),\n:ensure(equal-to(signer-domain<0>() ",
                    uint256(expected.domain).toHexString(),
                    ") \"",
                    name,
                    " domain\"),\n:ensure(equal-to(signed-context<0 0>() ",
                    uint256(expected.content).toHexString(),
                    ") \"",
                    name,
                    " signed context\"),\n_ _: 1 1;\n:;"
                )
            )
        );
    }

    function addFundedOrder(
        address owner,
        address inputToken,
        bytes32 inputVault,
        address outputToken,
        bytes32 outputVault,
        bytes memory bytecode
    ) internal returns (OrderV4 memory) {
        IOV2[] memory inputs = new IOV2[](1);
        inputs[0] = IOV2({token: inputToken, vaultId: inputVault});
        IOV2[] memory outputs = new IOV2[](1);
        outputs[0] = IOV2({token: outputToken, vaultId: outputVault});
        OrderConfigV4 memory config = OrderConfigV4({
            evaluable: EvaluableV4({bytecode: bytecode, interpreter: iInterpreter, store: iStore}),
            validInputs: inputs,
            validOutputs: outputs,
            nonce: 0,
            secret: 0,
            meta: ""
        });

        vm.mockCall(
            outputToken,
            abi.encodeWithSelector(IERC20.transferFrom.selector, owner, address(iRaindex)),
            abi.encode(true)
        );
        vm.startPrank(owner);
        iRaindex.deposit5(outputToken, outputVault, LibDecimalFloat.packLossless(100, 0), new TaskV3[](0));
        iRaindex.addOrder5(config, new TaskV3[](0));
        vm.stopPrank();

        return OrderV4(owner, config.evaluable, config.validInputs, config.validOutputs, config.nonce);
    }

    function orders(Expected memory alice, Expected memory bob) internal returns (OrderV4 memory, OrderV4 memory) {
        return (
            addFundedOrder(
                makeAddr("alice"),
                address(iToken0),
                ALICE_IN,
                address(iToken1),
                ALICE_OUT,
                expectingBytecode("alice", alice)
            ),
            addFundedOrder(
                makeAddr("bob"), address(iToken1), BOB_IN, address(iToken0), BOB_OUT, expectingBytecode("bob", bob)
            )
        );
    }

    function boundPk(uint256 pk) internal pure returns (uint256) {
        return bound(pk, 1, SECP256K1_ORDER - 1);
    }

    function clearConfig() internal pure returns (ClearConfigV2 memory) {
        return ClearConfigV2(0, 0, 0, 0, bytes32(uint256(0xc1)), bytes32(uint256(0xc2)));
    }

    /// Both orders receive the same signed context, so this holds under either
    /// routing of `aliceSignedContext`/`bobSignedContext`.
    /// forge-config: default.fuzz.runs = 32
    function testClearEvalSeesSignedContextV2(Expected memory expected) external {
        expected.pk = boundPk(expected.pk);
        vm.assume(expected.domain != bytes32(0));
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) = orders(expected, expected);
        SignedContextV2[] memory signedContexts = signedContext(expected);

        iRaindex.clear4(aliceOrder, bobOrder, clearConfig(), signedContexts, signedContexts);

        assertTrue(iRaindex.vaultBalance2(aliceOrder.owner, address(iToken0), ALICE_IN).gt(LibDecimalFloat.FLOAT_ZERO));
        assertTrue(iRaindex.vaultBalance2(bobOrder.owner, address(iToken1), BOB_IN).gt(LibDecimalFloat.FLOAT_ZERO));
    }

    function testClear3SelectorIsRemoved() external {
        (bool success, bytes memory returnData) = address(iRaindex)
            .call(
                abi.encodeCall(
                    IRaindexV6.clear3,
                    (
                        OrderV4(address(0), EvaluableV4(iInterpreter, iStore, ""), new IOV2[](0), new IOV2[](0), 0),
                        OrderV4(address(0), EvaluableV4(iInterpreter, iStore, ""), new IOV2[](0), new IOV2[](0), 0),
                        ClearConfigV2(0, 0, 0, 0, 0, 0),
                        new SignedContextV1[](0),
                        new SignedContextV1[](0)
                    )
                )
            );
        assertFalse(success);
        assertEq(returnData.length, 0);
    }
}
