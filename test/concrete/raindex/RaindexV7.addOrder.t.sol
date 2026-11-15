// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7ExternalRealTest} from "test/util/abstract/RaindexV7ExternalRealTest.sol";
import {LibTestAddOrder} from "test/util/lib/LibTestAddOrder.sol";
import {OrderConfigV4, EvaluableV4, TaskV3} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IParserV2} from "rainlang-0.2.35/src/interface/IParserV2.sol";
import {UnsupportedCalculateOutputs} from "../../../src/error/ErrRaindexV7.sol";

/// @title RaindexV7AddOrderTest
/// @notice A test harness for testing the RaindexV7 addOrder function.
contract RaindexV7AddOrderTest is RaindexV7ExternalRealTest {
    /// An order with no sources is unevaluable so it reverts at add time.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderRealNoSourcesReverts(address owner, OrderConfigV4 memory config) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        config.evaluable.bytecode = hex"";
        vm.prank(owner);
        vm.expectRevert(abi.encodeWithSelector(OrderNoSources.selector));
        iRaindex.addOrder5(config, new TaskV3[](0));
        (, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        assertTrue(!iRaindex.orderExists(orderHash));
    }

    /// An order with no handle IO entrypoint is unevaluable so it reverts at add
    /// time.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderRealNoHandleIOReverts(address owner, OrderConfigV4 memory config) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        // A single calculate source with no handle IO source: one source, so it
        // passes the no-sources check but trips the no-handle-IO check.
        bytes memory bytecode = iParserV2.parse2("_ _:1 1;");
        config.evaluable.bytecode = bytecode;
        vm.prank(owner);
        vm.expectRevert(abi.encodeWithSelector(OrderNoHandleIO.selector));
        iRaindex.addOrder5(config, new TaskV3[](0));
        (, bytes32 orderHash) = LibTestAddOrder.expectedOrder(owner, config);
        assertTrue(!iRaindex.orderExists(orderHash));
    }

    /// A stack of 0 for calculate order deploys.
    /// Stack size checks are runtime.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderRealZeroStackCalculateReverts(address owner, OrderConfigV4 memory config) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        bytes memory bytecode = iParserV2.parse2(":;:;");
        config.evaluable.bytecode = bytecode;
        vm.prank(owner);
        iRaindex.addOrder5(config, new TaskV3[](0));
    }

    /// A stack of 1 for calculate order reverts.
    /// Stack size checks are runtime.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderRealOneStackCalculateReverts(address owner, OrderConfigV4 memory config) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        bytes memory bytecode = iParserV2.parse2("_:block-timestamp();:;");
        config.evaluable.bytecode = bytecode;
        vm.prank(owner);
        iRaindex.addOrder5(config, new TaskV3[](0));
    }

    /// A stack of 2 for calculate order deploys.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderRealTwoStackCalculateReverts(address owner, OrderConfigV4 memory config) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        bytes memory bytecode = iParserV2.parse2("_ _:block-timestamp() chain-id();:;");
        config.evaluable.bytecode = bytecode;
        vm.prank(owner);
        iRaindex.addOrder5(config, new TaskV3[](0));
    }

    /// A stack of 3 for calculate order deploys.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderRealThreeStackCalculate(address owner, OrderConfigV4 memory config) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        bytes memory bytecode = iParserV2.parse2("_ _ _:block-timestamp() chain-id() block-number();:;");
        config.evaluable.bytecode = bytecode;
        vm.prank(owner);
        iRaindex.addOrder5(config, new TaskV3[](0));
    }

    /// Inputs for calculate order. Tests one input.
    /// Deploys because this is a runtime check.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderRealCalculateInputsReverts1(address owner, OrderConfigV4 memory config) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        bytes memory bytecode = iParserV2.parse2("i:;:;");
        config.evaluable.bytecode = bytecode;
        vm.prank(owner);
        iRaindex.addOrder5(config, new TaskV3[](0));
    }

    /// Inputs for calculate order errors. Tests two inputs.
    /// Deploys because this is a runtime check.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderRealCalculateInputsReverts2(address owner, OrderConfigV4 memory config) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        bytes memory bytecode = iParserV2.parse2("i0 i1:;:;");
        config.evaluable.bytecode = bytecode;
        vm.prank(owner);
        iRaindex.addOrder5(config, new TaskV3[](0));
    }

    /// Inputs for calculate order errors. This takes precedent over the same
    /// error in handle io.
    /// Deploys because this is a runtime check.
    /// forge-config: default.fuzz.runs = 100
    function testAddOrderRealCalculateInputsRevertsPreference(address owner, OrderConfigV4 memory config) public {
        LibTestAddOrder.conformConfig(config, iInterpreter, iStore);
        bytes memory bytecode = iParserV2.parse2("i:;i:;");
        config.evaluable.bytecode = bytecode;
        vm.prank(owner);
        iRaindex.addOrder5(config, new TaskV3[](0));
    }
}
