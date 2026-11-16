// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Vm} from "forge-std-1.17.0/src/Vm.sol";
import {SignedContextV2} from "rainlang-0.2.36/src/interface/IInterpreterCallerV5.sol";
import {LibContext} from "rainlang-0.2.36/src/lib/caller/LibContext.sol";
import {MessageHashUtils} from "@openzeppelin-contracts-5.7.0/utils/cryptography/MessageHashUtils.sol";

library LibTestSignedContext {
    /// @dev EIP-712 signs `context` by `pk` under `domainSeparator`.
    function sign(Vm vm, uint256 pk, bytes32 domainSeparator, bytes32[] memory context)
        internal
        pure
        returns (SignedContextV2 memory)
    {
        SignedContextV2 memory signedContext = SignedContextV2(vm.addr(pk), domainSeparator, context, "");
        (uint8 v, bytes32 r, bytes32 s) =
            vm.sign(pk, MessageHashUtils.toTypedDataHash(domainSeparator, LibContext.hashStruct(signedContext)));
        signedContext.signature = abi.encodePacked(r, s, v);
        return signedContext;
    }
}
