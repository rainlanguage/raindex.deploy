// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test, Vm, console2} from "forge-std-1.17.0/src/Test.sol";
import {REVERTING_MOCK_BYTECODE} from "test/util/lib/LibTestConstants.sol";
import {IRaindexV7Stub} from "test/util/abstract/IRaindexV7Stub.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.36/src/interface/IInterpreterStoreV3.sol";
import {IParserV2} from "rainlang-0.2.36/src/interface/IParserV2.sol";
import {
    IRaindexV7,
    IInterpreterV4,
    TaskV3,
    EvaluableV4,
    SignedContextV2
} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {RaindexV7SubParser} from "../../../src/concrete/parser/RaindexV7SubParser.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";
import {LibDecimalFloat, Float} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {
    LibTestInterpreterDeploy,
    TEST_INTERPRETER_ADDRESS,
    TEST_STORE_ADDRESS,
    TEST_COMPILER_ADDRESS,
    TEST_PARSER_ADDRESS
} from "rainlang-0.2.36/test/lib/deploy/LibTestInterpreterDeploy.sol";
import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";
import {
    CREATION_CODE as DECIMAL_FLOAT_CREATION_CODE
} from "rain-math-float-deploy-0.1.10/src/generated/0_1_5/DecimalFloat.sol";
import {LibRaindexDeploy} from "../../../src/lib/deploy/LibRaindexDeploy.sol";
import {LibEtchRaindex} from "test/util/lib/LibEtchRaindex.sol";

abstract contract RaindexV7ExternalRealTest is Test, IRaindexV7Stub {
    IInterpreterV4 internal immutable iInterpreter;
    IInterpreterStoreV3 internal immutable iStore;
    IParserV2 internal immutable iParserV2;
    IRaindexV7 internal immutable iRaindex;
    IERC20 internal immutable iToken0;
    IERC20 internal immutable iToken1;
    RaindexV7SubParser internal immutable iSubParser;

    constructor() {
        LibRainDeploy.etchZoltuFactory(vm);
        LibRainDeploy.deployZoltu(LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE);
        LibRainDeploy.deployZoltu(DECIMAL_FLOAT_CREATION_CODE);

        LibTestInterpreterDeploy.etchTestRainlang(vm);
        LibEtchRaindex.etchRaindex(vm);

        iInterpreter = IInterpreterV4(TEST_INTERPRETER_ADDRESS);
        iStore = IInterpreterStoreV3(TEST_STORE_ADDRESS);
        iParserV2 = IParserV2(TEST_COMPILER_ADDRESS);

        iRaindex = IRaindexV7(LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);

        iToken0 = IERC20(address(uint160(uint256(keccak256("token0.rain.test")))));
        vm.etch(address(iToken0), REVERTING_MOCK_BYTECODE);
        vm.mockCall(address(iToken0), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(18));

        iToken1 = IERC20(address(uint160(uint256(keccak256("token1.rain.test")))));
        vm.etch(address(iToken1), REVERTING_MOCK_BYTECODE);
        vm.mockCall(address(iToken1), abi.encodeWithSelector(IERC20Metadata.decimals.selector), abi.encode(18));

        iSubParser = RaindexV7SubParser(LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS);
    }

    function assumeEtchable(address account) internal view {
        assumeNotPrecompile(account);
        vm.assume(account != address(iInterpreter));
        vm.assume(account != address(iStore));
        vm.assume(account != address(iParserV2));
        vm.assume(account != TEST_PARSER_ADDRESS);

        vm.assume(account != LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
        vm.assume(account != address(iToken0));
        vm.assume(account != address(iToken1));
        vm.assume(account != LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS);

        vm.assume(account != address(this));
        vm.assume(account != address(vm));
        // The console.
        vm.assume(account != address(0x000000000000000000636F6e736F6c652e6c6f67));
    }

    function evalsToActions(bytes[] memory evals) internal view returns (TaskV3[] memory) {
        TaskV3[] memory actions = new TaskV3[](evals.length);
        for (uint256 i = 0; i < evals.length; i++) {
            actions[i] = TaskV3(EvaluableV4(iInterpreter, iStore, iParserV2.parse2(evals[i])), new SignedContextV2[](0));
        }
        return actions;
    }

    /// Mock the balanceOf and allowance for a vault 0 output vault (owner
    /// sells from wallet). Also mocks transferFrom for pullTokens with any
    /// amount.
    function mockVault0Output(address token, address owner, uint256 depositAmount18) internal {
        vm.mockCall(token, abi.encodeWithSelector(IERC20.balanceOf.selector, owner), abi.encode(depositAmount18));
        vm.mockCall(
            token,
            abi.encodeWithSelector(IERC20.allowance.selector, owner, address(iRaindex)),
            abi.encode(depositAmount18)
        );
        vm.mockCall(
            token, abi.encodeWithSelector(IERC20.transferFrom.selector, owner, address(iRaindex)), abi.encode(true)
        );
    }

    /// Mock the balanceOf and allowance (zero) for a vault 0 input vault
    /// (owner receives to wallet). Mocks the transfer for pushTokens if
    /// the expected amount is non-zero.
    function mockVault0Input(address token, address owner, uint256 expectAmount18) internal {
        vm.mockCall(token, abi.encodeWithSelector(IERC20.balanceOf.selector, owner), abi.encode(0));
        vm.mockCall(token, abi.encodeWithSelector(IERC20.allowance.selector, owner, address(iRaindex)), abi.encode(0));
        if (expectAmount18 > 0) {
            vm.mockCall(
                token, abi.encodeWithSelector(IERC20.transfer.selector, owner, expectAmount18), abi.encode(true)
            );
        }
    }
}
