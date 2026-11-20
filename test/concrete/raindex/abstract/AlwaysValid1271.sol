// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

/// @dev EIP-1271 always-valid signer bytecode target. Etched over the real
/// oracle signer so tests can mutate unsigned frames and still clear
/// `LibContext` auth, then hit the rainlang `ensure` messages.
contract AlwaysValid1271 {
    bytes4 internal constant MAGIC = 0x1626ba7e;

    function isValidSignature(bytes32, bytes calldata) external pure returns (bytes4) {
        return MAGIC;
    }
}
