// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {DeployDeclaration, RainDeploySuitesBase} from "rain-deploy-0.1.38/src/abstract/RainDeploySuitesBase.sol";
import {LibRainDeployDeclaration} from "rain-deploy-0.1.38/src/lib/LibRainDeployDeclaration.sol";
import {RaindexDeploySuites} from "../src/abstract/RaindexDeploySuites.sol";
import {Deploy} from "./Deploy.sol";

/// @title DeployZerog
/// @notice `Deploy` with the broadcast narrowed to `zerog`. Dispatch
/// `Manual sol artifacts` with `script` set to
/// `script/DeployZerog.sol:DeployZerog` to deploy a suite that only `zerog`
/// requires, such as the `RouteProcessor6` suites.
contract DeployZerog is Deploy {
    /// @return The declaration, with `deployNetworks` set to `zerogNetworks()`.
    function declaration()
        internal
        pure
        override(RainDeploySuitesBase, RaindexDeploySuites)
        returns (DeployDeclaration memory)
    {
        DeployDeclaration memory d = super.declaration();
        LibRainDeployDeclaration.setDeployNetworks(d, zerogNetworks());
        return d;
    }
}
