#!/bin/bash
# SPDX-License-Identifier: LicenseRef-DCL-1.0
# SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd

# Refresh the RaindexV7 deploy-block bookkeeping after a release.
#
# Finds the exact block at which the newest released RaindexV7 (the last entry
# of LibRaindexV7Released, else of LibRaindexV6Released) was deployed on each mainnet by binary-searching
# `eth_getCode` over an archive RPC, then rewrites the new per-chain blocks +
# that address into:
#   - the RAINDEX_START_BLOCK_* constants in src/lib/deploy/LibRaindexV7StartBlock.sol
#   - subgraph/networks.json (address + startBlock per network)
#   - subgraph/subgraph.yaml (data source address)
#
# RaindexV7 is CREATE2-deployed, so its address only ever holds the one bytecode
# it was created with: the first block with code at that address IS the deploy
# block. Per-chain RPCs come from the foundry rpc_endpoints env vars, falling
# back to public archive endpoints. Set BUILD_START_BLOCKS_INPUT to a file of
# `STARTBLOCK <network> <block>` lines to skip the (RPC-bound) search, e.g. for
# tests.
set -euo pipefail

LIB="src/lib/deploy/LibRaindexV7StartBlock.sol"
NET="subgraph/networks.json"
YAML="subgraph/subgraph.yaml"
# Releases cut before the rename are recorded as RaindexV6.
for NAME in RaindexV7 RaindexV6; do
  RELEASED="src/lib/Lib${NAME}Released.sol"
  TAG=$(grep -aoE "storedDeployedAddress: ${NAME}_[A-Za-z0-9_]+_DEPLOYED_ADDRESS" "$RELEASED" | tail -1 \
      | sed -E "s/.*${NAME}_(.+)_DEPLOYED_ADDRESS/\1/" || true)
  [ -n "$TAG" ] && break
done
PTR="src/generated/${TAG}/${NAME}.sol"
[ -n "$TAG" ] && [ -f "$PTR" ] || { echo "could not read the newest released RaindexV7 snapshot from $RELEASED" >&2; exit 1; }

ARB_RPC="${ARBITRUM_RPC_URL:-https://arbitrum.drpc.org}"
BASE_RPC="${BASE_RPC_URL:-https://mainnet.base.org}"
FLARE_RPC="${FLARE_RPC_URL:-https://flare-api.flare.network/ext/C/rpc}"
POLY_RPC="${POLYGON_RPC_URL:-https://polygon.drpc.org}"

ADDR=$(grep -aoE 'DEPLOYED_ADDRESS = address\(0x[0-9a-fA-F]{40}' "$PTR" | grep -aoE '0x[0-9a-fA-F]{40}')
[ -n "$ADDR" ] || { echo "could not read RAINDEX_DEPLOYED_ADDRESS from $PTR" >&2; exit 1; }

# rpc_call <rpc> <method> <params-json> -> raw JSON (empty on transport error).
rpc_call() {
    curl -fsSL -m 25 -X POST "$1" -H 'Content-Type: application/json' \
        -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$2\",\"params\":$3}" 2>/dev/null || true
}

# code_at <rpc> <addr> <block-dec> -> yes | no | err. Distinguishes an RPC error
# (retryable) from a genuinely empty account, so the search never mistakes an
# archive-window/timeout boundary for the deploy block.
code_at() {
    local hex res
    hex=$(printf '0x%x' "$3")
    res=$(rpc_call "$1" eth_getCode "[\"$2\",\"$hex\"]")
    case "$res" in
    *'"result":"0x"'*) echo no ;;
    *'"result":"0x'*) echo yes ;;
    *) echo err ;;
    esac
}

head_of() {
    local res
    res=$(rpc_call "$1" eth_blockNumber "[]")
    printf '%d' "$(printf '%s' "$res" | grep -aoE '"result":"0x[0-9a-f]+"' | grep -aoE '0x[0-9a-f]+')"
}

# find_deploy_block <rpc> <addr> -> first block with code (the CREATE2 deploy).
find_deploy_block() {
    local rpc=$1 addr=$2 head delta floor lo hi mid errs=0
    head=$(head_of "$rpc")
    [ "${head:-0}" -gt 0 ] || { echo "could not read head block from $rpc" >&2; return 1; }
    [ "$(code_at "$rpc" "$addr" "$head")" = "yes" ] || { echo "no code for $addr at head on $rpc" >&2; return 1; }

    # Walk a lower bound back in growing steps until the address has no code,
    # so the search holds however long ago the deploy happened.
    delta=250000
    floor=$(( head > delta ? head - delta : 1 ))
    while [ "$floor" -gt 1 ] && [ "$(code_at "$rpc" "$addr" "$floor")" = "yes" ]; do
        delta=$(( delta * 4 ))
        floor=$(( head > delta ? head - delta : 1 ))
    done

    lo=$floor; hi=$head
    while [ "$lo" -lt "$hi" ]; do
        mid=$(( (lo + hi) / 2 ))
        case "$(code_at "$rpc" "$addr" "$mid")" in
        yes) hi=$mid ;;
        no) lo=$(( mid + 1 )) ;;
        err)
            errs=$(( errs + 1 ))
            [ "$errs" -gt 12 ] && { echo "too many RPC errors searching $rpc" >&2; return 1; }
            sleep 2
            ;;
        esac
    done

    # Verify the boundary on a second, independent read to catch load-balanced
    # archives that answer the same historical block inconsistently.
    [ "$(code_at "$rpc" "$addr" "$lo")" = "yes" ] || { echo "boundary check failed: no code at $lo on $rpc" >&2; return 1; }
    [ "$lo" -le 1 ] || [ "$(code_at "$rpc" "$addr" "$(( lo - 1 ))")" = "no" ] \
        || { echo "boundary check failed: code already at $(( lo - 1 )) on $rpc" >&2; return 1; }
    echo "$lo"
}

if [ -n "${BUILD_START_BLOCKS_INPUT:-}" ]; then
    OUT=$(cat "$BUILD_START_BLOCKS_INPUT")
else
    echo "finding RaindexV7 ($ADDR) deploy block on each chain..." >&2
    OUT="STARTBLOCK arbitrum $(find_deploy_block "$ARB_RPC" "$ADDR")
STARTBLOCK base $(find_deploy_block "$BASE_RPC" "$ADDR")
STARTBLOCK flare $(find_deploy_block "$FLARE_RPC" "$ADDR")
STARTBLOCK polygon $(find_deploy_block "$POLY_RPC" "$ADDR")"
fi

get() { printf '%s\n' "$OUT" | grep -aoE "STARTBLOCK $1 [0-9]+" | grep -aoE '[0-9]+$' | head -1; }
ARB=$(get arbitrum); BASE=$(get base); FLARE=$(get flare); POLY=$(get polygon)
for pair in "arbitrum:$ARB" "base:$BASE" "flare:$FLARE" "polygon:$POLY"; do
    [ -n "${pair#*:}" ] || { echo "missing start block for ${pair%%:*}; finder output:" >&2; printf '%s\n' "$OUT" >&2; exit 1; }
done

sed -i -E "s/(RAINDEX_START_BLOCK_ARBITRUM = )[0-9]+/\1${ARB}/" "$LIB"
sed -i -E "s/(RAINDEX_START_BLOCK_BASE = )[0-9]+/\1${BASE}/" "$LIB"
sed -i -E "s/(RAINDEX_START_BLOCK_FLARE = )[0-9]+/\1${FLARE}/" "$LIB"
sed -i -E "s/(RAINDEX_START_BLOCK_POLYGON = )[0-9]+/\1${POLY}/" "$LIB"

sed -i -E "s/(address: \")0x[0-9a-fA-F]+(\")/\1${ADDR}\2/" "$YAML"

jq --arg addr "$ADDR" --argjson arb "$ARB" --argjson base "$BASE" --argjson flare "$FLARE" --argjson poly "$POLY" '
    ."arbitrum-one".Raindex = {address: $addr, startBlock: $arb}
    | .matic.Raindex = {address: $addr, startBlock: $poly}
    | .base.Raindex = {address: $addr, startBlock: $base}
    | .flare.Raindex = {address: $addr, startBlock: $flare}
' "$NET" >"${NET}.tmp" && mv "${NET}.tmp" "$NET"

echo "start blocks: arbitrum=${ARB} base=${BASE} flare=${FLARE} polygon=${POLY}  address=${ADDR}"
