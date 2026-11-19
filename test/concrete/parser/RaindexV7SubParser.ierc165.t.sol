// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";

import {IERC165} from "@openzeppelin-contracts-5.7.0/utils/introspection/IERC165.sol";
import {RaindexV7SubParser} from "../../../src/concrete/parser/RaindexV7SubParser.sol";
import {ISubParserV4} from "rainlang-0.2.36/src/interface/ISubParserV4.sol";
import {IDescribedByMetaV1} from "rain-metadata-0.1.46/src/interface/IDescribedByMetaV1.sol";
import {IParserToolingV1} from "rain-sol-codegen-0.1.39/src/interface/IParserToolingV1.sol";
import {ISubParserToolingV1} from "rain-sol-codegen-0.1.39/src/interface/ISubParserToolingV1.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {LibEtchRaindex} from "test/util/lib/LibEtchRaindex.sol";

contract RaindexV7SubParserIERC165Test is Test {
    function setUp() public {
        LibEtchRaindex.etchRaindex(vm);
    }

    function testRaindexV7SubParserIERC165(bytes4 badInterfaceId) external view {
        vm.assume(badInterfaceId != type(IERC165).interfaceId);
        vm.assume(badInterfaceId != type(ISubParserV4).interfaceId);
        vm.assume(badInterfaceId != type(IDescribedByMetaV1).interfaceId);
        vm.assume(badInterfaceId != type(IParserToolingV1).interfaceId);
        vm.assume(badInterfaceId != type(ISubParserToolingV1).interfaceId);

        RaindexV7SubParser subParser = RaindexV7SubParser(LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS);
        assertTrue(subParser.supportsInterface(type(IERC165).interfaceId));
        assertTrue(subParser.supportsInterface(type(ISubParserV4).interfaceId));
        assertTrue(subParser.supportsInterface(type(IDescribedByMetaV1).interfaceId));
        assertTrue(subParser.supportsInterface(type(IParserToolingV1).interfaceId));
        assertTrue(subParser.supportsInterface(type(ISubParserToolingV1).interfaceId));
        assertFalse(subParser.supportsInterface(badInterfaceId));
    }
}
