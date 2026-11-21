import {
  AddOrderV3,
  AfterClearV2,
  ClearV3,
  DepositV2,
  MetaV1_2,
  RemoveOrderV3,
  TakeOrderV3
} from "../generated/Raindex/Raindex";
import { WithdrawV2 } from "../generated/Raindex/Raindex";
import { handleDeposit as _handleDeposit } from "./deposit";
import { handleWithdraw as _handleWithdraw } from "./withdraw";
import {
  handleAddOrder as _handleAddOrder,
  handleRemoveOrder as _handleRemoveOrder
} from "./order";
import { handleMeta as _handleMeta } from "./meta";
import { handleTakeOrder as _handleTakeOrder } from "./takeorder";
import {
  handleClear as _handleClear,
  handleAfterClear as _handleAfterClear
} from "./clear";
import { createTransactionEntity } from "./transaction";
import { createRaindexEntity } from "./raindex";

export function handleDeposit(event: DepositV2): void {
  createTransactionEntity(event);
  createRaindexEntity(event);
  _handleDeposit(event);
}

export function handleWithdraw(event: WithdrawV2): void {
  createTransactionEntity(event);
  createRaindexEntity(event);
  _handleWithdraw(event);
}

export function handleAddOrder(event: AddOrderV3): void {
  createTransactionEntity(event);
  createRaindexEntity(event);
  _handleAddOrder(event);
}

export function handleRemoveOrder(event: RemoveOrderV3): void {
  createTransactionEntity(event);
  createRaindexEntity(event);
  _handleRemoveOrder(event);
}

export function handleTakeOrder(event: TakeOrderV3): void {
  createTransactionEntity(event);
  createRaindexEntity(event);
  _handleTakeOrder(event);
}

export function handleMeta(event: MetaV1_2): void {
  createTransactionEntity(event);
  createRaindexEntity(event);
  _handleMeta(event);
}

export function handleClear(event: ClearV3): void {
  createTransactionEntity(event);
  createRaindexEntity(event);
  _handleClear(event);
}

export function handleAfterClear(event: AfterClearV2): void {
  createTransactionEntity(event);
  createRaindexEntity(event);
  _handleAfterClear(event);
}
