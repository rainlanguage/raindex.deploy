#!/bin/bash
# SPDX-License-Identifier: LicenseRef-DCL-1.0
# SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd

# Post-CopyArtifacts hook for rainix-copy-artifacts. Regenerates all committed
# derived artifacts: subgraph ABIs + codegen, bindings ABIs (read by `sol!` in
# crates/bindings/src/lib.rs), test_fixtures ABIs + IMulticall3.sol. Strips
# forge JSON to just `.abi` (or `.abi + .bytecode` where alloy ::deploy()
# needs it) — bytecode/metadata/id embed build-host paths and aren't
# deterministic across runners, which would defeat the diff gate; consumers
# (alloy sol!, graph CLI, matchstick) only read those fields anyway.

set -euxo pipefail

cd "$(dirname "$0")/.."

# `forge script script/Build.sol` stages the generated network config here,
# because forge refuses filesystem writes to the root `foundry.toml`.
staged=.staged-config
if [ ! -d "$staged" ] || [ -z "$(find "$staged" -mindepth 1 -maxdepth 1 -type f)" ]; then
  echo "::error::$staged/ is missing or empty; run forge script ./script/Build.sol first." >&2
  exit 1
fi
find "$staged" -mindepth 1 -maxdepth 1 -type f -exec cp {} . \;
rm -rf "$staged"

# Pass the whole pipeline via `bash -c '...'` rather than `bash <<INNER`:
# graph codegen / npm ci consume the heredoc body off stdin otherwise,
# turning the rest of the script into bogus argv for the codegen command.
nix develop .#subgraph-shell -c bash -c '
set -euxo pipefail

mkdir -p subgraph/abis
jq "{abi}" out/RaindexV7.sol/RaindexV7.json > subgraph/abis/Raindex.json
jq "{abi}" out/ERC20.sol/ERC20.json > subgraph/abis/ERC20.json
jq "{abi}" out/DecimalFloat.sol/DecimalFloat.json > subgraph/abis/DecimalFloat.json

(cd subgraph && npm ci && graph codegen)

mkdir -p crates/bindings/abis
# Sources the raindex compiler profile also compiles get per-profile artifacts.
jq "{abi}" out/IRaindexV7.sol/IRaindexV7.default.json > crates/bindings/abis/IRaindexV7.json
jq "{abi}" out/RaindexV7.sol/RaindexV7.json > crates/bindings/abis/RaindexV7.json
jq "{abi}" out/ERC20.sol/ERC20.json > crates/bindings/abis/ERC20.json
jq "{abi}" out/IERC20Metadata.sol/IERC20Metadata.default.json > crates/bindings/abis/IERC20Metadata.json
jq "{abi}" out/IInterpreterStoreV3.sol/IInterpreterStoreV3.default.json > crates/bindings/abis/IInterpreterStoreV3.json

mkdir -p crates/test_fixtures/abis
# bytecode.sourceMap embeds a file-ID that depends on solc input ordering and
# differs across runners — drop it (debug-only; alloy ::deploy() only reads
# bytecode.object).
jq "{abi, bytecode: (.bytecode | {object, linkReferences})}" out/RaindexV7.sol/RaindexV7.json > crates/test_fixtures/abis/RaindexV7.json
jq "{abi, bytecode: (.bytecode | {object, linkReferences})}" out/RaindexV7SubParser.sol/RaindexV7SubParser.json > crates/test_fixtures/abis/RaindexV7SubParser.json

mkdir -p crates/test_fixtures/contracts
cp dependencies/forge-std-1.17.0/src/interfaces/IMulticall3.sol crates/test_fixtures/contracts/IMulticall3.sol
'
