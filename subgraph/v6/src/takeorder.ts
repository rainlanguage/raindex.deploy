import { Bytes, ethereum } from "@graphprotocol/graph-ts";
import { TakeOrderV3 } from "../generated/Raindex/Raindex";
import { TakeOrder } from "../generated/schema";
import { eventId } from "./interfaces/event";
import { handleVaultBalanceChange, vaultEntityId } from "./vault";
import { createTradeVaultBalanceChangeEntity } from "./tradevaultbalancechange";
import { createTradeEntity } from "./trade";
import { crypto } from "@graphprotocol/graph-ts";
import { getCalculator } from "./float";

export function orderHashFromTakeOrderEvent(event: TakeOrderV3): Bytes {
  let orderHash = crypto.keccak256(
    ethereum.encode(event.parameters[1].value.toTuple()[0])!
  );
  return Bytes.fromByteArray(orderHash);
}

export function handleTakeOrder(event: TakeOrderV3): void {
  let order = event.params.config.order;
  let orderHash = orderHashFromTakeOrderEvent(event);

  // Debit the output vault
  let orderOutput =
    order.validOutputs[event.params.config.outputIOIndex.toU32()];

  const calculator = getCalculator();

  let outVaultBalanceChange = handleVaultBalanceChange(
    event.address,
    orderOutput.vaultId,
    orderOutput.token,
    // input for the taker is a debit for the vault
    calculator.minus(event.params.input),
    order.owner
  );

  let outputVaultBalanceChange = createTradeVaultBalanceChangeEntity(
    event,
    orderHash,
    vaultEntityId(
      event.address,
      order.owner,
      orderOutput.vaultId,
      orderOutput.token
    ),
    outVaultBalanceChange.oldVaultBalance,
    calculator.minus(event.params.input) // change is negative
  );

  // Credit the input vault
  let orderInput = order.validInputs[event.params.config.inputIOIndex.toU32()];

  let inVaultBalance = handleVaultBalanceChange(
    event.address,
    orderInput.vaultId,
    orderInput.token,
    // output for the taker is a credit for the vault
    event.params.output,
    order.owner
  );

  let inputVaultBalanceChange = createTradeVaultBalanceChangeEntity(
    event,
    orderHash,
    vaultEntityId(
      event.address,
      order.owner,
      orderInput.vaultId,
      orderInput.token
    ),
    inVaultBalance.oldVaultBalance,
    event.params.output
  );

  createTradeEntity(
    event,
    orderHash,
    inputVaultBalanceChange,
    outputVaultBalanceChange
  );

  createTakeOrderEntity(event);
}

export function createTakeOrderEntity(event: TakeOrderV3): void {
  let takeOrder = new TakeOrder(eventId(event));
  takeOrder.raindex = event.address;
  takeOrder.inputAmount = event.params.input;
  takeOrder.outputAmount = event.params.output;
  takeOrder.sender = event.params.sender;
  takeOrder.transaction = event.transaction.hash;
  takeOrder.takeOrderConfigBytes = ethereum.encode(event.parameters[1].value)!;
  takeOrder.save();
}
