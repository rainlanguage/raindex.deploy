// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RainDeployVerifyDeclaration} from "rain-deploy-0.1.33/src/abstract/RainDeployVerifyDeclaration.sol";
import {ExternalDeployment} from "rain-deploy-0.1.33/src/abstract/RainDeployDeclarationChecks.sol";
import {RaindexDeploySuites} from "../../src/abstract/RaindexDeploySuites.sol";
import {RaindexV7SubParser} from "../../src/concrete/parser/RaindexV7SubParser.sol";
import {GenericPoolRaindexV7ArbOrderTaker} from "../../src/concrete/arb/GenericPoolRaindexV7ArbOrderTaker.sol";
import {RouteProcessorRaindexV7ArbOrderTaker} from "../../src/concrete/arb/RouteProcessorRaindexV7ArbOrderTaker.sol";
import {GenericPoolRaindexV7FlashBorrower} from "../../src/concrete/arb/GenericPoolRaindexV7FlashBorrower.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";

/// @title RaindexDeployDeclarationTest
/// @notice The suite fields only a dispatch, a verifier or a broadcast reads.
contract RaindexDeployDeclarationTest is RaindexDeploySuites, RainDeployVerifyDeclaration {
    function sourceCreationCode(string memory suite) internal view override returns (bytes memory) {
        bytes32 key = keccak256(bytes(suite));
        // `RaindexV7` is via-IR only; importing it here would pull this test under that restriction.
        if (key == keccak256("raindex")) return vm.getCode("src/concrete/raindex/RaindexV7.sol:RaindexV7");
        if (key == keccak256("subparser")) return type(RaindexV7SubParser).creationCode;
        if (key == keccak256("arb-generic-pool-order-taker")) {
            return type(GenericPoolRaindexV7ArbOrderTaker).creationCode;
        }
        if (key == keccak256("arb-route-processor-order-taker")) {
            return type(RouteProcessorRaindexV7ArbOrderTaker).creationCode;
        }
        if (key == keccak256("arb-generic-pool-flash-borrower")) {
            return type(GenericPoolRaindexV7FlashBorrower).creationCode;
        }
        revert(string.concat("no source for suite: ", suite));
    }

    function externalDeployments() internal pure override returns (ExternalDeployment[] memory) {
        ExternalDeployment[] memory externals = new ExternalDeployment[](1);
        externals[0] = ExternalDeployment({
            deployedAddress: address(LibTOFUTokenDecimals.TOFU_DECIMALS_DEPLOYMENT),
            codeHash: LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CODE_HASH
        });
        return externals;
    }
}
