// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {StackItem} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {EvalV4} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";

contract MockInterpreter {
    StackItem[] internal sStack;

    constructor(StackItem[] memory stack) {
        sStack = stack;
    }

    function eval4(EvalV4 memory) external view returns (StackItem[] memory, bytes32[] memory) {
        return (sStack, new bytes32[](0));
    }
}
