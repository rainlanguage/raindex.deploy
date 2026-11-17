// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Build, GeneratedContract, RetiredContract} from "script/Build.sol";
import {DeployCandidate} from "src/abstract/RainDeploySuitesBase.sol";

/// @title BuildHarness
/// @notice An external seam onto the internal declarations `BuildTest`
/// compares, so a test can hold them at once.
///
/// A harness rather than a change to `Build`: `generatedContracts()` is the
/// script's own declaration and has no caller outside it, and widening it to
/// `public` to be testable would put a second entry point on a script whose
/// whole surface is `run()` and `cutRelease()`.
contract BuildHarness is Build {
    /// The generator's list.
    /// @return The generated contracts.
    function externalGeneratedContracts() external view returns (GeneratedContract[] memory) {
        return generatedContracts(declaration());
    }

    /// The names a release cut from this script freezes.
    /// @return The snapshot contract names.
    function externalSnapshotContractNames() external view returns (string[] memory) {
        return snapshotContractNames(generatedContracts(declaration()));
    }

    /// The names the released-suites aggregate is emitted from.
    /// @return The released contract names.
    function externalReleasedContractNames() external view returns (string[] memory) {
        return releasedContractNames(generatedContracts(declaration()));
    }

    /// The contracts no longer built whose records stay released.
    /// @return The retired contracts.
    function externalRetiredContracts() external pure returns (RetiredContract[] memory) {
        return retiredContracts();
    }

    /// The deploy declaration's list, through the same guarded reader every
    /// other consumer of the declaration uses.
    /// @return The declared candidates.
    function externalCandidateSuites() external pure returns (DeployCandidate[] memory) {
        return checkedCandidateSuites(declaration());
    }
}
