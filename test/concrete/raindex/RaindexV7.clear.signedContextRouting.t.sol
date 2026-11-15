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
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {Strings} from "@openzeppelin-contracts-5.7.0/utils/Strings.sol";
import {LibDecimalFloat, Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibTestSignedContext} from "test/util/lib/LibTestSignedContext.sol";

/// @title RaindexV7ClearSignedContextRoutingTest
/// @notice `clear4` evaluates alice's order with `aliceSignedContext` and bob's
/// with `bobSignedContext`, matching `quote3`/`takeOrders5`. Each order's
/// expression asserts the signer, domain and signed content it sees, so routing
/// is pinned by signer, domain and content rather than slot. Runs against
/// fresh-compiled RaindexV7 so source mutations are observable.
contract RaindexV7ClearSignedContextRoutingTest is RaindexV7ExternalRealTest {
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

    /// forge-config: default.fuzz.runs = 32
    function testClearRoutesEachSignedContextToItsOwnOrder(Expected memory alice, Expected memory bob) external {
        alice.pk = boundPk(alice.pk);
        bob.pk = boundPk(bob.pk);
        vm.assume(vm.addr(alice.pk) != vm.addr(bob.pk));
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) = orders(alice, bob);

        iRaindex.clear4(aliceOrder, bobOrder, clearConfig(), signedContext(alice), signedContext(bob));

        assertTrue(iRaindex.vaultBalance2(aliceOrder.owner, address(iToken0), ALICE_IN).gt(LibDecimalFloat.FLOAT_ZERO));
        assertTrue(iRaindex.vaultBalance2(bobOrder.owner, address(iToken1), BOB_IN).gt(LibDecimalFloat.FLOAT_ZERO));
    }

    /// forge-config: default.fuzz.runs = 32
    function testClearRevertsWhenSignedContextsAreSwapped(Expected memory alice, Expected memory bob) external {
        alice.pk = boundPk(alice.pk);
        bob.pk = boundPk(bob.pk);
        vm.assume(vm.addr(alice.pk) != vm.addr(bob.pk));
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) = orders(alice, bob);
        SignedContextV2[] memory aliceSignedContext = signedContext(alice);
        SignedContextV2[] memory bobSignedContext = signedContext(bob);

        vm.expectRevert("alice signer");
        iRaindex.clear4(aliceOrder, bobOrder, clearConfig(), bobSignedContext, aliceSignedContext);
    }

    /// Same signer and content for both orders, so only the domain tells the
    /// two signed contexts apart.
    /// forge-config: default.fuzz.runs = 32
    function testClearRevertsWhenDomainsAreSwapped(uint256 pk, bytes32 aliceDomain, bytes32 bobDomain, bytes32 content)
        external
    {
        vm.assume(aliceDomain != bobDomain);
        pk = boundPk(pk);
        Expected memory alice = Expected(pk, aliceDomain, content);
        Expected memory bob = Expected(pk, bobDomain, content);
        (OrderV4 memory aliceOrder, OrderV4 memory bobOrder) = orders(alice, bob);
        SignedContextV2[] memory aliceSignedContext = signedContext(alice);
        SignedContextV2[] memory bobSignedContext = signedContext(bob);

        vm.expectRevert("alice domain");
        iRaindex.clear4(aliceOrder, bobOrder, clearConfig(), bobSignedContext, aliceSignedContext);
    }
}
