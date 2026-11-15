// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    OrderConfigV4,
    OrderV4,
    IOV2,
    TaskV3,
    EvaluableV4,
    SignedContextV2
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {LibTestSignedContext} from "test/util/lib/LibTestSignedContext.sol";
import {Strings} from "@openzeppelin-contracts-5.7.0/utils/Strings.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";

/// @title RaindexV7PostTaskSignedContextTest
/// @notice `signer<i>()`, `signer-domain<i>()` and `signed-context<i j>()` read
/// the real signers, domains and signed data in every Raindex post task, so a
/// caller cannot forge a signer by placing an address in signed data.
contract RaindexV7PostTaskSignedContextTest is RaindexV7ExternalRealTest {
    using Strings for address;
    using Strings for uint256;

    uint256 internal constant SIGNER_A_PK = 0xA11CE;
    uint256 internal constant SIGNER_B_PK = 0xB0B;
    address internal constant DATA_A = address(0xDA7A0);
    address internal constant DATA_B = address(0xDA7A1);
    address internal constant TRUSTED = address(0x7777);
    address internal constant ACTOR = address(0xD0);
    bytes32 internal constant DOMAIN_A = keccak256("domain a");
    bytes32 internal constant DOMAIN_B = keccak256("domain b");

    function signed(uint256 pk, bytes32 domain, bytes32[] memory ctx) internal pure returns (SignedContextV2 memory) {
        return LibTestSignedContext.sign(vm, pk, domain, ctx);
    }

    function single(address a) internal pure returns (bytes32[] memory) {
        bytes32[] memory ctx = new bytes32[](1);
        ctx[0] = bytes32(uint256(uint160(a)));
        return ctx;
    }

    function task(string memory body, SignedContextV2[] memory sc) internal view returns (TaskV3[] memory) {
        TaskV3[] memory post = new TaskV3[](1);
        post[0] = TaskV3(
            EvaluableV4(
                iInterpreter,
                iStore,
                iParserV2.parse2(
                    bytes(string.concat("using-words-from ", address(iSubParser).toHexString(), "\n", body))
                )
            ),
            sc
        );
        return post;
    }

    /// Two signers, each signing its own data; the task asserts each word reads
    /// its own signer and datum.
    function realSignersTask() internal view returns (TaskV3[] memory) {
        SignedContextV2[] memory sc = new SignedContextV2[](2);
        sc[0] = signed(SIGNER_A_PK, DOMAIN_A, single(DATA_A));
        sc[1] = signed(SIGNER_B_PK, DOMAIN_B, single(DATA_B));
        return task(
            string.concat(
                ":ensure(equal-to(signer<0>() ",
                vm.addr(SIGNER_A_PK).toHexString(),
                ") \"signer 0\"),",
                ":ensure(equal-to(signer<1>() ",
                vm.addr(SIGNER_B_PK).toHexString(),
                ") \"signer 1\"),",
                ":ensure(equal-to(signer-domain<0>() ",
                uint256(DOMAIN_A).toHexString(),
                ") \"signer domain 0\"),",
                ":ensure(equal-to(signer-domain<1>() ",
                uint256(DOMAIN_B).toHexString(),
                ") \"signer domain 1\"),",
                ":ensure(equal-to(signed-context<0 0>() ",
                DATA_A.toHexString(),
                ") \"signed context 0\"),",
                ":ensure(equal-to(signed-context<1 0>() ",
                DATA_B.toHexString(),
                ") \"signed context 1\");"
            ),
            sc
        );
    }

    /// One attacker key signs three contexts, the third holding `TRUSTED`. A
    /// task that accepts only `TRUSTED` as signer must reject it.
    function forgedSignerTask() internal view returns (TaskV3[] memory) {
        SignedContextV2[] memory sc = new SignedContextV2[](3);
        sc[0] = signed(SIGNER_A_PK, DOMAIN_A, single(DATA_A));
        sc[1] = signed(SIGNER_A_PK, DOMAIN_A, single(DATA_A));
        sc[2] = signed(SIGNER_A_PK, DOMAIN_A, single(TRUSTED));
        return task(string.concat(":ensure(equal-to(signer<0>() ", TRUSTED.toHexString(), ") \"signer trusted\");"), sc);
    }

    function deposit(TaskV3[] memory post) internal {
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, ACTOR, address(iRaindex), 1e18),
            abi.encode(true)
        );
        vm.prank(ACTOR);
        iRaindex.deposit5(address(iToken0), bytes32(uint256(1)), LibDecimalFloat.packLossless(1, 0), post);
    }

    function withdraw(TaskV3[] memory post) internal {
        deposit(new TaskV3[](0));
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20.transfer.selector, ACTOR, 1e18), abi.encode(true));
        vm.prank(ACTOR);
        iRaindex.withdraw5(address(iToken0), bytes32(uint256(1)), LibDecimalFloat.packLossless(1, 0), post);
    }

    function orderConfig() internal view returns (OrderConfigV4 memory) {
        IOV2[] memory inputs = new IOV2[](1);
        inputs[0] = IOV2(address(iToken0), bytes32(uint256(1)));
        IOV2[] memory outputs = new IOV2[](1);
        outputs[0] = IOV2(address(iToken1), bytes32(uint256(1)));
        return OrderConfigV4(
            EvaluableV4(iInterpreter, iStore, LibTestAddOrder.VALID_ORDER_BYTECODE), inputs, outputs, bytes32(0), "", ""
        );
    }

    function addOrder(TaskV3[] memory post) internal {
        vm.prank(ACTOR);
        assertTrue(iRaindex.addOrder5(orderConfig(), post));
    }

    function removeOrder(TaskV3[] memory post) internal {
        OrderConfigV4 memory config = orderConfig();
        vm.prank(ACTOR);
        iRaindex.addOrder5(config, new TaskV3[](0));
        vm.prank(ACTOR);
        assertTrue(
            iRaindex.removeOrder4(
                OrderV4(ACTOR, config.evaluable, config.validInputs, config.validOutputs, config.nonce), post
            )
        );
    }

    function entask(TaskV3[] memory post) internal {
        vm.prank(ACTOR);
        iRaindex.entask3(post);
    }

    function testDepositTaskSignerWordsReadRealSigners() external {
        deposit(realSignersTask());
    }

    function testDepositTaskRejectsSignerForgedInSignedData() external {
        TaskV3[] memory post = forgedSignerTask();
        vm.expectRevert("signer trusted");
        this.externalDeposit(post);
    }

    function testWithdrawTaskSignerWordsReadRealSigners() external {
        withdraw(realSignersTask());
    }

    function testWithdrawTaskRejectsSignerForgedInSignedData() external {
        TaskV3[] memory post = forgedSignerTask();
        vm.expectRevert("signer trusted");
        this.externalWithdraw(post);
    }

    function testAddOrderTaskSignerWordsReadRealSigners() external {
        addOrder(realSignersTask());
    }

    function testAddOrderTaskRejectsSignerForgedInSignedData() external {
        TaskV3[] memory post = forgedSignerTask();
        vm.expectRevert("signer trusted");
        this.externalAddOrder(post);
    }

    function testRemoveOrderTaskSignerWordsReadRealSigners() external {
        removeOrder(realSignersTask());
    }

    function testRemoveOrderTaskRejectsSignerForgedInSignedData() external {
        TaskV3[] memory post = forgedSignerTask();
        vm.expectRevert("signer trusted");
        this.externalRemoveOrder(post);
    }

    function testEntaskSignerWordsReadRealSigners() external {
        entask(realSignersTask());
    }

    function testEntaskRejectsSignerForgedInSignedData() external {
        TaskV3[] memory post = forgedSignerTask();
        vm.expectRevert("signer trusted");
        this.externalEntask(post);
    }

    function externalDeposit(TaskV3[] memory post) external {
        deposit(post);
    }

    function externalWithdraw(TaskV3[] memory post) external {
        withdraw(post);
    }

    function externalAddOrder(TaskV3[] memory post) external {
        addOrder(post);
    }

    function externalRemoveOrder(TaskV3[] memory post) external {
        removeOrder(post);
    }

    function externalEntask(TaskV3[] memory post) external {
        entask(post);
    }
}
