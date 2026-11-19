// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Script} from "forge-std-1.17.0/src/Script.sol";
import {IMetaBoardV1_2} from "rain-metadata-0.1.46/src/interface/unstable/IMetaBoardV1_2.sol";
import {LibDescribedByMeta} from "rain-metadata-0.1.46/src/lib/LibDescribedByMeta.sol";
import {LibMetaBoardDeploy} from "rain-metadata-deploy-0.1.7/src/lib/LibMetaBoardDeploy.sol";
import {RaindexV7SubParser} from "../src/concrete/parser/RaindexV7SubParser.sol";
import {LibRaindexDeploy} from "../src/lib/deploy/LibRaindexDeploy.sol";

/// @title DescribeSubParser
/// @notice Emits the sub-parser's described-by meta onto the metaboard for the
/// already-deployed `RaindexV7SubParser`. Split out of `script/Deploy.sol`
/// when the deploy moved onto `RainDeployBroadcast`, whose `run()` has no
/// post-deploy hook.
///
/// Emits on exactly ONE network per dispatch, the one `DEPLOYMENT_NETWORK`
/// names — dispatch once per network to describe several. The previous
/// in-line version emitted on whichever fork the deploy loop happened to
/// leave selected last, which was one network chosen by accident; requiring
/// the network to be named makes the same one-network behaviour deliberate.
contract DescribeSubParser is Script {
    /// @dev Entry point. Reads `DEPLOYMENT_NETWORK` (an `[rpc_endpoints]`
    /// alias) and `DEPLOYMENT_KEY` from env, then emits the meta for the
    /// deployed sub-parser address on that network.
    function run() external {
        string memory network = vm.envString("DEPLOYMENT_NETWORK");
        uint256 deployerPrivateKey = vm.envUint("DEPLOYMENT_KEY");
        bytes memory subParserDescribedByMeta = vm.readFileBinary("meta/RaindexV7SubParser.rain.meta");

        vm.createSelectFork(network);
        IMetaBoardV1_2 metaboard = IMetaBoardV1_2(LibMetaBoardDeploy.META_BOARD_DEPLOYED_ADDRESS);
        vm.startBroadcast(deployerPrivateKey);
        LibDescribedByMeta.emitForDescribedAddress(
            metaboard, RaindexV7SubParser(LibRaindexDeploy.SUB_PARSER_DEPLOYED_ADDRESS), subParserDescribedByMeta
        );
        vm.stopBroadcast();
    }
}
