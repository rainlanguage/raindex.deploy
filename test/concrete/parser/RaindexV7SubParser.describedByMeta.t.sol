// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {RaindexV7SubParser} from "../../../src/concrete/parser/RaindexV7SubParser.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {LibEtchRaindex} from "test/util/lib/LibEtchRaindex.sol";

contract RaindexV7SubParserDescribedByMetaV1Test is Test {
    function setUp() public {
        LibEtchRaindex.etchRaindex(vm);
    }

    function testRaindexV7SubParserDescribedByMetaV1Happy() external view {
        bytes memory describedByMeta = vm.readFileBinary("meta/RaindexV7SubParser.rain.meta");
        RaindexV7SubParser subParser = RaindexV7SubParser(LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS);

        assertEq(keccak256(describedByMeta), subParser.describedByMetaV1());
    }
}
