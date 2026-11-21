import { newMockEvent } from "matchstick-as";
import {
  BigInt,
  ethereum,
  Address,
  Bytes,
  Value,
} from "@graphprotocol/graph-ts";
import {
  AddOrderV3,
  ClearV3,
  AfterClearV2,
  DepositV2,
  MetaV1_2,
  RemoveOrderV3,
  TakeOrderV3,
} from "../generated/Raindex/Raindex";
import { WithdrawV2 } from "../generated/Raindex/Raindex";
import { createTransactionEntity } from "../src/transaction";
import { Float } from "../src/float";

// event DepositV2(address sender, address token, bytes32 vaultId, uint256 depositAmountUint256);
export function createDepositEvent(
  sender: Address,
  token: Address,
  vaultId: Bytes,
  amount: BigInt
): DepositV2 {
  let mockEvent = newMockEvent();
  let depositEvent = new DepositV2(
    mockEvent.address,
    mockEvent.logIndex,
    mockEvent.transactionLogIndex,
    mockEvent.logType,
    mockEvent.block,
    mockEvent.transaction,
    mockEvent.parameters,
    null
  );
  depositEvent.parameters = new Array();
  depositEvent.parameters.push(
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(sender))
  );
  depositEvent.parameters.push(
    new ethereum.EventParam("token", ethereum.Value.fromAddress(token))
  );
  depositEvent.parameters.push(
    new ethereum.EventParam("vaultId", ethereum.Value.fromBytes(vaultId))
  );
  depositEvent.parameters.push(
    new ethereum.EventParam("amount", ethereum.Value.fromUnsignedBigInt(amount))
  );

  createTransactionEntity(depositEvent);
  return depositEvent;
}

// event WithdrawV2(
//   address sender,
//   address token,
//   bytes32 vaultId,
//   Float targetAmount,
//   Float withdrawAmount,
//   uint256 withdrawAmountUint256
// );
export function createWithdrawEvent(
  sender: Address,
  token: Address,
  vaultId: Bytes,
  targetAmount: Float,
  amount: Float,
  withdrawAmountUint256: BigInt
): WithdrawV2 {
  let mockEvent = newMockEvent();
  let withdrawalEvent = new WithdrawV2(
    mockEvent.address,
    mockEvent.logIndex,
    mockEvent.transactionLogIndex,
    mockEvent.logType,
    mockEvent.block,
    mockEvent.transaction,
    mockEvent.parameters,
    null
  );
  withdrawalEvent.parameters = new Array();
  withdrawalEvent.parameters.push(
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(sender))
  );
  withdrawalEvent.parameters.push(
    new ethereum.EventParam("token", ethereum.Value.fromAddress(token))
  );
  withdrawalEvent.parameters.push(
    new ethereum.EventParam("vaultId", ethereum.Value.fromBytes(vaultId))
  );
  withdrawalEvent.parameters.push(
    new ethereum.EventParam(
      "targetAmount",
      ethereum.Value.fromBytes(targetAmount)
    )
  );
  withdrawalEvent.parameters.push(
    new ethereum.EventParam("withdrawAmount", ethereum.Value.fromBytes(amount))
  );
  withdrawalEvent.parameters.push(
    new ethereum.EventParam(
      "withdrawAmountUint256",
      ethereum.Value.fromUnsignedBigInt(withdrawAmountUint256)
    )
  );

  createTransactionEntity(withdrawalEvent);
  return withdrawalEvent;
}

export class IOV2 {
  token: Address;
  vaultId: Bytes;

  constructor(token: Address, vaultId: Bytes) {
    this.token = token;
    this.vaultId = vaultId;
  }
}

export class Evaluable {
  interpreter: Address;
  store: Address;
  bytecode: Bytes;

  constructor(interpreter: Address, store: Address, bytecode: Bytes) {
    this.interpreter = interpreter;
    this.store = store;
    this.bytecode = bytecode;
  }
}

export function createOrder(
  owner: Address,
  evaluable: Evaluable,
  validInputs: Array<IOV2>,
  validOutputs: Array<IOV2>,
  nonce: Bytes
): ethereum.Tuple {
  let _evaluable = new ethereum.Tuple();
  _evaluable.push(ethereum.Value.fromAddress(evaluable.interpreter));
  _evaluable.push(ethereum.Value.fromAddress(evaluable.store));
  _evaluable.push(ethereum.Value.fromBytes(evaluable.bytecode));

  let _validInputs = validInputs.map<ethereum.Tuple>((input) => {
    let _input = new ethereum.Tuple();
    _input.push(ethereum.Value.fromAddress(input.token));
    _input.push(ethereum.Value.fromBytes(input.vaultId));
    return _input;
  });

  let _validOutputs = validOutputs.map<ethereum.Tuple>((output) => {
    let _output = new ethereum.Tuple();
    _output.push(ethereum.Value.fromAddress(output.token));
    _output.push(ethereum.Value.fromBytes(output.vaultId));
    return _output;
  });

  let order = new ethereum.Tuple();
  order.push(ethereum.Value.fromAddress(owner));
  order.push(ethereum.Value.fromTuple(_evaluable));
  order.push(ethereum.Value.fromTupleArray(_validInputs));
  order.push(ethereum.Value.fromTupleArray(_validOutputs));
  order.push(ethereum.Value.fromFixedBytes(nonce));

  return order;
}

// event AddOrderV2(address sender, bytes32 orderHash, Order order);
export function createAddOrderEvent(
  sender: Address,
  orderHash: Bytes,
  validInputs: Array<IOV2>,
  validOutputs: Array<IOV2>,
  nonce: Bytes,
  evaluable: Evaluable
): AddOrderV3 {
  let mockEvent = newMockEvent();
  let addOrderEvent = new AddOrderV3(
    mockEvent.address,
    mockEvent.logIndex,
    mockEvent.transactionLogIndex,
    mockEvent.logType,
    mockEvent.block,
    mockEvent.transaction,
    mockEvent.parameters,
    null
  );
  addOrderEvent.parameters = new Array();
  addOrderEvent.parameters.push(
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(sender))
  );
  addOrderEvent.parameters.push(
    new ethereum.EventParam(
      "orderHash",
      ethereum.Value.fromFixedBytes(orderHash)
    )
  );
  let order = createOrder(sender, evaluable, validInputs, validOutputs, nonce);
  addOrderEvent.parameters.push(
    new ethereum.EventParam("order", ethereum.Value.fromTuple(order))
  );

  return addOrderEvent;
}

// event RemoveOrderV2(address sender, bytes32 orderHash, OrderV3 order);
export function createRemoveOrderEvent(
  sender: Address,
  orderHash: Bytes,
  owner: Address,
  validInputs: Array<IOV2>,
  validOutputs: Array<IOV2>,
  nonce: Bytes,
  evaluable: Evaluable
): RemoveOrderV3 {
  let mockEvent = newMockEvent();
  let removeOrderEvent = new RemoveOrderV3(
    mockEvent.address,
    mockEvent.logIndex,
    mockEvent.transactionLogIndex,
    mockEvent.logType,
    mockEvent.block,
    mockEvent.transaction,
    mockEvent.parameters,
    null
  );
  removeOrderEvent.parameters = new Array();
  removeOrderEvent.parameters.push(
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(sender))
  );
  removeOrderEvent.parameters.push(
    new ethereum.EventParam(
      "orderHash",
      ethereum.Value.fromFixedBytes(orderHash)
    )
  );
  let order = createOrder(owner, evaluable, validInputs, validOutputs, nonce);
  removeOrderEvent.parameters.push(
    new ethereum.EventParam("order", ethereum.Value.fromTuple(order))
  );
  return removeOrderEvent;
}

// event TakeOrderV3(address sender, TakeOrderConfigV4 config, Float input, Float output);
export function createTakeOrderEvent(
  sender: Address,
  owner: Address,
  validInputs: Array<IOV2>,
  validOutputs: Array<IOV2>,
  nonce: Bytes,
  evaluable: Evaluable,
  input: Float,
  output: Float
): TakeOrderV3 {
  let mockEvent = newMockEvent();
  let takeOrderEvent = new TakeOrderV3(
    mockEvent.address,
    mockEvent.logIndex,
    mockEvent.transactionLogIndex,
    mockEvent.logType,
    mockEvent.block,
    mockEvent.transaction,
    mockEvent.parameters,
    null
  );
  takeOrderEvent.parameters = new Array();

  takeOrderEvent.parameters.push(
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(sender))
  );

  let config = new ethereum.Tuple();

  let signedContext = new ethereum.Tuple();
  signedContext.push(ethereum.Value.fromAddress(sender));
  signedContext.push(ethereum.Value.fromUnsignedBigIntArray([]));
  signedContext.push(ethereum.Value.fromBytes(Bytes.fromHexString("0x")));

  let signedContextArray = new Array<ethereum.Tuple>();
  signedContextArray.push(signedContext);

  let order = createOrder(owner, evaluable, validInputs, validOutputs, nonce);

  config.push(ethereum.Value.fromTuple(order));
  config.push(ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(0)));
  config.push(ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(0)));
  config.push(ethereum.Value.fromTupleArray(signedContextArray));

  takeOrderEvent.parameters.push(
    new ethereum.EventParam("config", ethereum.Value.fromTuple(config))
  );
  takeOrderEvent.parameters.push(
    new ethereum.EventParam("input", ethereum.Value.fromBytes(input))
  );
  takeOrderEvent.parameters.push(
    new ethereum.EventParam("output", ethereum.Value.fromBytes(output))
  );
  return takeOrderEvent;
}

// event MetaV1(address sender, uint256 subject, bytes meta);
export function createMetaEvent(
  sender: Address,
  subject: Bytes,
  meta: Bytes
): MetaV1_2 {
  let mockEvent = newMockEvent();
  let metaEvent = new MetaV1_2(
    mockEvent.address,
    mockEvent.logIndex,
    mockEvent.transactionLogIndex,
    mockEvent.logType,
    mockEvent.block,
    mockEvent.transaction,
    mockEvent.parameters,
    null
  );
  metaEvent.parameters = new Array();
  metaEvent.parameters.push(
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(sender))
  );
  metaEvent.parameters.push(
    new ethereum.EventParam("subject", ethereum.Value.fromBytes(subject))
  );
  metaEvent.parameters.push(
    new ethereum.EventParam("meta", ethereum.Value.fromBytes(meta))
  );
  return metaEvent;
}

// event ClearV3(address sender, OrderV4 alice, OrderV4 bob, ClearConfigV2 clearConfig);
export function createClearEvent(
  sender: Address,
  aliceOrder: ethereum.Tuple,
  bobOrder: ethereum.Tuple,
  aliceInputIOIndex: BigInt,
  aliceOutputIOIndex: BigInt,
  bobInputIOIndex: BigInt,
  bobOutputIOIndex: BigInt,
  aliceBountyVaultId: Bytes,
  bobBountyVaultId: Bytes
): ClearV3 {
  let mockEvent = newMockEvent();
  let clearEvent = new ClearV3(
    mockEvent.address,
    mockEvent.logIndex,
    mockEvent.transactionLogIndex,
    mockEvent.logType,
    mockEvent.block,
    mockEvent.transaction,
    mockEvent.parameters,
    null
  );

  clearEvent.parameters = new Array();
  clearEvent.parameters.push(
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(sender))
  );
  clearEvent.parameters.push(
    new ethereum.EventParam("alice", ethereum.Value.fromTuple(aliceOrder))
  );
  clearEvent.parameters.push(
    new ethereum.EventParam("bob", ethereum.Value.fromTuple(bobOrder))
  );

  let _clearConfig = new ethereum.Tuple();
  _clearConfig.push(ethereum.Value.fromUnsignedBigInt(aliceInputIOIndex));
  _clearConfig.push(ethereum.Value.fromUnsignedBigInt(aliceOutputIOIndex));
  _clearConfig.push(ethereum.Value.fromUnsignedBigInt(bobInputIOIndex));
  _clearConfig.push(ethereum.Value.fromUnsignedBigInt(bobOutputIOIndex));
  _clearConfig.push(ethereum.Value.fromFixedBytes(aliceBountyVaultId));
  _clearConfig.push(ethereum.Value.fromFixedBytes(bobBountyVaultId));
  clearEvent.parameters.push(
    new ethereum.EventParam(
      "clearConfig",
      ethereum.Value.fromTuple(_clearConfig)
    )
  );

  return clearEvent;
}

// event AfterClearV2(address sender, ClearStateChangeV2 clearStateChange)
export function createAfterClearEvent(
  sender: Address,
  aliceOutput: Float,
  bobOutput: Float,
  aliceInput: Float,
  bobInput: Float
): AfterClearV2 {
  let mockEvent = newMockEvent();
  let afterClearEvent = new AfterClearV2(
    mockEvent.address,
    mockEvent.logIndex,
    mockEvent.transactionLogIndex,
    mockEvent.logType,
    mockEvent.block,
    mockEvent.transaction,
    mockEvent.parameters,
    null
  );
  afterClearEvent.parameters = new Array();
  afterClearEvent.parameters.push(
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(sender))
  );
  let _clearStateChange = new ethereum.Tuple();
  _clearStateChange.push(ethereum.Value.fromBytes(aliceOutput));
  _clearStateChange.push(ethereum.Value.fromBytes(bobOutput));
  _clearStateChange.push(ethereum.Value.fromBytes(aliceInput));
  _clearStateChange.push(ethereum.Value.fromBytes(bobInput));
  afterClearEvent.parameters.push(
    new ethereum.EventParam(
      "clearStateChange",
      ethereum.Value.fromTuple(_clearStateChange)
    )
  );
  return afterClearEvent;
}
