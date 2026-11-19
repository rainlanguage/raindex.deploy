// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {ArbTest} from "./ArbTest.sol";
import {RouteProcessorRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/RouteProcessorRaindexV7ArbOrderTaker.sol";

contract RouteProcessorRaindexV7ArbOrderTakerTest is ArbTest {
    function buildArb() internal override returns (address payable) {
        return payable(address(new RouteProcessorRaindexV7ArbOrderTaker()));
    }

    constructor() ArbTest() {}
}
