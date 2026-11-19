// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {ArbTest} from "./ArbTest.sol";
import {GenericPoolRaindexV7ArbOrderTaker} from "../../../src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol";

contract GenericPoolRaindexV7ArbOrderTakerTest is ArbTest {
    function buildArb() internal override returns (address payable) {
        return payable(address(new GenericPoolRaindexV7ArbOrderTaker()));
    }

    constructor() ArbTest() {}
}
