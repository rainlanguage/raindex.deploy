// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7} from "src/concrete/raindex/RaindexV7.sol";
import {IRaindexV7ClearStateChangeHarness} from "test/util/interface/IRaindexV7ClearStateChangeHarness.sol";
import {OrderIOCalculationV4} from "src/struct/OrderIOCalculationV4.sol";
import {ClearStateChangeV2} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";

/// @dev Exposes RaindexV7's internal pure clear-state-change math so it can be
/// tested against a FRESH-COMPILED copy of the source (not the etched committed
/// bytecode the external mock/real suites run, which is blind to src/*.sol
/// mutations).
contract RaindexV7ClearStateChangeHarness is RaindexV7, IRaindexV7ClearStateChangeHarness {
    function exposedCalculateClearStateChange(
        OrderIOCalculationV4 memory aliceOrderIOCalculation,
        OrderIOCalculationV4 memory bobOrderIOCalculation
    ) external pure returns (ClearStateChangeV2 memory) {
        return calculateClearStateChange(aliceOrderIOCalculation, bobOrderIOCalculation);
    }

    function exposedCalculateClearStateAlice(
        OrderIOCalculationV4 memory aliceOrderIOCalculation,
        OrderIOCalculationV4 memory bobOrderIOCalculation
    ) external pure returns (Float, Float) {
        return calculateClearStateAlice(aliceOrderIOCalculation, bobOrderIOCalculation);
    }
}
