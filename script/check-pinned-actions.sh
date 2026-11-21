#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-DCL-1.0
# SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
#
# Prints "OK" iff every third-party GitHub Action referenced by a `uses:` line
# in `.github/workflows/` is pinned to a full 40-hex commit SHA rather than a
# mutable ref (a branch like `@main` or a tag like `@v4`). Mutable refs are the
# supply-chain hole this guards: whoever controls the upstream tag controls the
# code our CI runs, so a compromised `@v4` runs in our pipeline the moment it
# moves.
#
# First-party `rainlanguage/*` refs are excluded: the org's shared CI (rainix /
# github-chore reusable workflows + composite actions) is intentionally tracked
# at `@main`, so pinning those is an org-wide decision rather than this repo's.
# The shared third-party actions used by the nix/cachix CI preamble are wrapped
# in rainix composite actions that pin each SHA once, so a workflow satisfies
# this check either by SHA-pinning a third-party action inline or by delegating
# to a `rainlanguage/rainix/.github/actions/*` composite that already pins it.
#
# Consumed by `test/PinnedActions.t.sol` via FFI. Output is one of:
#   OK                       - every third-party action is SHA-pinned
#   UNPINNED: <ref> ...       - one or more refs use a mutable tag/branch
#
# Always exits 0 so the test sees the message rather than an ffi failure.

set -euo pipefail

workflows_dir=".github/workflows"

# Extract every `uses:` ref. Tolerates leading `- ` and arbitrary indentation,
# quoted or unquoted values, and trailing `# comment`.
refs=$(
  grep -rhoE '^[[:space:]]*-?[[:space:]]*uses:[[:space:]]*[^[:space:]#]+' "$workflows_dir" \
    | sed -E 's/.*uses:[[:space:]]*//; s/["'\'']//g'
)

unpinned=""
while IFS= read -r ref; do
  [ -z "$ref" ] && continue

  # Skip first-party shared CI (intentionally tracked at @main).
  case "$ref" in
    rainlanguage/*) continue ;;
  esac

  # Local actions (`./...`) and docker refs are not tag-pinnable here.
  case "$ref" in
    ./*) continue ;;
    docker://*) continue ;;
  esac

  # Split owner/repo[/path]@ref on the LAST '@'.
  pin="${ref##*@}"

  # A pinned ref is exactly 40 lowercase hex chars (a full commit SHA).
  if ! printf '%s' "$pin" | grep -qE '^[0-9a-f]{40}$'; then
    unpinned="$unpinned $ref"
  fi
done <<<"$refs"

if [ -n "$unpinned" ]; then
  printf 'UNPINNED:%s' "$unpinned"
  exit 0
fi

printf 'OK'
