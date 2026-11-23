// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibRainDeploy} from "rain-deploy-0.1.38/src/lib/LibRainDeploy.sol";
import {ROUTE_PROCESSOR_6_CREATION_CODE} from "../../../src/lib/deploy/LibRouteProcessor6CreationCode.sol";
import {
    LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode,
    AddressPushCount
} from "../../../src/lib/deploy/LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode.sol";
import {
    BYTECODE_HASH as ROUTE_PROCESSOR_6_HASH,
    DEPLOYED_ADDRESS as ROUTE_PROCESSOR_6_ADDRESS,
    RUNTIME_CODE as ROUTE_PROCESSOR_6_RUNTIME_CODE
} from "../../../src/generated/candidate/RouteProcessor6.sol";
import {
    BYTECODE_HASH as RP6_ARB_OT_V6_HASH,
    DEPLOYED_ADDRESS as RP6_ARB_OT_V6_ADDRESS,
    RUNTIME_CODE as RP6_ARB_OT_V6_RUNTIME_CODE
} from "../../../src/generated/candidate/RouteProcessor6RaindexV6ArbOrderTaker.sol";
import {
    CREATION_CODE as RP_ARB_OT_V6_CREATION_CODE,
    RUNTIME_CODE as RP_ARB_OT_V6_RUNTIME_CODE
} from "../../../src/generated/0_1_15/RouteProcessorRaindexV6ArbOrderTaker.sol";
import {DEPLOYED_ADDRESS as ROUTE_PROCESSOR_4_ADDRESS} from "../../../src/generated/0_1_15/RouteProcessor4.sol";
import {DEPLOYED_ADDRESS as RAINDEX_V6_ADDRESS} from "../../../src/generated/0_1_15/RaindexV6.sol";

/// The `RouteProcessor6` getters this test reads.
interface IRouteProcessor6View {
    function owner() external view returns (address);
    function bentoBox() external view returns (address);
}

contract LibRouteProcessor6DeployTest is Test {
    /// External seam so `expectRevert` sees the library revert.
    /// @param code The code.
    /// @param from The address to replace.
    /// @param to The replacement address.
    /// @return The code with the replacement.
    function externalReplaceAddressPush(bytes memory code, address from, address to)
        external
        pure
        returns (bytes memory)
    {
        return LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode.replaceAddressPush(code, from, to);
    }

    /// Deploying RouteProcessor6 via Zoltu MUST produce the generated address,
    /// codehash and runtime code.
    function testDeployAddressRouteProcessor6() external {
        LibRainDeploy.etchZoltuFactory(vm);

        address deployed = LibRainDeploy.deployZoltu(ROUTE_PROCESSOR_6_CREATION_CODE);

        assertEq(deployed, ROUTE_PROCESSOR_6_ADDRESS);
        assertEq(deployed, 0x2b530Fb50B6e5BD8BFB44588c70ca89b0c0A7017);
        assertEq(deployed.codehash, ROUTE_PROCESSOR_6_HASH);
        assertEq(keccak256(deployed.code), keccak256(ROUTE_PROCESSOR_6_RUNTIME_CODE));
    }

    /// RouteProcessor6 deployed via Zoltu has no bento, and the Zoltu factory
    /// is its owner, the same as RouteProcessor4.
    function testRouteProcessor6ConstructorArgs() external {
        LibRainDeploy.etchZoltuFactory(vm);

        IRouteProcessor6View deployed = IRouteProcessor6View(LibRainDeploy.deployZoltu(ROUTE_PROCESSOR_6_CREATION_CODE));

        assertEq(deployed.bentoBox(), address(0));
        assertEq(deployed.owner(), LibRainDeploy.ZOLTU_FACTORY);
    }

    /// Deploying the RouteProcessor6 arb via Zoltu MUST produce the generated
    /// address, codehash and runtime code.
    function testDeployAddressRouteProcessor6ArbOrderTakerV6() external {
        LibRainDeploy.etchZoltuFactory(vm);

        address deployed =
            LibRainDeploy.deployZoltu(LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode.creationCode());

        assertEq(deployed, RP6_ARB_OT_V6_ADDRESS);
        assertEq(deployed, 0x2C03883B066098a4876eBa90D56d200F004D8c2d);
        assertEq(deployed.codehash, RP6_ARB_OT_V6_HASH);
        assertEq(keccak256(deployed.code), keccak256(RP6_ARB_OT_V6_RUNTIME_CODE));
    }

    /// The arb creation code is the `0_1_15` creation code with its one
    /// `PUSH20` of RouteProcessor4 replaced by RouteProcessor6, and its runtime
    /// code is the `0_1_15` runtime code with the same replacement.
    function testRouteProcessor6ArbOrderTakerV6IsThe0115Arb() external pure {
        bytes memory patched = LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode.creationCode();

        assertEq(patched.length, RP_ARB_OT_V6_CREATION_CODE.length);
        assertTrue(keccak256(patched) != keccak256(RP_ARB_OT_V6_CREATION_CODE));
        assertEq(
            keccak256(
                LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode.replaceAddressPush(
                    bytes.concat(patched), ROUTE_PROCESSOR_6_ADDRESS, ROUTE_PROCESSOR_4_ADDRESS
                )
            ),
            keccak256(RP_ARB_OT_V6_CREATION_CODE)
        );
        assertEq(
            keccak256(
                LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode.replaceAddressPush(
                    RP_ARB_OT_V6_RUNTIME_CODE, ROUTE_PROCESSOR_4_ADDRESS, ROUTE_PROCESSOR_6_ADDRESS
                )
            ),
            keccak256(RP6_ARB_OT_V6_RUNTIME_CODE)
        );
    }

    /// The arb keeps the one `PUSH20` of the `0_1_15` RaindexV6, the only
    /// raindex it takes orders from.
    function testRouteProcessor6ArbOrderTakerV6PinsRaindexV6() external pure {
        bytes memory patched = LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode.creationCode();
        assertEq(
            keccak256(
                LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode.replaceAddressPush(
                    bytes.concat(patched), RAINDEX_V6_ADDRESS, RAINDEX_V6_ADDRESS
                )
            ),
            keccak256(patched)
        );
    }

    /// `replaceAddressPush` replaces the one `PUSH20` of `from`, and ignores
    /// `from` without the `PUSH20` before it.
    function testReplaceAddressPushReplacesOne() external pure {
        address from = 0x1111111111111111111111111111111111111111;
        address to = 0x2222222222222222222222222222222222222222;
        bytes memory code = abi.encodePacked(hex"6001", bytes1(0x73), from, hex"5b", from, hex"00");
        bytes memory expected = abi.encodePacked(hex"6001", bytes1(0x73), to, hex"5b", from, hex"00");
        assertEq(LibRouteProcessor6RaindexV6ArbOrderTakerCreationCode.replaceAddressPush(code, from, to), expected);
    }

    /// `replaceAddressPush` refuses code without the `PUSH20` and code with
    /// two.
    function testReplaceAddressPushRefusesNoneOrMany() external {
        address from = 0x1111111111111111111111111111111111111111;
        address to = 0x2222222222222222222222222222222222222222;

        vm.expectRevert(abi.encodeWithSelector(AddressPushCount.selector, 0));
        this.externalReplaceAddressPush(abi.encodePacked(hex"6001", from), from, to);

        vm.expectRevert(abi.encodeWithSelector(AddressPushCount.selector, 2));
        this.externalReplaceAddressPush(abi.encodePacked(bytes1(0x73), from, bytes1(0x73), from), from, to);
    }
}
