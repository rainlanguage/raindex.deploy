// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Script} from "forge-std-1.17.0/src/Script.sol";

contract DiagOrder is Script {
    function run() external {
        vm.createSelectFork(""); // rpc url
        vm.rollFork(1234); // block number
        address to = address(0); // put arb contract address
        address from = address(0); // sender address
        bytes memory data = hex""; // put calldata here without 0x

        vm.startPrank(from);
        (bool success, bytes memory result) = to.call(data);
        (success, result);
    }
}
