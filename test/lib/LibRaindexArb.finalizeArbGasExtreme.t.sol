// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {LibTestArb, ArbResult} from "test/util/lib/LibTestArb.sol";

contract LibRaindexArbFinalizeArbGasExtremeTest is Test {
    /// finalizeArb must not revert when the swept native-gas balance exceeds the
    /// Float coefficient range (int224, ~1.35e67). The gas column is packed
    /// lossily, so an extreme balance saturates precision instead of reverting
    /// the whole arb on the gas sweep.
    function testFinalizeArbExtremeGasBalanceDoesNotRevert() external {
        // Far beyond int224.max: the previous packLossless(int256(gasBalance),
        // -18) reverted LossyConversionToFloat here; the lossy pack saturates.
        uint256 hugeGas = 1e70;
        vm.deal(address(this), hugeGas);

        // No token profit; hugeGas is sent with arb6, returned by the exchange,
        // and swept by finalizeArb. The arb completes (no revert on the pack).
        ArbResult memory result =
            LibTestArb.setupAndArb(vm, 100e18, 100e18, 100e18, 100e18, LibTestArb.noopTask(), hugeGas);

        assertEq(address(result.arb).balance, 0, "arb gas swept");
    }

    /// Needed to receive the swept gas.
    receive() external payable {}
}
