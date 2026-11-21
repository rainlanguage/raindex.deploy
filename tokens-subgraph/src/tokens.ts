import { Address, BigInt } from "@graphprotocol/graph-ts";
import {
  Deposit,
  DepositV2,
} from "../generated/RaindexDeposits/RaindexDeposits";
import { RaindexToken } from "../generated/schema";

// Raindex interface versions 3 and 4 emit `Deposit`.
export function handleDeposit(event: Deposit): void {
  recordToken(event.address, event.params.token, event.block.number);
}

// Raindex interface versions 5 to 7 emit `DepositV2`.
export function handleDepositV2(event: DepositV2): void {
  recordToken(event.address, event.params.token, event.block.number);
}

// One row per (raindex, token), written once: the first deposit wins because
// events are processed in block order.
export function recordToken(raindex: Address, token: Address, block: BigInt): void {
  const id = raindex.concat(token);
  if (RaindexToken.load(id) != null) {
    return;
  }
  const row = new RaindexToken(id);
  row.raindex = raindex;
  row.token = token;
  row.firstSeenBlock = block;
  row.save();
}
