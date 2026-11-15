// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest, LibDecimalFloat, Float} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    OrderConfigV4,
    EvaluableV4,
    TaskV3,
    SignedContextV2
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";
import {LibFormatDecimalFloat} from "rain-math-float-0.3.11/src/lib/format/LibFormatDecimalFloat.sol";

import {Strings} from "@openzeppelin-contracts-5.7.0/utils/Strings.sol";

contract RaindexV7DepositEnactTest is RaindexV7ExternalRealTest {
    using Strings for address;
    using Strings for uint256;
    using LibDecimalFloat for Float;
    using LibFormatDecimalFloat for Float;

    function checkReentrancyRW() internal view {
        (bytes32[] memory reads, bytes32[] memory writes) = vm.accesses(address(iRaindex));
        // ReentrancyGuard.REENTRANCY_GUARD_STORAGE
        bytes32 reentrancyGuardStorage = 0x9b779b17422d0df92223018b32b4d1fa46e071723d6817e2486d003becc55f00;

        assertEq(reads.length, 5);
        assertEq(reads[0], reentrancyGuardStorage);
        assertEq(reads[1], reentrancyGuardStorage);
        assertEq(reads[reads.length - 1], reentrancyGuardStorage);

        assertEq(writes.length, 3);
        assertEq(writes[0], reentrancyGuardStorage);
        assertEq(writes[writes.length - 1], reentrancyGuardStorage);
    }

    function checkDeposit(
        address owner,
        bytes32 vaultId,
        Float amount,
        uint8 decimals,
        bytes[] memory evalStrings,
        uint256 expectedReads,
        uint256 expectedWrites
    ) internal {
        vm.assume(vaultId != bytes32(0));
        uint256 amount18 = LibDecimalFloat.toFixedDecimalLossless(amount, decimals);
        vm.startPrank(owner);
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, owner, address(iRaindex), amount18),
            abi.encode(true)
        );

        TaskV3[] memory actions = new TaskV3[](evalStrings.length);
        for (uint256 i = 0; i < evalStrings.length; i++) {
            actions[i] =
                TaskV3(EvaluableV4(iInterpreter, iStore, iParserV2.parse2(evalStrings[i])), new SignedContextV2[](0));
        }
        vm.record();
        iRaindex.deposit5(address(iToken0), vaultId, amount, actions);
        checkReentrancyRW();
        (bytes32[] memory reads, bytes32[] memory writes) = vm.accesses(address(iStore));
        assert(reads.length == expectedReads);
        assert(writes.length == expectedWrites);
        vm.stopPrank();
    }

    /// forge-config: default.fuzz.runs = 10
    function testRaindexDepositEnactEmptyNoop(address alice, bytes32 vaultId, uint256 amount18) external {
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)));
        Float amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(amount18, 18);
        vm.assume(amount.gt(LibDecimalFloat.packLossless(0, 0)));
        bytes[] memory evals = new bytes[](0);
        checkDeposit(alice, vaultId, amount, 18, evals, 0, 0);
    }

    /// forge-config: default.fuzz.runs = 10
    function testRaindexDepositEnactOneStateless(address alice, bytes32 vaultId, uint256 amount18) external {
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)));
        Float amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(amount18, 18);
        vm.assume(amount.gt(LibDecimalFloat.packLossless(0, 0)));
        bytes[] memory evals = new bytes[](1);
        evals[0] = bytes("_:1;");
        checkDeposit(alice, vaultId, amount, 18, evals, 0, 0);
    }

    /// forge-config: default.fuzz.runs = 10
    function testRaindexDepositEnactOneReadState(address alice, bytes32 vaultId, uint256 amount18) external {
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)));
        Float amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(amount18, 18);
        vm.assume(amount.gt(LibDecimalFloat.packLossless(0, 0)));
        bytes[] memory evals = new bytes[](1);
        evals[0] = bytes("_:get(0);");
        // each get is 2 reads. 1 during eval and 1 during store set.
        // each get is 1 write.
        checkDeposit(alice, vaultId, amount, 18, evals, 2, 1);
    }

    /// forge-config: default.fuzz.runs = 10
    function testRaindexDepositEvalWriteStateSingle(address alice, bytes32 vaultId, uint256 amount18) external {
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)));
        // amount18 is bound above so safe to typecast.
        // forge-lint: disable-next-line(unsafe-typecast)
        Float amount = LibDecimalFloat.packLossless(int256(amount18), -18);

        bytes[] memory evals = new bytes[](1);
        evals[0] = bytes(":set(1 2);");
        // 1 for the set.
        checkDeposit(alice, vaultId, amount, 18, evals, 1, 1);

        evals[0] = bytes(":ensure(equal-to(get(1) 2) \"set works\");");
        checkDeposit(alice, vaultId, amount, 18, evals, 2, 1);
    }

    /// forge-config: default.fuzz.runs = 10
    function testRaindexDepositEvalWriteStateSequential(address alice, bytes32 vaultId, uint256 amount18) external {
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)));
        // amount18 is bound above so safe to typecast.
        // forge-lint: disable-next-line(unsafe-typecast)
        Float amount = LibDecimalFloat.packLossless(int256(amount18), -18);

        bytes[] memory evals0 = new bytes[](4);
        evals0[0] = bytes(":set(1 2);");
        evals0[1] = bytes(":ensure(equal-to(get(1) 2) \"0th set not equal\");");
        evals0[2] = bytes(":set(2 3);");
        evals0[3] = bytes(":ensure(equal-to(get(2) 3) \"1st set not equal\");");
        checkDeposit(alice, vaultId, amount, 18, evals0, 6, 4);

        bytes[] memory evals1 = new bytes[](4);
        evals1[0] = bytes(":set(1 20);");
        evals1[1] = bytes(":ensure(equal-to(get(1) 20) \"0th set not equal\");");
        evals1[2] = bytes(":set(2 30);");
        evals1[3] = bytes(":ensure(equal-to(get(2) 30) \"1st set not equal\");");
        checkDeposit(alice, vaultId, amount, 18, evals1, 6, 4);
    }

    /// forge-config: default.fuzz.runs = 10
    function testRaindexDepositEvalWriteStateDifferentOwnersNamespaced(
        address alice,
        address bob,
        bytes32 vaultId,
        uint256 amount18
    ) external {
        vm.assume(alice != bob);
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)));
        // amount18 is bound above so safe to typecast.
        // forge-lint: disable-next-line(unsafe-typecast)
        Float amount = LibDecimalFloat.packLossless(int256(amount18), -18);

        bytes[] memory evals0 = new bytes[](4);
        evals0[0] = bytes(":set(1 2);");
        evals0[1] = bytes(":ensure(equal-to(get(1) 2) \"0th set not equal\");");
        evals0[2] = bytes(":set(2 3);");
        evals0[3] = bytes(":ensure(equal-to(get(2) 3) \"1st set not equal\");");
        checkDeposit(alice, vaultId, amount, 18, evals0, 6, 4);

        bytes[] memory evals1 = new bytes[](4);
        evals1[0] = bytes(":set(1 20);");
        evals1[1] = bytes(":ensure(equal-to(get(1) 20) \"0th set not equal\");");
        evals1[2] = bytes(":set(2 30);");
        evals1[3] = bytes(":ensure(equal-to(get(2) 30) \"1st set not equal\");");
        checkDeposit(bob, vaultId, amount, 18, evals1, 6, 4);

        bytes[] memory evals2 = new bytes[](2);
        evals2[0] = bytes(":ensure(equal-to(get(1) 2) \"alice state 1\");");
        evals2[1] = bytes(":ensure(equal-to(get(2) 3) \"alice state 2\");");
        checkDeposit(alice, vaultId, amount, 18, evals2, 4, 2);

        bytes[] memory evals3 = new bytes[](2);
        evals3[0] = bytes(":ensure(equal-to(get(1) 20) \"bob state 1\");");
        evals3[1] = bytes(":ensure(equal-to(get(2) 30) \"bob state 2\");");
        checkDeposit(bob, vaultId, amount, 18, evals3, 4, 2);
    }

    /// forge-config: default.fuzz.runs = 10
    function testOrderDepositContext(
        address alice,
        bytes32 vaultId,
        uint256 preDepositAmount18,
        uint256 depositAmount18
    ) external {
        vm.assume(vaultId != bytes32(0));
        preDepositAmount18 = bound(preDepositAmount18, 1, uint256(int256(type(int128).max)));
        depositAmount18 = bound(depositAmount18, 1, uint256(int256(type(int128).max)));

        // pre deposit amount is bound above so safe to typecast.
        // forge-lint: disable-next-line(unsafe-typecast)
        Float preDepositAmount = LibDecimalFloat.packLossless(int256(preDepositAmount18), -6);
        // deposit amount is bound above so safe to typecast.
        // forge-lint: disable-next-line(unsafe-typecast)
        Float depositAmount = LibDecimalFloat.packLossless(int256(depositAmount18), -6);

        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(6));

        checkDeposit(alice, vaultId, preDepositAmount, 6, new bytes[](0), 0, 0);

        string memory usingWordsFrom = string.concat("using-words-from ", address(iSubParser).toHexString(), "\n");

        bytes[] memory evals = new bytes[](7);
        evals[0] = bytes(
            string.concat(
                usingWordsFrom,
                ":ensure(equal-to(raindex() ",
                address(iRaindex).toHexString(),
                ") \"raindex is iRaindex\");"
            )
        );
        evals[1] = bytes(
            string.concat(
                usingWordsFrom, ":ensure(equal-to(depositor() ", alice.toHexString(), ") \"depositor is alice\");"
            )
        );
        evals[2] = bytes(
            string.concat(
                usingWordsFrom,
                ":ensure(equal-to(deposit-token() ",
                address(iToken0).toHexString(),
                ") \"token is iToken0\");"
            )
        );
        evals[3] = bytes(
            string.concat(
                usingWordsFrom,
                ":ensure(equal-to(deposit-vault-id() ",
                uint256(vaultId).toHexString(),
                ") \"deposit vaultId is vaultId\");"
            )
        );
        evals[4] = bytes(
            string.concat(
                usingWordsFrom,
                ":ensure(equal-to(deposit-vault-before() ",
                preDepositAmount.toDecimalString(false),
                ") \"vault balance before\");"
            )
        );
        evals[5] = bytes(
            string.concat(
                usingWordsFrom,
                ":ensure(equal-to(deposit-vault-after() ",
                preDepositAmount.add(depositAmount).toDecimalString(false),
                ") \"vault balance after\");"
            )
        );
        evals[6] = bytes(
            string.concat(
                usingWordsFrom,
                ":ensure(equal-to(deposit-token-decimals() ",
                uint256(6).toHexString(),
                ") \"deposit token decimals\");"
            )
        );

        checkDeposit(alice, vaultId, depositAmount, 6, evals, 0, 0);
    }

    /// A revert in the action prevents the deposit from being enacted.
    /// forge-config: default.fuzz.runs = 10
    function testDepositRevertInAction(address alice, bytes32 vaultId, uint256 amount18) external {
        vm.assume(vaultId != bytes32(0));
        amount18 = bound(amount18, 1, uint256(int256(type(int224).max)));
        vm.startPrank(alice);
        vm.mockCall(
            address(iToken0),
            abi.encodeWithSelector(IERC20.transferFrom.selector, alice, address(iRaindex), amount18),
            abi.encode(true)
        );

        Float amount = LibDecimalFloat.fromFixedDecimalLosslessPacked(amount18, 18);

        bytes[] memory evals = new bytes[](1);
        evals[0] = bytes(":ensure(0 \"revert in action\");");

        TaskV3[] memory actions = evalsToActions(evals);

        assertTrue(iRaindex.vaultBalance2(alice, address(iToken0), vaultId).isZero());

        vm.expectRevert("revert in action");
        iRaindex.deposit5(address(iToken0), vaultId, amount, actions);

        assertTrue(iRaindex.vaultBalance2(alice, address(iToken0), vaultId).isZero());
    }
}
