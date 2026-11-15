// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";

import {ChildRaindexV7ArbOrderTaker} from "test/util/concrete/ChildRaindexV7ArbOrderTaker.sol";
import {GenericPoolRaindexV7ArbOrderTaker} from "../../src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol";
import {Float} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";

/// Outside an arb the abstract `RaindexV7ArbOrderTaker.onTakeOrders2` body is a no-op:
/// it neither reverts nor touches state, and it does NOT decode or act on its
/// `bytes` argument. A non-overriding child (`ChildRaindexV7ArbOrderTaker`)
/// exercises the base directly.
contract RaindexV7ArbOrderTakerOnTakeOrders2BaseTest is Test {
    /// Outside an arb the base no-op does nothing: it succeeds for ANY caller and ANY
    /// arguments (including non-decodable garbage `bytes`), and leaves token
    /// balances untouched. Inserting a `revert` into the base body, or making
    /// it decode/act on the data, breaks this.
    function testBaseOnTakeOrders2IsNoop() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        MockToken tokenA = new MockToken("A", "A", 18);
        MockToken tokenB = new MockToken("B", "B", 18);

        ChildRaindexV7ArbOrderTaker arb = new ChildRaindexV7ArbOrderTaker();
        tokenA.mint(address(arb), 7e18);
        tokenB.mint(address(arb), 0);

        // Arbitrary, non-decodable garbage in the data slot. The base no-op must
        // ignore it entirely rather than attempting to decode it as a generic
        // pool exchange (which would revert).
        bytes memory garbage = hex"deadbeefdeadbeef";

        vm.prank(address(0xBEEF));
        arb.onTakeOrders2(
            address(tokenA),
            address(tokenB),
            Float.wrap(bytes32(uint256(123))),
            Float.wrap(bytes32(uint256(456))),
            garbage
        );

        // No state change: balances are exactly as before the call.
        assertEq(tokenA.balanceOf(address(arb)), 7e18, "tokenA balance unchanged by base no-op");
        assertEq(tokenB.balanceOf(address(arb)), 0, "tokenB balance unchanged by base no-op");
    }

    /// `onTakeOrders2` is `virtual override` so a subclass override is what gets
    /// dispatched. The GenericPool concrete overrides it to perform a pool
    /// exchange that decodes the data; calling THAT concrete with the same
    /// non-decodable garbage reverts on the decode. This observably distinguishes
    /// the dispatched override (acts on data) from the base no-op (ignores data),
    /// proving the override — not the base — runs for the subclass.
    function testSubclassOverrideIsDispatchedNotBaseNoop() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        MockToken tokenA = new MockToken("A", "A", 18);
        MockToken tokenB = new MockToken("B", "B", 18);

        GenericPoolRaindexV7ArbOrderTaker arb = new GenericPoolRaindexV7ArbOrderTaker();

        bytes memory garbage = hex"deadbeefdeadbeef";

        // The override decodes `data` as (spender, pool, call); garbage fails the
        // decode and reverts. If the base no-op ran instead it would silently
        // succeed.
        vm.expectRevert();
        arb.onTakeOrders2(address(tokenA), address(tokenB), Float.wrap(0), Float.wrap(0), garbage);
    }
}
