import { Address, BigInt } from "@graphprotocol/graph-ts";
import {
  AddOrder,
  AddOrderV2,
  AddOrderV3,
  Deposit,
  DepositV2,
} from "../generated/RaindexTokenEvents/RaindexTokenEvents";
import { RaindexToken } from "../generated/schema";

// Raindex interface versions 3 and 4 emit `Deposit`.
export function handleDeposit(event: Deposit): void {
  recordToken(event.address, event.params.token, event.block.number);
}

// Raindex interface versions 5 to 7 emit `DepositV2`.
export function handleDepositV2(event: DepositV2): void {
  recordToken(event.address, event.params.token, event.block.number);
}

// A token can enter a raindex without ever being deposited: an order's inputs
// arrive by trade. Every input and output of an added order is recorded, so
// those tokens are discovered too.

// Raindex interface version 3 emits `AddOrder`.
export function handleAddOrder(event: AddOrder): void {
  const order = event.params.order;
  for (let i = 0; i < order.validInputs.length; i++) {
    recordToken(event.address, order.validInputs[i].token, event.block.number);
  }
  for (let i = 0; i < order.validOutputs.length; i++) {
    recordToken(event.address, order.validOutputs[i].token, event.block.number);
  }
}

// Raindex interface version 4 emits `AddOrderV2`.
export function handleAddOrderV2(event: AddOrderV2): void {
  const order = event.params.order;
  for (let i = 0; i < order.validInputs.length; i++) {
    recordToken(event.address, order.validInputs[i].token, event.block.number);
  }
  for (let i = 0; i < order.validOutputs.length; i++) {
    recordToken(event.address, order.validOutputs[i].token, event.block.number);
  }
}

// Raindex interface versions 5 to 7 emit `AddOrderV3`.
export function handleAddOrderV3(event: AddOrderV3): void {
  const order = event.params.order;
  for (let i = 0; i < order.validInputs.length; i++) {
    recordToken(event.address, order.validInputs[i].token, event.block.number);
  }
  for (let i = 0; i < order.validOutputs.length; i++) {
    recordToken(event.address, order.validOutputs[i].token, event.block.number);
  }
}

// One row per (raindex, token), written once: the first deposit or order wins
// because events are processed in block order.
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
