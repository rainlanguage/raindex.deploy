// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {LibRaindexArb} from "src/lib/LibRaindexArb.sol";
import {TaskV3, SignedContextV2, EvaluableV4} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.39/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.39/src/interface/IInterpreterStoreV3.sol";

/// @dev Exposes the internal `finalizeArb` so its zero-balance sweep guards can
/// be exercised directly. `finalizeArb` sends to `msg.sender`, which here is the
/// caller of `callFinalize` (the test, or a reverting-receiver proxy).
contract LibRaindexArbFinalizeHarness {
    function callFinalize(address inputToken, uint8 inputDecimals, address outputToken, uint8 outputDecimals) external {
        TaskV3 memory task = TaskV3({
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            signedContext: new SignedContextV2[](0)
        });
        LibRaindexArb.finalizeArb(task, inputToken, inputDecimals, outputToken, outputDecimals);
    }

    receive() external payable {}
}
