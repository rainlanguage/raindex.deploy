// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {EvalV4, StackItem} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";

/// @dev A fixed-stack interpreter: `eval4` returns the configured stack and no
/// KVs. Used to drive `calculateOrderIO`'s read-back of (IORatio, outputMax)
/// from stack[0]/stack[1] and the MIN_OUTPUTS guard independent of any real
/// expression. Standalone (not etched) so the harness's order can point at it.
contract FixedStackInterpreter {
    bytes32[] internal sStack;

    function setStack(bytes32[] memory stack) external {
        sStack = stack;
    }

    function eval4(EvalV4 calldata) external view returns (StackItem[] memory, bytes32[] memory) {
        bytes32[] memory stack = sStack;
        StackItem[] memory items = new StackItem[](stack.length);
        for (uint256 i = 0; i < stack.length; i++) {
            items[i] = StackItem.wrap(stack[i]);
        }
        return (items, new bytes32[](0));
    }
}
