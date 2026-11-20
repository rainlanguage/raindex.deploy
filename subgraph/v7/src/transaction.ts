import { Bytes, ethereum } from "@graphprotocol/graph-ts";
import { Transaction } from "../generated/schema";

export function createTransactionEntity(event: ethereum.Event): Bytes {
  let transaction = Transaction.load(event.transaction.hash);
  if (!transaction) {
    transaction = new Transaction(event.transaction.hash);
    transaction.blockNumber = event.block.number;
    transaction.timestamp = event.block.timestamp;
    transaction.from = event.transaction.from;
    transaction.save();
  }
  return transaction.id;
}
