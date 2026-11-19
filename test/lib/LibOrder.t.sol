// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";

import {LibOrder, OrderV4} from "../../src/lib/LibOrder.sol";
import {EvaluableV4} from "rainlang-0.2.39/src/interface/IInterpreterCallerV5.sol";
import {IOV2} from "raindex-interface-0.1.16/src/interface/deprecated/v5/IOrderBookV5.sol";

/// @title LibOrderTest
/// Exercises the LibOrder library.
contract LibOrderTest is Test {
    /// Hashing should always produce the same result for the same input.
    /// forge-config: default.fuzz.runs = 100
    function testHashEqual(OrderV4 memory a) public pure {
        assertTrue(LibOrder.hash(a) == LibOrder.hash(a));
    }

    /// Hashing should always produce different results for different inputs.
    /// forge-config: default.fuzz.runs = 100
    function testHashNotEqual(OrderV4 memory a, OrderV4 memory b) public pure {
        // Only test with actually different inputs.
        vm.assume(keccak256(abi.encode(a)) != keccak256(abi.encode(b)));
        assertTrue(LibOrder.hash(a) != LibOrder.hash(b));
    }

    /// Mutating only the owner should always produce a different hash, proving
    /// the owner is part of the hash preimage.
    /// forge-config: default.fuzz.runs = 100
    function testHashNotEqualMutatedOwner(OrderV4 memory a, address otherOwner) public pure {
        vm.assume(a.owner != otherOwner);
        OrderV4 memory b = OrderV4({
            owner: otherOwner,
            evaluable: a.evaluable,
            validInputs: a.validInputs,
            validOutputs: a.validOutputs,
            nonce: a.nonce
        });
        assertTrue(LibOrder.hash(a) != LibOrder.hash(b));
    }

    /// Mutating only the nonce should always produce a different hash. The
    /// nonce is the order's replay/uniqueness discriminator, so it must be
    /// bound into the hash.
    /// forge-config: default.fuzz.runs = 100
    function testHashNotEqualMutatedNonce(OrderV4 memory a, bytes32 otherNonce) public pure {
        vm.assume(a.nonce != otherNonce);
        OrderV4 memory b = OrderV4({
            owner: a.owner,
            evaluable: a.evaluable,
            validInputs: a.validInputs,
            validOutputs: a.validOutputs,
            nonce: otherNonce
        });
        assertTrue(LibOrder.hash(a) != LibOrder.hash(b));
    }

    /// Mutating only the evaluable should always produce a different hash. The
    /// evaluable binds the order to a specific interpreter/store/bytecode, so it
    /// must be part of the hash preimage.
    /// forge-config: default.fuzz.runs = 100
    function testHashNotEqualMutatedEvaluable(OrderV4 memory a, EvaluableV4 memory otherEvaluable) public pure {
        vm.assume(keccak256(abi.encode(a.evaluable)) != keccak256(abi.encode(otherEvaluable)));
        OrderV4 memory b = OrderV4({
            owner: a.owner,
            evaluable: otherEvaluable,
            validInputs: a.validInputs,
            validOutputs: a.validOutputs,
            nonce: a.nonce
        });
        assertTrue(LibOrder.hash(a) != LibOrder.hash(b));
    }

    /// Mutating only the valid inputs should always produce a different hash.
    /// The input vaults/tokens are part of the order's identity, so they must be
    /// bound into the hash.
    /// forge-config: default.fuzz.runs = 100
    function testHashNotEqualMutatedValidInputs(OrderV4 memory a, IOV2[] memory otherInputs) public pure {
        vm.assume(keccak256(abi.encode(a.validInputs)) != keccak256(abi.encode(otherInputs)));
        OrderV4 memory b = OrderV4({
            owner: a.owner,
            evaluable: a.evaluable,
            validInputs: otherInputs,
            validOutputs: a.validOutputs,
            nonce: a.nonce
        });
        assertTrue(LibOrder.hash(a) != LibOrder.hash(b));
    }

    /// Mutating only the valid outputs should always produce a different hash.
    /// The output vaults/tokens are part of the order's identity, so they must
    /// be bound into the hash.
    /// forge-config: default.fuzz.runs = 100
    function testHashNotEqualMutatedValidOutputs(OrderV4 memory a, IOV2[] memory otherOutputs) public pure {
        vm.assume(keccak256(abi.encode(a.validOutputs)) != keccak256(abi.encode(otherOutputs)));
        OrderV4 memory b = OrderV4({
            owner: a.owner,
            evaluable: a.evaluable,
            validInputs: a.validInputs,
            validOutputs: otherOutputs,
            nonce: a.nonce
        });
        assertTrue(LibOrder.hash(a) != LibOrder.hash(b));
    }
}
