// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {RaindexV7SubParser} from "../../../src/concrete/parser/RaindexV7SubParser.sol";
import {DESCRIBED_BY_META_HASH} from "../../../src/generated/RaindexV7SubParserPointers.sol";

/// @title RaindexV7SubParserForwardersTest
/// @notice Pins the small external view/pure entrypoints of
/// `RaindexV7SubParser` that simply forward a constant or a freshly built
/// value. These are exercised against a freshly deployed instance (not the
/// etched runtime), so the freshly compiled source is what runs and no pointer
/// regeneration is required to observe a mutation.
contract RaindexV7SubParserForwardersTest is Test {
    RaindexV7SubParser internal subParser;

    function setUp() public {
        subParser = new RaindexV7SubParser();
    }

    /// `buildLiteralParserFunctionPointers` must return the empty byte string.
    /// Raindex provides no custom literal parsers, so the literal parser
    /// function pointers are empty. A mutation that returns any non-empty bytes
    /// is caught by the exact equality with "".
    function testBuildLiteralParserFunctionPointersEmpty() external view {
        assertEq(subParser.buildLiteralParserFunctionPointers(), "");
    }

    /// `describedByMetaV1` must forward the committed `DESCRIBED_BY_META_HASH`
    /// constant verbatim. Asserted against a fresh deployment so the freshly
    /// compiled forwarder body is what runs.
    function testDescribedByMetaV1ForwardsConstant() external view {
        assertEq(subParser.describedByMetaV1(), DESCRIBED_BY_META_HASH);
    }
}
