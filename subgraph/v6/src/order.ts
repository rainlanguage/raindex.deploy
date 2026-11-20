import { Bytes, ethereum, crypto } from "@graphprotocol/graph-ts";
import { AddOrderV3, RemoveOrderV3 } from "../generated/Raindex/Raindex";
import { AddOrder, Order, RemoveOrder } from "../generated/schema";
import { getVault } from "./vault";
import { eventId } from "./interfaces/event";

export function handleAddOrder(event: AddOrderV3): void {
  createOrderEntity(event);
  createAddOrderEntity(event);
}

export function handleRemoveOrder(event: RemoveOrderV3): void {
  let order = Order.load(makeOrderId(event.address, event.params.orderHash));
  if (order != null) {
    order.active = false;
    order.save();
  }
  createRemoveOrderEntity(event);
}

export function makeOrderId(raindex: Bytes, orderHash: Bytes): Bytes {
  let bytes = raindex.concat(orderHash);
  return Bytes.fromByteArray(crypto.keccak256(bytes));
}

export function createOrderEntity(event: AddOrderV3): void {
  let order = new Order(makeOrderId(event.address, event.params.orderHash));
  order.raindex = event.address;
  order.active = true;
  order.orderHash = event.params.orderHash;
  order.owner = event.params.sender;
  let sender = event.params.sender;

  let inputs: Bytes[] = [];
  let outputs: Bytes[] = [];

  for (let i = 0; i < event.params.order.validInputs.length; i++) {
    let input = event.params.order.validInputs[i];
    let vaultId = input.vaultId;
    let token = input.token;
    let vault = getVault(event.address, sender, vaultId, token).id;
    inputs.push(vault);
  }

  order.inputs = inputs;

  for (let i = 0; i < event.params.order.validOutputs.length; i++) {
    let output = event.params.order.validOutputs[i];
    let vaultId = output.vaultId;
    let token = output.token;
    let vault = getVault(event.address, sender, vaultId, token).id;
    outputs.push(vault);
  }

  order.outputs = outputs;

  order.nonce = event.params.order.nonce;
  order.orderBytes = ethereum.encode(event.parameters[2].value)!;
  order.timestampAdded = event.block.timestamp;
  order.save();
}

export function createAddOrderEntity(event: AddOrderV3): void {
  let addOrder = new AddOrder(eventId(event));
  addOrder.raindex = event.address;
  addOrder.order = makeOrderId(event.address, event.params.orderHash);
  addOrder.sender = event.params.sender;
  addOrder.transaction = event.transaction.hash;
  addOrder.save();
}

export function createRemoveOrderEntity(event: RemoveOrderV3): void {
  let removeOrder = new RemoveOrder(eventId(event));
  removeOrder.raindex = event.address;
  removeOrder.order = makeOrderId(event.address, event.params.orderHash);
  removeOrder.sender = event.params.sender;
  removeOrder.transaction = event.transaction.hash;
  removeOrder.save();
}
