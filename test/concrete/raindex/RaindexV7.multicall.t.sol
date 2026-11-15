// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {IRaindexV7, TaskV3} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float, LibDecimalFloat} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// `multicall` is inherited from OpenZeppelin and not declared on the Raindex
/// interface, so reach it through a minimal local interface.
interface IMulticall {
    function multicall(bytes[] calldata data) external returns (bytes[] memory results);
}

/// @title RaindexV7MulticallTest
/// @notice Audit A08-12 (#2535): RaindexV7 inherits OpenZeppelin `Multicall`
/// but nothing verified bundled calls behave correctly. These tests confirm
/// (1) two state-changing calls bundled into one transaction both apply and
/// are credited to the original caller (`msg.sender` survives the internal
/// delegatecall), and (2) the bundle is atomic: if any sub-call reverts the
/// whole multicall reverts and no earlier sub-call's state persists.
contract RaindexV7MulticallTest is RaindexV7ExternalRealTest {
    using LibDecimalFloat for Float;

    address internal immutable iAlice = address(uint160(uint256(keccak256("alice.rain.test"))));

    /// Two deposits to different vaults, bundled into a single `multicall`,
    /// both land and are credited to alice. The returned `results` array has
    /// one (empty, since `deposit5` is void) entry per call.
    function testMulticallBundledDepositsBothApply() external {
        Float amount1 = LibDecimalFloat.packLossless(3, 0);
        Float amount2 = LibDecimalFloat.packLossless(5, 0);

        // Any pull of token1 from alice succeeds.
        vm.mockCall(
            address(iToken1),
            abi.encodeWithSelector(IERC20.transferFrom.selector, iAlice, address(iRaindex)),
            abi.encode(true)
        );

        bytes[] memory calls = new bytes[](2);
        calls[0] =
            abi.encodeCall(IRaindexV7.deposit5, (address(iToken1), bytes32(uint256(0x01)), amount1, new TaskV3[](0)));
        calls[1] =
            abi.encodeCall(IRaindexV7.deposit5, (address(iToken1), bytes32(uint256(0x02)), amount2, new TaskV3[](0)));

        vm.prank(iAlice);
        bytes[] memory results = IMulticall(address(iRaindex)).multicall(calls);
        assertEq(results.length, 2, "one result per bundled call");

        // Both deposits applied, each credited to alice in its own vault.
        assertTrue(
            iRaindex.vaultBalance2(iAlice, address(iToken1), bytes32(uint256(0x01))).eq(amount1), "vault 1 balance"
        );
        assertTrue(
            iRaindex.vaultBalance2(iAlice, address(iToken1), bytes32(uint256(0x02))).eq(amount2), "vault 2 balance"
        );
    }

    /// If a later sub-call reverts, the whole `multicall` reverts and the
    /// earlier sub-call's deposit is rolled back. The first deposit (token1)
    /// would succeed on its own, but the second deposit pulls token0 whose
    /// transfer reverts, so the bundle must leave vault 1 empty.
    function testMulticallRevertsAtomically() external {
        Float amount1 = LibDecimalFloat.packLossless(3, 0);
        Float amount2 = LibDecimalFloat.packLossless(5, 0);

        // token1 pull would succeed...
        vm.mockCall(
            address(iToken1),
            abi.encodeWithSelector(IERC20.transferFrom.selector, iAlice, address(iRaindex)),
            abi.encode(true)
        );
        // ...but token0 pull reverts, failing the second bundled call.
        vm.mockCallRevert(
            address(iToken0), abi.encodeWithSelector(IERC20.transferFrom.selector), "token0 pull rejected"
        );

        bytes[] memory calls = new bytes[](2);
        calls[0] =
            abi.encodeCall(IRaindexV7.deposit5, (address(iToken1), bytes32(uint256(0x01)), amount1, new TaskV3[](0)));
        calls[1] =
            abi.encodeCall(IRaindexV7.deposit5, (address(iToken0), bytes32(uint256(0x02)), amount2, new TaskV3[](0)));

        vm.prank(iAlice);
        vm.expectRevert();
        IMulticall(address(iRaindex)).multicall(calls);

        // The first deposit must have been rolled back with the whole tx.
        assertTrue(
            iRaindex.vaultBalance2(iAlice, address(iToken1), bytes32(uint256(0x01))).isZero(), "vault 1 rolled back"
        );
    }
}
