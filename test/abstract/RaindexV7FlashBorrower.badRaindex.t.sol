// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";
import {IERC3156FlashLender} from "raindex-interface-0.1.16/src/interface/ierc3156/IERC3156FlashLender.sol";
import {IRaindexV7} from "raindex-interface-0.1.16/src/interface/IRaindexV7.sol";
import {BadRaindex} from "../../src/abstract/RaindexV7ArbCommon.sol";
import {LibRaindexDeploy} from "../../src/lib/deploy/LibRaindexDeploy.sol";
import {LibTestFlashBorrowerArb, FlashBorrowerSetup} from "test/util/lib/LibTestFlashBorrowerArb.sol";
import {LibTestArb} from "test/util/lib/LibTestArb.sol";
import {MockExchange} from "test/util/concrete/MockExchange.sol";

contract RaindexV7FlashBorrowerBadRaindexTest is Test {
    /// arb5 reverts BadRaindex for any raindex other than the deterministic
    /// deployment, even a funded working lender, before calling it.
    function testArb4RevertsUntrustedRaindex(address badRaindex) external {
        vm.assume(badRaindex != LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS);
        vm.assume(badRaindex != address(0));
        assumeNotPrecompile(badRaindex);
        assumeNotForgeAddress(badRaindex);

        MockExchange exchange = new MockExchange();
        FlashBorrowerSetup memory setup = LibTestFlashBorrowerArb.setup(vm, address(exchange), 100e18);
        vm.assume(badRaindex.code.length == 0);
        vm.assume(badRaindex != address(setup.arb));
        vm.etch(badRaindex, LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS.code);
        setup.outputToken.mint(badRaindex, 1000e18);

        vm.expectCall(badRaindex, abi.encodeWithSelector(IERC3156FlashLender.flashLoan.selector), 0);
        vm.expectRevert(abi.encodeWithSelector(BadRaindex.selector, badRaindex));
        setup.arb.arb5(IRaindexV7(badRaindex), setup.takeOrdersConfig, setup.exchangeData, LibTestArb.noopTask());
    }

    /// arb5 reverts BadRaindex for the zero address.
    function testArb4RevertsZeroRaindex() external {
        MockExchange exchange = new MockExchange();
        FlashBorrowerSetup memory setup = LibTestFlashBorrowerArb.setup(vm, address(exchange), 100e18);

        vm.expectRevert(abi.encodeWithSelector(BadRaindex.selector, address(0)));
        setup.arb.arb5(IRaindexV7(address(0)), setup.takeOrdersConfig, setup.exchangeData, LibTestArb.noopTask());
    }
}
