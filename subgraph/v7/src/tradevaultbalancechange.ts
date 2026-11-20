import { BigInt, Bytes, ethereum, crypto } from "@graphprotocol/graph-ts";
import { TradeVaultBalanceChange } from "../generated/schema";
import { eventId } from "./interfaces/event";
import { makeTradeId } from "./trade";
import { Float, getCalculator } from "./float";

export function tradeVaultBalanceChangeId(
  event: ethereum.Event,
  vaultEntityId: Bytes
): Bytes {
  let bytes = eventId(event).concat(vaultEntityId);
  return Bytes.fromByteArray(crypto.keccak256(bytes));
}

export function createTradeVaultBalanceChangeEntity(
  event: ethereum.Event,
  orderHash: Bytes,
  vaultEntityId: Bytes,
  oldVaultBalance: Float,
  amount: Float
): TradeVaultBalanceChange {
  const calculator = getCalculator();

  let tradeVaultBalanceChange = new TradeVaultBalanceChange(
    tradeVaultBalanceChangeId(event, vaultEntityId)
  );
  tradeVaultBalanceChange.raindex = event.address;
  tradeVaultBalanceChange.amount = amount;
  tradeVaultBalanceChange.oldVaultBalance = oldVaultBalance;
  tradeVaultBalanceChange.newVaultBalance = calculator.add(
    oldVaultBalance,
    amount
  );
  tradeVaultBalanceChange.vault = vaultEntityId;
  tradeVaultBalanceChange.trade = makeTradeId(event, orderHash);
  tradeVaultBalanceChange.timestamp = event.block.timestamp;
  tradeVaultBalanceChange.transaction = event.transaction.hash;
  tradeVaultBalanceChange.save();
  return tradeVaultBalanceChange;
}
