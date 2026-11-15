// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {
    IRaindexV7,
    QuoteV3,
    OrderConfigV4,
    OrderV4,
    IOV2,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {Float} from "rain-math-float-0.3.11/src/lib/LibDecimalFloat.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";

/// @title RaindexV7QuoteSameTokenTest
contract RaindexV7QuoteSameTokenTest is RaindexV7ExternalRealTest {
    /// Same token for input and output is error.
    /// forge-config: default.fuzz.runs = 10
    function testQuoteSameToken(QuoteV3 memory quoteConfig) external {
        vm.assume(quoteConfig.order.validInputs.length > 0);
        vm.assume(quoteConfig.order.validOutputs.length > 0);
        // addOrder5 requires a valid serialized parser output with >= 2 sources,
        // so replace the fuzzed bytecode with the canonical valid order bytecode.
        // The self-trade check happens at quote time (TokenSelfTrade below), not
        // add time, so the order still adds successfully.
        quoteConfig.order.evaluable.bytecode = LibTestAddOrder.VALID_ORDER_BYTECODE;
        quoteConfig.order.validInputs[0].token = quoteConfig.order.validOutputs[0].token;
        quoteConfig.inputIOIndex = 0;
        quoteConfig.outputIOIndex = 0;
        vm.prank(quoteConfig.order.owner);
        iRaindex.addOrder5(
            OrderConfigV4({
                evaluable: quoteConfig.order.evaluable,
                validInputs: quoteConfig.order.validInputs,
                validOutputs: quoteConfig.order.validOutputs,
                nonce: quoteConfig.order.nonce,
                secret: bytes32(0),
                meta: ""
            }),
            new TaskV3[](0)
        );
        vm.expectRevert(abi.encodeWithSelector(IRaindexV7.TokenSelfTrade.selector));
        (bool success, Float maxOutput, Float ioRatio) = iRaindex.quote3(quoteConfig);
        (success, maxOutput, ioRatio);
    }

    /// The self-trade check must compare `validInputs[inputIOIndex]` against
    /// `validOutputs[outputIOIndex]`, i.e. the input array is indexed with
    /// `inputIOIndex` and the output array with `outputIOIndex`. This test pins
    /// the index pairing: the self-trade only exists at the (inputIOIndex=0,
    /// outputIOIndex=1) pairing, NOT at (0, 0). Any mutation that reuses a single
    /// index for both lookups, or swaps which array each index addresses, fails
    /// to detect the self-trade and so does NOT revert TokenSelfTrade.
    /// forge-config: default.fuzz.runs = 10
    function testQuoteSameTokenIndexPairing(address owner, OrderConfigV4 memory config) external {
        // Build explicit two-element input and output arrays where the self-trade
        // is only visible when input index 0 is compared against output index 1.
        // tokenA appears at validInputs[0] and validOutputs[1].
        // tokenB at validInputs[1], tokenC at validOutputs[0] are all distinct.
        address tokenA = address(0xAAAA);
        address tokenB = address(0xBBBB);
        address tokenC = address(0xCCCC);

        config.validInputs = new IOV2[](2);
        config.validInputs[0] = IOV2(tokenA, bytes32(uint256(0x11)));
        config.validInputs[1] = IOV2(tokenB, bytes32(uint256(0x12)));
        config.validOutputs = new IOV2[](2);
        config.validOutputs[0] = IOV2(tokenC, bytes32(uint256(0x13)));
        config.validOutputs[1] = IOV2(tokenA, bytes32(uint256(0x14)));

        // conformConfig sets a valid bytecode + interpreter/store and only
        // rewrites token addresses if validInputs[0].token == validOutputs[0].token
        // (tokenA != tokenC here, so our token layout is preserved).
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        assertTrue(config.validInputs[0].token == tokenA, "input0 preserved");
        assertTrue(config.validOutputs[1].token == tokenA, "output1 preserved");
        assertTrue(config.validOutputs[0].token == tokenC, "output0 preserved");

        OrderV4 memory order = OrderV4(owner, config.evaluable, config.validInputs, config.validOutputs, config.nonce);

        vm.prank(owner);
        iRaindex.addOrder5(config, new TaskV3[](0));

        QuoteV3 memory quoteConfig =
            QuoteV3({order: order, inputIOIndex: 0, outputIOIndex: 1, signedContext: new SignedContextV2[](0)});

        // validInputs[0] (tokenA) == validOutputs[1] (tokenA): self-trade.
        vm.expectRevert(abi.encodeWithSelector(IRaindexV7.TokenSelfTrade.selector));
        (bool success, Float maxOutput, Float ioRatio) = iRaindex.quote3(quoteConfig);
        (success, maxOutput, ioRatio);
    }
}
