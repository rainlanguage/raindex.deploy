// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {LibEtchRaindex} from "test/util/lib/LibEtchRaindex.sol";
import {RaindexV7SubParser} from "../../../src/concrete/parser/RaindexV7SubParser.sol";
import {
    CREATION_CODE as RAINDEX_CREATION_CODE,
    RUNTIME_CODE as RAINDEX_RUNTIME_CODE,
    DEPLOYED_ADDRESS as RAINDEX_GENERATED_ADDRESS
} from "../../../src/generated/candidate/RaindexV7.sol";
import {
    CREATION_CODE as SUB_PARSER_CREATION_CODE,
    RUNTIME_CODE as SUB_PARSER_RUNTIME_CODE,
    DEPLOYED_ADDRESS as SUB_PARSER_GENERATED_ADDRESS
} from "../../../src/generated/candidate/RaindexV7SubParser.sol";
import {
    RUNTIME_CODE as ROUTE_PROCESSOR_RUNTIME_CODE,
    DEPLOYED_ADDRESS as ROUTE_PROCESSOR_GENERATED_ADDRESS,
    BYTECODE_HASH as ROUTE_PROCESSOR_GENERATED_CODEHASH
} from "../../../src/generated/candidate/RouteProcessor4.sol";
import {ROUTE_PROCESSOR_4_CREATION_CODE} from "../../../src/lib/deploy/LibRouteProcessor4CreationCode.sol";
import {GenericPoolRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol";
import {RouteProcessorRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/RouteProcessorRaindexV7ArbOrderTaker.sol";
import {GenericPoolRaindexV7FlashBorrower} from "../../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";
import {
    RUNTIME_CODE as GENERIC_POOL_ARB_OT_RUNTIME_CODE,
    DEPLOYED_ADDRESS as GENERIC_POOL_ARB_OT_GENERATED_ADDRESS,
    BYTECODE_HASH as GENERIC_POOL_ARB_OT_GENERATED_CODEHASH
} from "../../../src/generated/candidate/GenericPoolRaindexV7ArbOrderTaker.sol";
import {
    RUNTIME_CODE as RP_ARB_OT_RUNTIME_CODE,
    DEPLOYED_ADDRESS as RP_ARB_OT_GENERATED_ADDRESS,
    BYTECODE_HASH as RP_ARB_OT_GENERATED_CODEHASH
} from "../../../src/generated/candidate/RouteProcessorRaindexV7ArbOrderTaker.sol";
import {
    RUNTIME_CODE as GENERIC_POOL_FB_RUNTIME_CODE,
    DEPLOYED_ADDRESS as GENERIC_POOL_FB_GENERATED_ADDRESS,
    BYTECODE_HASH as GENERIC_POOL_FB_GENERATED_CODEHASH
} from "../../../src/generated/candidate/GenericPoolRaindexV7FlashBorrower.sol";

contract LibRaindexDeployTest is Test {
    /// Deploying RaindexV7 via Zoltu MUST produce the expected address and
    /// codehash.
    function testDeployAddressRaindex() external {
        LibRainDeploy.etchZoltuFactory(vm);

        address deployedAddress = LibRainDeploy.deployZoltu(vm.getCode("RaindexV7.sol:RaindexV7"));

        assertEq(deployedAddress, LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
        assertTrue(address(deployedAddress).code.length > 0, "Deployed address has no code");
        assertEq(address(deployedAddress).codehash, LibRaindexDeploy.RAINDEX_DEPLOYED_CODEHASH);
    }

    /// Deploying RaindexV7SubParser via Zoltu MUST produce the expected
    /// address and codehash.
    function testDeployAddressSubParser() external {
        LibRainDeploy.etchZoltuFactory(vm);

        address deployedAddress = LibRainDeploy.deployZoltu(type(RaindexV7SubParser).creationCode);

        assertEq(deployedAddress, LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS);
        assertTrue(address(deployedAddress).code.length > 0, "Deployed address has no code");
        assertEq(address(deployedAddress).codehash, LibRaindexDeploy.SUB_PARSER_DEPLOYED_CODEHASH);
    }

    /// The codehash of a freshly deployed RaindexV7 MUST match the expected
    /// codehash constant.
    function testExpectedCodeHashRaindex() external {
        address raindex = deployCode("RaindexV7.sol:RaindexV7");
        assertEq(raindex.codehash, LibRaindexDeploy.RAINDEX_DEPLOYED_CODEHASH);
    }

    /// The codehash of a freshly deployed RaindexV7SubParser MUST match the
    /// expected codehash constant.
    function testExpectedCodeHashSubParser() external {
        RaindexV7SubParser subParser = new RaindexV7SubParser();
        assertEq(address(subParser).codehash, LibRaindexDeploy.SUB_PARSER_DEPLOYED_CODEHASH);
    }

    /// The precompiled creation code constant for RaindexV7 MUST match the
    /// compiler's creation code.
    function testCreationCodeRaindex() external view {
        assertEq(keccak256(RAINDEX_CREATION_CODE), keccak256(vm.getCode("RaindexV7.sol:RaindexV7")));
    }

    /// The precompiled creation code constant for RaindexV7SubParser MUST
    /// match the compiler's creation code.
    function testCreationCodeSubParser() external pure {
        assertEq(keccak256(SUB_PARSER_CREATION_CODE), keccak256(type(RaindexV7SubParser).creationCode));
    }

    /// The precompiled runtime code constant for RaindexV7 MUST match the
    /// deployed runtime bytecode.
    function testRuntimeCodeRaindex() external {
        address raindex = deployCode("RaindexV7.sol:RaindexV7");
        assertEq(keccak256(RAINDEX_RUNTIME_CODE), keccak256(raindex.code));
    }

    /// The precompiled runtime code constant for RaindexV7SubParser MUST
    /// match the deployed runtime bytecode.
    function testRuntimeCodeSubParser() external {
        RaindexV7SubParser subParser = new RaindexV7SubParser();
        assertEq(keccak256(SUB_PARSER_RUNTIME_CODE), keccak256(address(subParser).code));
    }

    /// The generated deployed address for RaindexV7 MUST match the deploy
    /// library constant.
    function testGeneratedDeployedAddressRaindex() external pure {
        assertEq(RAINDEX_GENERATED_ADDRESS, LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
    }

    /// The generated deployed address for RaindexV7SubParser MUST match the
    /// deploy library constant.
    function testGeneratedDeployedAddressSubParser() external pure {
        assertEq(SUB_PARSER_GENERATED_ADDRESS, LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS);
    }

    /// Deploying RouteProcessor4 via Zoltu MUST produce the expected address
    /// and codehash.
    function testDeployAddressRouteProcessor() external {
        LibRainDeploy.etchZoltuFactory(vm);

        address deployedAddress = LibRainDeploy.deployZoltu(ROUTE_PROCESSOR_4_CREATION_CODE);

        assertEq(deployedAddress, LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS);
        assertTrue(address(deployedAddress).code.length > 0, "Deployed address has no code");
        assertEq(address(deployedAddress).codehash, LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_CODEHASH);
    }

    /// The precompiled runtime code constant for RouteProcessor4 MUST match
    /// the deployed runtime bytecode.
    function testRuntimeCodeRouteProcessor() external {
        bytes memory creationCode = ROUTE_PROCESSOR_4_CREATION_CODE;
        address deployed;
        assembly ("memory-safe") {
            deployed := create(0, add(creationCode, 0x20), mload(creationCode))
        }
        assertTrue(deployed != address(0), "RouteProcessor4 deployment failed");
        assertEq(keccak256(ROUTE_PROCESSOR_RUNTIME_CODE), keccak256(deployed.code));
    }

    /// The generated deployed address for RouteProcessor4 MUST match the
    /// deploy library constant.
    function testGeneratedDeployedAddressRouteProcessor() external pure {
        assertEq(ROUTE_PROCESSOR_GENERATED_ADDRESS, LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS);
    }

    /// The generated codehash for RouteProcessor4 MUST match the deploy
    /// library constant.
    function testGeneratedCodehashRouteProcessor() external pure {
        assertEq(ROUTE_PROCESSOR_GENERATED_CODEHASH, LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_CODEHASH);
    }

    /// Deploying GenericPoolRaindexV7ArbOrderTaker via Zoltu MUST produce
    /// the expected address and codehash.
    function testDeployAddressGenericPoolArbOrderTaker() external {
        LibRainDeploy.etchZoltuFactory(vm);

        address deployedAddress = LibRainDeploy.deployZoltu(type(GenericPoolRaindexV7ArbOrderTaker).creationCode);

        assertEq(deployedAddress, LibRaindexDeploy.GENERIC_POOL_ARB_ORDER_TAKER_DEPLOYED_ADDRESS);
        assertTrue(address(deployedAddress).code.length > 0, "Deployed address has no code");
        assertEq(address(deployedAddress).codehash, LibRaindexDeploy.GENERIC_POOL_ARB_ORDER_TAKER_DEPLOYED_CODEHASH);
    }

    /// Deploying RouteProcessorRaindexV7ArbOrderTaker via Zoltu MUST produce
    /// the expected address and codehash.
    function testDeployAddressRouteProcessorArbOrderTaker() external {
        LibRainDeploy.etchZoltuFactory(vm);

        address deployedAddress = LibRainDeploy.deployZoltu(type(RouteProcessorRaindexV7ArbOrderTaker).creationCode);

        assertEq(deployedAddress, LibRaindexDeploy.ROUTE_PROCESSOR_ARB_ORDER_TAKER_DEPLOYED_ADDRESS);
        assertTrue(address(deployedAddress).code.length > 0, "Deployed address has no code");
        assertEq(address(deployedAddress).codehash, LibRaindexDeploy.ROUTE_PROCESSOR_ARB_ORDER_TAKER_DEPLOYED_CODEHASH);
    }

    /// Deploying GenericPoolRaindexV7FlashBorrower via Zoltu MUST produce
    /// the expected address and codehash.
    function testDeployAddressGenericPoolFlashBorrower() external {
        LibRainDeploy.etchZoltuFactory(vm);

        address deployedAddress = LibRainDeploy.deployZoltu(type(GenericPoolRaindexV7FlashBorrower).creationCode);

        assertEq(deployedAddress, LibRaindexDeploy.GENERIC_POOL_FLASH_BORROWER_DEPLOYED_ADDRESS);
        assertTrue(address(deployedAddress).code.length > 0, "Deployed address has no code");
        assertEq(address(deployedAddress).codehash, LibRaindexDeploy.GENERIC_POOL_FLASH_BORROWER_DEPLOYED_CODEHASH);
    }

    /// The codehash of a freshly deployed GenericPoolRaindexV7ArbOrderTaker
    /// MUST match the expected codehash constant.
    function testExpectedCodeHashGenericPoolArbOrderTaker() external {
        GenericPoolRaindexV7ArbOrderTaker arb = new GenericPoolRaindexV7ArbOrderTaker();
        assertEq(address(arb).codehash, LibRaindexDeploy.GENERIC_POOL_ARB_ORDER_TAKER_DEPLOYED_CODEHASH);
    }

    /// The codehash of a freshly deployed RouteProcessorRaindexV7ArbOrderTaker
    /// MUST match the expected codehash constant.
    function testExpectedCodeHashRouteProcessorArbOrderTaker() external {
        RouteProcessorRaindexV7ArbOrderTaker arb = new RouteProcessorRaindexV7ArbOrderTaker();
        assertEq(address(arb).codehash, LibRaindexDeploy.ROUTE_PROCESSOR_ARB_ORDER_TAKER_DEPLOYED_CODEHASH);
    }

    /// The codehash of a freshly deployed GenericPoolRaindexV7FlashBorrower
    /// MUST match the expected codehash constant.
    function testExpectedCodeHashGenericPoolFlashBorrower() external {
        GenericPoolRaindexV7FlashBorrower fb = new GenericPoolRaindexV7FlashBorrower();
        assertEq(address(fb).codehash, LibRaindexDeploy.GENERIC_POOL_FLASH_BORROWER_DEPLOYED_CODEHASH);
    }

    /// The precompiled runtime code for GenericPoolRaindexV7ArbOrderTaker
    /// MUST match the deployed runtime bytecode.
    function testRuntimeCodeGenericPoolArbOrderTaker() external {
        GenericPoolRaindexV7ArbOrderTaker arb = new GenericPoolRaindexV7ArbOrderTaker();
        assertEq(keccak256(GENERIC_POOL_ARB_OT_RUNTIME_CODE), keccak256(address(arb).code));
    }

    /// The precompiled runtime code for RouteProcessorRaindexV7ArbOrderTaker
    /// MUST match the deployed runtime bytecode.
    function testRuntimeCodeRouteProcessorArbOrderTaker() external {
        RouteProcessorRaindexV7ArbOrderTaker arb = new RouteProcessorRaindexV7ArbOrderTaker();
        assertEq(keccak256(RP_ARB_OT_RUNTIME_CODE), keccak256(address(arb).code));
    }

    /// The precompiled runtime code for GenericPoolRaindexV7FlashBorrower
    /// MUST match the deployed runtime bytecode.
    function testRuntimeCodeGenericPoolFlashBorrower() external {
        GenericPoolRaindexV7FlashBorrower fb = new GenericPoolRaindexV7FlashBorrower();
        assertEq(keccak256(GENERIC_POOL_FB_RUNTIME_CODE), keccak256(address(fb).code));
    }

    /// The generated deployed address for GenericPoolRaindexV7ArbOrderTaker
    /// MUST match the deploy library constant.
    function testGeneratedDeployedAddressGenericPoolArbOrderTaker() external pure {
        assertEq(GENERIC_POOL_ARB_OT_GENERATED_ADDRESS, LibRaindexDeploy.GENERIC_POOL_ARB_ORDER_TAKER_DEPLOYED_ADDRESS);
    }

    /// The generated deployed address for RouteProcessorRaindexV7ArbOrderTaker
    /// MUST match the deploy library constant.
    function testGeneratedDeployedAddressRouteProcessorArbOrderTaker() external pure {
        assertEq(RP_ARB_OT_GENERATED_ADDRESS, LibRaindexDeploy.ROUTE_PROCESSOR_ARB_ORDER_TAKER_DEPLOYED_ADDRESS);
    }

    /// The generated deployed address for GenericPoolRaindexV7FlashBorrower
    /// MUST match the deploy library constant.
    function testGeneratedDeployedAddressGenericPoolFlashBorrower() external pure {
        assertEq(GENERIC_POOL_FB_GENERATED_ADDRESS, LibRaindexDeploy.GENERIC_POOL_FLASH_BORROWER_DEPLOYED_ADDRESS);
    }

    /// The generated codehash for GenericPoolRaindexV7ArbOrderTaker MUST
    /// match the deploy library constant.
    function testGeneratedCodehashGenericPoolArbOrderTaker() external pure {
        assertEq(
            GENERIC_POOL_ARB_OT_GENERATED_CODEHASH, LibRaindexDeploy.GENERIC_POOL_ARB_ORDER_TAKER_DEPLOYED_CODEHASH
        );
    }

    /// The generated codehash for RouteProcessorRaindexV7ArbOrderTaker MUST
    /// match the deploy library constant.
    function testGeneratedCodehashRouteProcessorArbOrderTaker() external pure {
        assertEq(RP_ARB_OT_GENERATED_CODEHASH, LibRaindexDeploy.ROUTE_PROCESSOR_ARB_ORDER_TAKER_DEPLOYED_CODEHASH);
    }

    /// The generated codehash for GenericPoolRaindexV7FlashBorrower MUST
    /// match the deploy library constant.
    function testGeneratedCodehashGenericPoolFlashBorrower() external pure {
        assertEq(GENERIC_POOL_FB_GENERATED_CODEHASH, LibRaindexDeploy.GENERIC_POOL_FLASH_BORROWER_DEPLOYED_CODEHASH);
    }

    /// After calling etchRaindex, all three contracts MUST have the expected
    /// codehash at their expected addresses.
    function testEtchRaindex() external {
        LibEtchRaindex.etchRaindex(vm);

        assertEq(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS.codehash, LibRaindexDeploy.RAINDEX_DEPLOYED_CODEHASH);
        assertEq(LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS.codehash, LibRaindexDeploy.SUB_PARSER_DEPLOYED_CODEHASH);
        assertEq(
            LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS.codehash,
            LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_CODEHASH
        );
    }

    /// Calling etchRaindex twice MUST be idempotent — codehashes remain
    /// correct on the second call.
    function testEtchRaindexIdempotent() external {
        LibEtchRaindex.etchRaindex(vm);
        LibEtchRaindex.etchRaindex(vm);

        assertEq(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS.codehash, LibRaindexDeploy.RAINDEX_DEPLOYED_CODEHASH);
        assertEq(LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS.codehash, LibRaindexDeploy.SUB_PARSER_DEPLOYED_CODEHASH);
        assertEq(
            LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_ADDRESS.codehash,
            LibRaindexDeploy.ROUTE_PROCESSOR_DEPLOYED_CODEHASH
        );
    }
}
