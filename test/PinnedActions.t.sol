// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Test} from "forge-std-1.17.0/src/Test.sol";

/// @title PinnedActionsTest
/// @notice Every third-party GitHub Action referenced by a `uses:` line in
/// `.github/workflows/` must be pinned to a full 40-hex commit SHA, never a
/// mutable tag (a "v4" tag) or branch (a "main" branch). A mutable ref lets
/// whoever controls the upstream tag inject code into our CI the moment it
/// moves (https://github.com/rainlanguage/raindex/issues/620). First-party
/// `rainlanguage/*` shared-CI refs are intentionally excluded -- pinning those
/// is an org-wide decision, not this repo's. A workflow therefore satisfies the
/// invariant either by SHA-pinning a third-party action inline or by delegating
/// to a `rainlanguage/rainix/.github/actions/*` composite that pins it once.
///
/// `script/check-pinned-actions.sh` enumerates the refs (via FFI) and returns
/// `OK` only when every third-party action is SHA-pinned; otherwise it returns
/// `UNPINNED: <ref> ...` listing the offenders. Reintroducing a floating ref
/// (e.g. an unpinned "actions/checkout" at a tag) therefore reds this test
/// with the exact ref.
contract PinnedActionsTest is Test {
    function testEveryThirdPartyActionIsShaPinned() external {
        string[] memory cmd = new string[](2);
        cmd[0] = "bash";
        cmd[1] = "script/check-pinned-actions.sh";
        bytes memory out = vm.ffi(cmd);

        // On failure the actual value is `UNPINNED: <ref> ...`, naming each
        // mutable third-party action ref so the regression is obvious.
        assertEq(string(out), "OK", "a third-party github action is not pinned to a commit sha");
    }
}
