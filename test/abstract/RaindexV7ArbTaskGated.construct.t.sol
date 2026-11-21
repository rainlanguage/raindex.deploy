// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {ChildRaindexV7ArbTaskGated} from "test/util/concrete/ChildRaindexV7ArbTaskGated.sol";
import {RaindexV7ArbConfig, RaindexV7ArbCommon} from "../../src/abstract/RaindexV7ArbCommon.sol";
import {TaskV3, EvaluableV4, SignedContextV2} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS
} from "rainlang-0.2.40/test/lib/deploy/LibTestInterpreterDeploy.sol";

/// Construction MUST emit the Construct event with the sender and config.
contract RaindexV7ArbTaskGatedConstructTest is Test {
    function testConstructEmitsEvent(bytes memory bytecode) external {
        TaskV3 memory task = TaskV3(
            EvaluableV4(IInterpreterV4(TEST_INTERPRETER_ADDRESS), IInterpreterStoreV3(TEST_STORE_ADDRESS), bytecode),
            new SignedContextV2[](0)
        );
        RaindexV7ArbConfig memory config = RaindexV7ArbConfig(task);

        vm.expectEmit(false, false, false, true);
        emit RaindexV7ArbCommon.Construct(address(this), config);
        new ChildRaindexV7ArbTaskGated(config);
    }
}
