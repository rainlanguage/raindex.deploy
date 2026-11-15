// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {StackItem} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {Float} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";
import {EvalV4} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";

/// A constructable interpreter mock that returns [ratio, max] for the calculate
/// entrypoint and an empty stack for handle IO.
contract CalcInterpreter {
    StackItem[] internal sCalcStack;

    constructor(Float ratio, Float max) {
        sCalcStack.push(StackItem.wrap(Float.unwrap(ratio)));
        sCalcStack.push(StackItem.wrap(Float.unwrap(max)));
    }

    function eval4(EvalV4 memory) external view returns (StackItem[] memory, bytes32[] memory) {
        // The calculate entrypoint reads [ratio, max] off this stack; the handle
        // IO entrypoint discards the returned stack, so returning the same stack
        // for both is harmless.
        return (sCalcStack, new bytes32[](0));
    }
}
