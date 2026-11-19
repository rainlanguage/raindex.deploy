// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {
    IRaindexV7,
    OrderV4,
    IOV2,
    EvaluableV4,
    ClearConfigV2,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.39/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.39/src/interface/IInterpreterStoreV3.sol";

/// @title RaindexV7ErrorSelectorTest
/// @notice Signature strings are the oracle for the revert data the deployed
/// RaindexV7 artifact reverts with.
contract RaindexV7ErrorSelectorTest is Test {
    IRaindexV7 internal raindex;

    function setUp() external {
        raindex = IRaindexV7(deployCode("RaindexV7.sol:RaindexV7"));
    }

    function order(address owner, address inputToken, address outputToken) internal pure returns (OrderV4 memory) {
        IOV2[] memory inputs = new IOV2[](1);
        inputs[0] = IOV2(inputToken, bytes32(uint256(1)));
        IOV2[] memory outputs = new IOV2[](1);
        outputs[0] = IOV2(outputToken, bytes32(uint256(1)));
        return OrderV4({
            owner: owner,
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), ""),
            validInputs: inputs,
            validOutputs: outputs,
            nonce: bytes32(0)
        });
    }

    function clear(OrderV4 memory alice, OrderV4 memory bob) internal {
        raindex.clear4(
            alice,
            bob,
            ClearConfigV2(0, 0, 0, 0, bytes32(0), bytes32(0)),
            new SignedContextV2[](0),
            new SignedContextV2[](0)
        );
    }

    function testRemoveOrderNotOwnerRevertData(address owner, address sender) external {
        vm.assume(owner != sender);
        vm.prank(sender);
        vm.expectRevert(abi.encodeWithSelector(bytes4(keccak256("NotOrderOwner(address)")), owner));
        raindex.removeOrder4(order(owner, address(1), address(2)), new TaskV3[](0));
    }

    function testClearSameOwnerRevertData(address owner) external {
        vm.expectRevert(abi.encodeWithSelector(bytes4(keccak256("SameOwner()"))));
        clear(order(owner, address(1), address(2)), order(owner, address(2), address(1)));
    }

    function testClearTokenMismatchRevertData(address alice, address bob) external {
        vm.assume(alice != bob);
        vm.expectRevert(abi.encodeWithSelector(bytes4(keccak256("TokenMismatch()"))));
        clear(order(alice, address(1), address(2)), order(bob, address(1), address(2)));
    }
}
