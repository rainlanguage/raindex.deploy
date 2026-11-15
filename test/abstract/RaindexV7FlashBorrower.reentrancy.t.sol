// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";

import {GenericPoolRaindexV7FlashBorrower} from "../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";
import {
    IRaindexV7,
    TakeOrdersConfigV6,
    TakeOrderConfigV5,
    OrderV4,
    IOV2,
    EvaluableV4,
    SignedContextV2,
    TaskV3
} from "raindex-interface-0.1.13/src/interface/IRaindexV7.sol";
import {IInterpreterV4} from "rainlang-0.2.35/src/interface/IInterpreterV4.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.35/src/interface/IInterpreterStoreV3.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibRaindexDeploy} from "../../src/lib/deploy/LibRaindexDeploy.sol";
import {MockToken} from "test/util/concrete/MockToken.sol";
import {FlashLendingMockRaindex} from "test/util/concrete/FlashLendingMockRaindex.sol";
import {ReentrantExchange} from "test/util/concrete/ReentrantExchange.sol";

contract RaindexV7FlashBorrowerReentrancyTest is Test {
    /// arb5 MUST revert when re-entered via the pool call in _exchange.
    function testArb4Reentrancy() external {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);

        MockToken inputToken = new MockToken("Input", "IN", 18);
        MockToken outputToken = new MockToken("Output", "OUT", 18);

        // The flash borrower checks msg.sender == RAINDEX_DEPLOYED_ADDRESS in
        // onFlashLoan. Deploy a FlashLendingMockRaindex and etch it there.
        FlashLendingMockRaindex raindex = new FlashLendingMockRaindex();
        vm.etch(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, address(raindex).code);

        // Mint output tokens to the etched Raindex so it can flash-lend them.
        outputToken.mint(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS, 100e18);

        GenericPoolRaindexV7FlashBorrower arb = new GenericPoolRaindexV7FlashBorrower();

        // Deploy the reentrant exchange that will call arb5 when invoked.
        ReentrantExchange exchange = new ReentrantExchange(arb, IRaindexV7(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS));

        IOV2[] memory validInputs = new IOV2[](1);
        validInputs[0] = IOV2(address(inputToken), bytes32(0));
        IOV2[] memory validOutputs = new IOV2[](1);
        validOutputs[0] = IOV2(address(outputToken), bytes32(0));

        OrderV4 memory order = OrderV4({
            owner: address(0x1234),
            evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
            validInputs: validInputs,
            validOutputs: validOutputs,
            nonce: bytes32(0)
        });

        TakeOrderConfigV5[] memory orders = new TakeOrderConfigV5[](1);
        orders[0] = TakeOrderConfigV5(order, 0, 0, new SignedContextV2[](0));

        // exchangeData: spender=exchange, pool=exchange, callData=empty (triggers fallback).
        bytes memory exchangeData = abi.encode(address(exchange), address(exchange), hex"");

        vm.expectRevert(abi.encodeWithSelector(bytes4(keccak256("ReentrancyGuardReentrantCall()"))));
        arb.arb5(
            IRaindexV7(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS),
            TakeOrdersConfigV6({
                minimumIO: LibDecimalFloat.packLossless(100, 0),
                maximumIO: LibDecimalFloat.packLossless(type(int224).max, 0),
                maximumIORatio: LibDecimalFloat.packLossless(type(int224).max, 0),
                IOIsInput: true,
                orders: orders,
                data: ""
            }),
            exchangeData,
            TaskV3({
                evaluable: EvaluableV4(IInterpreterV4(address(0)), IInterpreterStoreV3(address(0)), hex""),
                signedContext: new SignedContextV2[](0)
            })
        );
    }
}
