// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {DeploySuite} from "rain-deploy-0.1.32/src/abstract/RainDeploySuitesBase.sol";
import {LibRaindexV6Released} from "src/lib/LibRaindexV6Released.sol";
import {LibRaindexV7Released} from "src/lib/LibRaindexV7Released.sol";

library LibRaindexV7NewestRelease {
    function suite() internal pure returns (DeploySuite memory) {
        DeploySuite[] memory released = LibRaindexV7Released.releasedSuites();
        if (released.length == 0) {
            released = LibRaindexV6Released.releasedSuites();
        }
        return released[released.length - 1];
    }
}
