// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {EvaluableV4, SignedContextV2, TaskV3} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {IParserV2} from "rainlang-0.2.40/src/interface/IParserV2.sol";
import {
    LibTestInterpreterDeploy,
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS,
    TEST_COMPILER_ADDRESS
} from "rainlang-0.2.40/test/lib/deploy/LibTestInterpreterDeploy.sol";
import {LibTestSignedContext} from "test/util/lib/LibTestSignedContext.sol";
import {Strings} from "@openzeppelin-contracts-5.7.0/utils/Strings.sol";
import {LibTestArb} from "test/util/lib/LibTestArb.sol";
import {LibRaindexDeploy} from "src/lib/deploy/LibRaindexDeploy.sol";
import {RUNTIME_CODE as SUB_PARSER_RUNTIME_CODE} from "src/generated/candidate/RaindexV7SubParser.sol";

/// @title LibRaindexArbFinalizeArbTaskSignedContextTest
/// @notice `signer<i>()`, `signer-domain<i>()` and `signed-context<i j>()` read
/// the real signers, domains and signed data in the post-arb task.
contract LibRaindexArbFinalizeArbTaskSignedContextTest is Test {
    using Strings for address;
    using Strings for uint256;

    uint256 internal constant SIGNER_A_PK = 0xA11CE;
    uint256 internal constant SIGNER_B_PK = 0xB0B;
    address internal constant DATA_A = address(0xDA7A0);
    address internal constant DATA_B = address(0xDA7A1);
    address internal constant TRUSTED = address(0x7777);
    bytes32 internal constant DOMAIN_A = keccak256("domain a");
    bytes32 internal constant DOMAIN_B = keccak256("domain b");

    function signed(uint256 pk, bytes32 domain, address datum) internal pure returns (SignedContextV2 memory) {
        bytes32[] memory ctx = new bytes32[](1);
        ctx[0] = bytes32(uint256(uint160(datum)));
        return LibTestSignedContext.sign(vm, pk, domain, ctx);
    }

    function task(string memory body, SignedContextV2[] memory sc) internal returns (TaskV3 memory) {
        LibTestInterpreterDeploy.etchTestRainlang(vm);
        vm.etch(LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS, SUB_PARSER_RUNTIME_CODE);
        bytes memory bytecode = IParserV2(TEST_COMPILER_ADDRESS)
            .parse2(
                bytes(
                    string.concat(
                        "using-words-from ", LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS.toHexString(), "\n", body
                    )
                )
            );
        return TaskV3({
            evaluable: EvaluableV4(
                IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), bytecode
            ),
            signedContext: sc
        });
    }

    function testFinalizeArbTaskSignerWordsReadRealSigners() external {
        SignedContextV2[] memory sc = new SignedContextV2[](2);
        sc[0] = signed(SIGNER_A_PK, DOMAIN_A, DATA_A);
        sc[1] = signed(SIGNER_B_PK, DOMAIN_B, DATA_B);
        TaskV3 memory t = task(
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
        LibTestArb.setupAndArb(vm, 80e18, 100e18, 100e18, 100e18, t, 0);
    }

    function testFinalizeArbTaskRejectsSignerForgedInSignedData() external {
        SignedContextV2[] memory sc = new SignedContextV2[](3);
        sc[0] = signed(SIGNER_A_PK, DOMAIN_A, DATA_A);
        sc[1] = signed(SIGNER_A_PK, DOMAIN_A, DATA_A);
        sc[2] = signed(SIGNER_A_PK, DOMAIN_A, TRUSTED);
        TaskV3 memory t =
            task(string.concat(":ensure(equal-to(signer<0>() ", TRUSTED.toHexString(), ") \"signer trusted\");"), sc);
        vm.expectRevert("signer trusted");
        this.externalArb(t);
    }

    function externalArb(TaskV3 memory t) external {
        LibTestArb.setupAndArb(vm, 80e18, 100e18, 100e18, 100e18, t, 0);
    }
}
