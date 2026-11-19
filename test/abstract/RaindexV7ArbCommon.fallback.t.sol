// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {GenericPoolRaindexV7ArbOrderTakerTest} from "test/util/abstract/GenericPoolRaindexV7ArbOrderTakerTest.sol";

/// @dev Tests fallback behavior on the order taker arb contract.
contract RaindexV7ArbOrderTakerFallbackTest is GenericPoolRaindexV7ArbOrderTakerTest {
    /// The fallback MUST accept calldata that does not match any selector.
    function testFallbackAcceptsCalldata() external {
        // 0xdeadbeef does not match any function selector on the arb contracts.
        (bool success,) = iArb.call(hex"deadbeef");
        assertTrue(success, "fallback should accept non-matching calldata");
    }

    /// The fallback MUST accept empty calldata.
    function testFallbackAcceptsEmptyCalldata() external {
        (bool success,) = iArb.call("");
        assertTrue(success, "fallback should accept empty calldata");
    }

    /// The receive function MUST accept plain ETH transfers.
    function testReceiveAcceptsETH() external {
        vm.deal(address(this), 1 ether);
        (bool success,) = iArb.call{value: 1}("");
        assertTrue(success, "receive should accept ETH");
    }

    /// The fallback MUST accept ETH with non-matching calldata (payable).
    function testFallbackAcceptsETHWithCalldata() external {
        vm.deal(address(this), 1 ether);
        (bool success,) = iArb.call{value: 1}(hex"deadbeef");
        assertTrue(success, "fallback should accept ETH with calldata");
    }
}
