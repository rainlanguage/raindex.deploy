#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-DCL-1.0
# SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd

# Writes manifests/<graph network>.yaml from ../deployments/raindex.json: one
# data source per raindex on that network, from its first block with code,
# handling the deposit event its interface version emits (3-4 `Deposit`,
# 5-7 `DepositV2`). Run by script/build.sh, so git-clean fails a stale manifest.
set -euo pipefail
cd "$(dirname "$0")"

# The repo's rpc alias name -> the graph network name.
graph_network() {
  case "$1" in
    arbitrum) echo arbitrum-one ;;
    base) echo base ;;
    base_sepolia) echo base-sepolia ;;
    bsc) echo bsc ;;
    ethereum) echo mainnet ;;
    flare) echo flare ;;
    hyperevm) echo hyperevm ;;
    linea) echo linea ;;
    matchain) echo matchain ;;
    polygon) echo matic ;;
    robinhood) echo robinhood ;;
    zerog) echo 0g ;;
    *) echo "no graph network for $1" >&2; return 1 ;;
  esac
}

rm -rf manifests
mkdir -p manifests
for network in $(jq -r '[.[].network] | unique | .[]' ../deployments/raindex.json); do
  gn="$(graph_network "$network")"
  {
    printf 'specVersion: "0.0.4"\nschema:\n  file: ../schema.graphql\ndataSources:\n'
    jq -r --arg n "$network" '.[] | select(.network == $n) | [.address, .interfaceVersion, .startBlock] | @tsv' \
      ../deployments/raindex.json |
      while IFS=$'\t' read -r address version start; do
        if [ "$version" -le 4 ]; then
          event='Deposit(address,address,uint256,uint256)' handler=handleDeposit
        else
          event='DepositV2(address,address,bytes32,uint256)' handler=handleDepositV2
        fi
        cat <<DS
  - kind: ethereum/contract
    name: Raindex_${address}
    network: ${gn}
    source:
      address: "${address}"
      abi: RaindexDeposits
      startBlock: ${start}
    mapping:
      kind: ethereum/events
      apiVersion: 0.0.7
      language: wasm/assemblyscript
      entities:
        - RaindexToken
      abis:
        - name: RaindexDeposits
          file: ../abis/RaindexDeposits.json
      eventHandlers:
        - event: ${event}
          handler: ${handler}
      file: ../src/tokens.ts
DS
      done
  } > "manifests/${gn}.yaml"
done
