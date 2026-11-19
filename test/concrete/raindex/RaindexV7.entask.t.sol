// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    OrderConfigV4,
    EvaluableV4,
    TaskV3,
    SignedContextV2
} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";

contract RaindexV7EnactTest is RaindexV7ExternalRealTest {
    function checkReentrancyRW() internal view {
        (bytes32[] memory reads, bytes32[] memory writes) = vm.accesses(address(iRaindex));
        // reads/writes for reentrancy guard.
        // ReentrancyGuard.REENTRANCY_GUARD_STORAGE
        bytes32 reentrancyGuardStorage = 0x9b779b17422d0df92223018b32b4d1fa46e071723d6817e2486d003becc55f00;
        assert(reads.length == 3);
        assert(reads[0] == reentrancyGuardStorage);
        assert(reads[1] == reentrancyGuardStorage);
        assert(reads[2] == reentrancyGuardStorage);
        assert(writes.length == 2);
        assert(writes[0] == reentrancyGuardStorage);
        assert(writes[1] == reentrancyGuardStorage);
    }

    function checkEntask(address owner, bytes[] memory evalStrings, uint256 expectedReads, uint256 expectedWrites)
        internal
    {
        vm.startPrank(owner);
        TaskV3[] memory actions = new TaskV3[](evalStrings.length);
        for (uint256 i = 0; i < evalStrings.length; i++) {
            actions[i] =
                TaskV3(EvaluableV4(iInterpreter, iStore, iParserV2.parse2(evalStrings[i])), new SignedContextV2[](0));
        }
        vm.record();
        iRaindex.entask3(actions);
        checkReentrancyRW();
        (bytes32[] memory reads, bytes32[] memory writes) = vm.accesses(address(iStore));
        assert(reads.length == expectedReads);
        assert(writes.length == expectedWrites);
        vm.stopPrank();
    }

    /// forge-config: default.fuzz.runs = 100
    function testRaindexEvalEmptyNoop(address alice) external {
        checkEntask(alice, new bytes[](0), 0, 0);
    }

    /// forge-config: default.fuzz.runs = 100
    function testRaindexEvalOneStateless(address alice) external {
        bytes[] memory evals = new bytes[](1);
        evals[0] = bytes("_:1;");
        checkEntask(alice, evals, 0, 0);
    }

    /// forge-config: default.fuzz.runs = 100
    function testRaindexEvalOneReadState(address alice) external {
        bytes[] memory evals = new bytes[](1);
        evals[0] = bytes("_:get(0);");
        checkEntask(alice, evals, 2, 1);
    }

    /// forge-config: default.fuzz.runs = 100
    function testRaindexEvalWriteStateSingle(address alice) external {
        bytes[] memory evals0 = new bytes[](1);
        evals0[0] = bytes(":set(1 2);");
        checkEntask(alice, evals0, 1, 1);

        bytes[] memory evals1 = new bytes[](1);
        evals1[0] = bytes(":ensure(equal-to(get(1) 2) \"set works\");");
        checkEntask(alice, evals1, 2, 1);
    }

    /// forge-config: default.fuzz.runs = 100
    function testRaindexEvalWriteStateSequential() external {
        bytes[] memory evals0 = new bytes[](4);
        evals0[0] = bytes(":set(1 2);");
        evals0[1] = bytes(":ensure(equal-to(get(1) 2) \"0th set not equal\");");
        evals0[2] = bytes(":set(2 3);");
        evals0[3] = bytes(":ensure(equal-to(get(2) 3) \"1st set not equal\");");
        checkEntask(address(0), evals0, 6, 4);

        bytes[] memory evals1 = new bytes[](4);
        evals1[0] = bytes(":set(1 20);");
        evals1[1] = bytes(":ensure(equal-to(get(1) 20) \"0th set not equal\");");
        evals1[2] = bytes(":set(2 30);");
        evals1[3] = bytes(":ensure(equal-to(get(2) 30) \"1st set not equal\");");
        checkEntask(address(0), evals1, 6, 4);
    }

    /// forge-config: default.fuzz.runs = 100
    function testRaindexEvalWriteStateDifferentOwnersNamespaced(address alice, address bob) external {
        vm.assume(alice != bob);
        bytes[] memory evals0 = new bytes[](4);
        evals0[0] = bytes(":set(1 2);");
        evals0[1] = bytes(":ensure(equal-to(get(1) 2) \"0th set not equal\");");
        evals0[2] = bytes(":set(2 3);");
        evals0[3] = bytes(":ensure(equal-to(get(2) 3) \"1st set not equal\");");
        checkEntask(alice, evals0, 6, 4);

        bytes[] memory evals1 = new bytes[](4);
        evals1[0] = bytes(":set(1 20);");
        evals1[1] = bytes(":ensure(equal-to(get(1) 20) \"0th set not equal\");");
        evals1[2] = bytes(":set(2 30);");
        evals1[3] = bytes(":ensure(equal-to(get(2) 30) \"1st set not equal\");");
        checkEntask(bob, evals1, 6, 4);

        bytes[] memory evals2 = new bytes[](2);
        evals2[0] = bytes(":ensure(equal-to(get(1) 2) \"alice state 1\");");
        evals2[1] = bytes(":ensure(equal-to(get(2) 3) \"alice state 2\");");
        checkEntask(alice, evals2, 4, 2);

        bytes[] memory evals3 = new bytes[](2);
        evals3[0] = bytes(":ensure(equal-to(get(1) 20) \"bob state 1\");");
        evals3[1] = bytes(":ensure(equal-to(get(2) 30) \"bob state 2\");");
        checkEntask(bob, evals3, 4, 2);
    }

    /// Rainlang's power and sqrt ops call the deployed DecimalFloat.
    function testRaindexEvalDecimalFloatOps() external {
        bytes[] memory evals = new bytes[](2);
        evals[0] = bytes(":ensure(equal-to(power(2 3) 8) \"power\");");
        evals[1] = bytes(":ensure(equal-to(sqrt(16) 4) \"sqrt\");");
        checkEntask(address(this), evals, 0, 0);
    }
}
