#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-DCL-1.0
# SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd

# Writes manifests/<graph network>.yaml from ../deployments/raindex.json: one
# data source per raindex on that network, from its first block with code,
# handling the deposit and add-order events its interface version emits (3
# `Deposit` + `AddOrder`, 4 `Deposit` + `AddOrderV2`, 5-7 `DepositV2` +
# `AddOrderV3`). Run by script/build.sh, so git-clean fails a stale manifest.
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
        case "$version" in
          3) deposit='Deposit(address,address,uint256,uint256)' dh=handleDeposit
             order='AddOrder(address,address,(address,bool,(address,address,address),(address,uint8,uint256)[],(address,uint8,uint256)[]),bytes32)' oh=handleAddOrder ;;
          4) deposit='Deposit(address,address,uint256,uint256)' dh=handleDeposit
             order='AddOrderV2(address,bytes32,(address,(address,address,bytes),(address,uint8,uint256)[],(address,uint8,uint256)[],bytes32))' oh=handleAddOrderV2 ;;
          5 | 6 | 7) deposit='DepositV2(address,address,bytes32,uint256)' dh=handleDepositV2
             order='AddOrderV3(address,bytes32,(address,(address,address,bytes),(address,bytes32)[],(address,bytes32)[],bytes32))' oh=handleAddOrderV3 ;;
          *) echo "no events for interface version $version" >&2; exit 1 ;;
        esac
        cat <<DS
  - kind: ethereum/contract
    name: Raindex_${address}
    network: ${gn}
    source:
      address: "${address}"
      abi: RaindexTokenEvents
      startBlock: ${start}
    mapping:
      kind: ethereum/events
      apiVersion: 0.0.7
      language: wasm/assemblyscript
      entities:
        - RaindexToken
      abis:
        - name: RaindexTokenEvents
          file: ../abis/RaindexTokenEvents.json
      eventHandlers:
        - event: ${deposit}
          handler: ${dh}
        - event: "${order}"
          handler: ${oh}
      file: ../src/tokens.ts
DS
      done
  } > "manifests/${gn}.yaml"
done
