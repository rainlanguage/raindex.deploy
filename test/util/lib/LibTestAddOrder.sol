// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.19;

import {META_MAGIC_NUMBER_V1} from "rain-metadata-0.1.46/src/lib/LibMeta.sol";
import {LibOrder} from "../../../src/lib/LibOrder.sol";
import {OrderConfigV4, OrderV4, IOV2} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {IInterpreterV4, SourceIndexV2} from "rainlang-0.2.39/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.39/src/interface/IInterpreterStoreV3.sol";
import {EvaluableV4} from "rainlang-0.2.39/src/interface/IInterpreterCallerV5.sol";
import {HANDLE_IO_ENTRYPOINT} from "../../../src/constant/OrderEntrypoints.sol";
import {LibBytecode} from "rainlang-0.2.39/src/lib/bytecode/LibBytecode.sol";

library LibTestAddOrder {
    /// Serialized parser (parse2) output for "_ _:1e18 1e18;:;": the
    /// `[constants length][constants][bytecode length][bytecode]` blob that
    /// `evaluable.bytecode` holds in production, not the bare rain bytecode. This
    /// is the canonical valid order bytecode (two sources: calculate + handle IO)
    /// that the addOrder source-count guard accepts.
    bytes internal constant VALID_ORDER_BYTECODE =
        hex"000000000000000000000000000000000000000000000000000000000000000100000012000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000015020000000c02020002011000000110000000000000";

    /// A little boilerplate to make it easier to build the order that we expect
    /// for a given order config.
    function expectedOrder(address owner, OrderConfigV4 memory config) internal pure returns (OrderV4 memory, bytes32) {
        OrderV4 memory order = OrderV4(owner, config.evaluable, config.validInputs, config.validOutputs, config.nonce);
        return (order, LibOrder.hash(order));
    }

    /// Valid config has a few requirements. Mutates the config in place.
    /// Anything that doesn't meet the requirements will just be set to 0 values
    /// as this is faster than forcing the fuzzer to rebuild with assume.
    function conformConfig(OrderConfigV4 memory config, IInterpreterV4 interpreter, IInterpreterStoreV3 store)
        internal
        pure
    {
        if (config.meta.length > 0) {
            // This is a bit of a hack, but it's the easiest way to get a valid
            // meta document.
            config.meta = abi.encodePacked(META_MAGIC_NUMBER_V1, config.meta);
        }
        config.evaluable.interpreter = interpreter;
        config.evaluable.store = store;
        if (config.validInputs.length == 0) {
            config.validInputs = new IOV2[](1);
            config.validInputs[0] = IOV2(address(0), bytes32(uint256(0x01)));
        }
        if (config.validOutputs.length == 0) {
            config.validOutputs = new IOV2[](1);
            config.validOutputs[0] = IOV2(address(1), bytes32(uint256(0x01)));
        }
        if (config.validInputs[0].token == config.validOutputs[0].token) {
            config.validInputs[0].token = address(0);
            config.validOutputs[0].token = address(1);
        }

        for (uint256 i = 0; i < config.validInputs.length; i++) {
            if (config.validInputs[i].vaultId == bytes32(0)) {
                config.validInputs[i].vaultId = bytes32(uint256(0x01 + i));
            }
        }
        for (uint256 i = 0; i < config.validOutputs.length; i++) {
            if (config.validOutputs[i].vaultId == bytes32(0)) {
                config.validOutputs[i].vaultId = bytes32(uint256(0x01 + i));
            }
        }

        config.evaluable.bytecode = VALID_ORDER_BYTECODE;
    }
}
