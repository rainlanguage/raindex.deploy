import {
  test,
  assert,
  clearStore,
  describe,
  afterEach,
  newMockEvent,
} from "matchstick-as";
import { Address, BigInt, Bytes, ethereum } from "@graphprotocol/graph-ts";
import {
  AddOrder,
  AddOrderV2,
  AddOrderV3,
  Deposit,
  DepositV2,
} from "../generated/RaindexTokenEvents/RaindexTokenEvents";
import {
  handleAddOrder,
  handleAddOrderV2,
  handleAddOrderV3,
  handleDeposit,
  handleDepositV2,
} from "../src/tokens";

const RAINDEX_A = Address.fromString("0x000000000000000000000000000000000000aaaa");
const RAINDEX_B = Address.fromString("0x000000000000000000000000000000000000bbbb");
const TOKEN_X = Address.fromString("0x0000000000000000000000000000000000001111");
const TOKEN_Y = Address.fromString("0x0000000000000000000000000000000000002222");
const SENDER = Address.fromString("0x0000000000000000000000000000000000009999");

function depositV1(raindex: Address, token: Address, block: i32): Deposit {
  const event = changetype<Deposit>(newMockEvent());
  event.address = raindex;
  event.block.number = BigInt.fromI32(block);
  event.parameters = [
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(SENDER)),
    new ethereum.EventParam("token", ethereum.Value.fromAddress(token)),
    new ethereum.EventParam("vaultId", ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(1))),
    new ethereum.EventParam("amount", ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(5))),
  ];
  return event;
}

function depositV2(raindex: Address, token: Address, block: i32): DepositV2 {
  const event = changetype<DepositV2>(newMockEvent());
  event.address = raindex;
  event.block.number = BigInt.fromI32(block);
  event.parameters = [
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(SENDER)),
    new ethereum.EventParam("token", ethereum.Value.fromAddress(token)),
    new ethereum.EventParam("vaultId", ethereum.Value.fromFixedBytes(Bytes.fromHexString("0x" + "01".repeat(32)))),
    new ethereum.EventParam("depositAmountUint256", ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(5))),
  ];
  return event;
}

const TOKEN_Z = Address.fromString("0x0000000000000000000000000000000000003333");
const ZERO = Address.fromString("0x0000000000000000000000000000000000000000");
const HASH = Bytes.fromHexString("0x" + "02".repeat(32));

function tuple(values: Array<ethereum.Value>): ethereum.Value {
  return ethereum.Value.fromTuple(changetype<ethereum.Tuple>(values));
}

// `IO` (interface versions 3 and 4): token, decimals, vaultId.
function io(token: Address): ethereum.Value {
  return tuple([
    ethereum.Value.fromAddress(token),
    ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(18)),
    ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(1)),
  ]);
}

// `IOV2` (interface versions 5 to 7): token, vaultId.
function ioV2(token: Address): ethereum.Value {
  return tuple([ethereum.Value.fromAddress(token), ethereum.Value.fromFixedBytes(HASH)]);
}

function ios(tokens: Array<Address>, v2: bool): ethereum.Value {
  const out = new Array<ethereum.Value>();
  for (let i = 0; i < tokens.length; i++) {
    out.push(v2 ? ioV2(tokens[i]) : io(tokens[i]));
  }
  return ethereum.Value.fromArray(out);
}

// `(address,address,bytes)` evaluable of interface versions 4 to 7.
function evaluable(): ethereum.Value {
  return tuple([
    ethereum.Value.fromAddress(ZERO),
    ethereum.Value.fromAddress(ZERO),
    ethereum.Value.fromBytes(Bytes.fromHexString("0x")),
  ]);
}

function addOrder(raindex: Address, inputs: Array<Address>, outputs: Array<Address>, block: i32): AddOrder {
  const event = changetype<AddOrder>(newMockEvent());
  event.address = raindex;
  event.block.number = BigInt.fromI32(block);
  const order = tuple([
    ethereum.Value.fromAddress(SENDER),
    ethereum.Value.fromBoolean(false),
    tuple([ethereum.Value.fromAddress(ZERO), ethereum.Value.fromAddress(ZERO), ethereum.Value.fromAddress(ZERO)]),
    ios(inputs, false),
    ios(outputs, false),
  ]);
  event.parameters = [
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(SENDER)),
    new ethereum.EventParam("expressionDeployer", ethereum.Value.fromAddress(ZERO)),
    new ethereum.EventParam("order", order),
    new ethereum.EventParam("orderHash", ethereum.Value.fromFixedBytes(HASH)),
  ];
  return event;
}

function addOrderV2(raindex: Address, inputs: Array<Address>, outputs: Array<Address>, block: i32): AddOrderV2 {
  const event = changetype<AddOrderV2>(newMockEvent());
  event.address = raindex;
  event.block.number = BigInt.fromI32(block);
  const order = tuple([
    ethereum.Value.fromAddress(SENDER),
    evaluable(),
    ios(inputs, false),
    ios(outputs, false),
    ethereum.Value.fromFixedBytes(HASH),
  ]);
  event.parameters = [
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(SENDER)),
    new ethereum.EventParam("orderHash", ethereum.Value.fromFixedBytes(HASH)),
    new ethereum.EventParam("order", order),
  ];
  return event;
}

function addOrderV3(raindex: Address, inputs: Array<Address>, outputs: Array<Address>, block: i32): AddOrderV3 {
  const event = changetype<AddOrderV3>(newMockEvent());
  event.address = raindex;
  event.block.number = BigInt.fromI32(block);
  const order = tuple([
    ethereum.Value.fromAddress(SENDER),
    evaluable(),
    ios(inputs, true),
    ios(outputs, true),
    ethereum.Value.fromFixedBytes(HASH),
  ]);
  event.parameters = [
    new ethereum.EventParam("sender", ethereum.Value.fromAddress(SENDER)),
    new ethereum.EventParam("orderHash", ethereum.Value.fromFixedBytes(HASH)),
    new ethereum.EventParam("order", order),
  ];
  return event;
}

function id(raindex: Address, token: Address): string {
  return raindex.concat(token).toHexString();
}

describe("RaindexToken", () => {
  afterEach(() => {
    clearStore();
  });

  test("a Deposit records its raindex, token and block", () => {
    handleDeposit(depositV1(RAINDEX_A, TOKEN_X, 10));
    assert.entityCount("RaindexToken", 1);
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_X), "raindex", RAINDEX_A.toHexString());
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_X), "token", TOKEN_X.toHexString());
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_X), "firstSeenBlock", "10");
  });

  test("a DepositV2 records its raindex, token and block", () => {
    handleDepositV2(depositV2(RAINDEX_B, TOKEN_Y, 20));
    assert.entityCount("RaindexToken", 1);
    assert.fieldEquals("RaindexToken", id(RAINDEX_B, TOKEN_Y), "raindex", RAINDEX_B.toHexString());
    assert.fieldEquals("RaindexToken", id(RAINDEX_B, TOKEN_Y), "token", TOKEN_Y.toHexString());
    assert.fieldEquals("RaindexToken", id(RAINDEX_B, TOKEN_Y), "firstSeenBlock", "20");
  });

  test("a later deposit of the same token keeps the first block", () => {
    handleDepositV2(depositV2(RAINDEX_A, TOKEN_X, 20));
    handleDepositV2(depositV2(RAINDEX_A, TOKEN_X, 30));
    assert.entityCount("RaindexToken", 1);
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_X), "firstSeenBlock", "20");
  });

  test("the same token in two raindexes is two rows; two tokens in one raindex is two rows", () => {
    handleDepositV2(depositV2(RAINDEX_A, TOKEN_X, 1));
    handleDepositV2(depositV2(RAINDEX_B, TOKEN_X, 2));
    handleDeposit(depositV1(RAINDEX_A, TOKEN_Y, 3));
    assert.entityCount("RaindexToken", 3);
    assert.fieldEquals("RaindexToken", id(RAINDEX_B, TOKEN_X), "firstSeenBlock", "2");
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_Y), "firstSeenBlock", "3");
  });

  // Each version: every input and every output is recorded, so a token that
  // only arrives by trade (an input never deposited) is discovered.
  test("an AddOrder records every input and output token", () => {
    handleAddOrder(addOrder(RAINDEX_A, [TOKEN_X, TOKEN_Y], [TOKEN_Z], 40));
    assert.entityCount("RaindexToken", 3);
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_X), "firstSeenBlock", "40");
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_Y), "firstSeenBlock", "40");
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_Z), "token", TOKEN_Z.toHexString());
  });

  test("an AddOrderV2 records every input and output token", () => {
    handleAddOrderV2(addOrderV2(RAINDEX_B, [TOKEN_X], [TOKEN_Y, TOKEN_Z], 50));
    assert.entityCount("RaindexToken", 3);
    assert.fieldEquals("RaindexToken", id(RAINDEX_B, TOKEN_X), "firstSeenBlock", "50");
    assert.fieldEquals("RaindexToken", id(RAINDEX_B, TOKEN_Y), "firstSeenBlock", "50");
    assert.fieldEquals("RaindexToken", id(RAINDEX_B, TOKEN_Z), "raindex", RAINDEX_B.toHexString());
  });

  test("an AddOrderV3 records every input and output token", () => {
    handleAddOrderV3(addOrderV3(RAINDEX_A, [TOKEN_X, TOKEN_Z], [TOKEN_Y], 60));
    assert.entityCount("RaindexToken", 3);
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_X), "firstSeenBlock", "60");
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_Z), "firstSeenBlock", "60");
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_Y), "firstSeenBlock", "60");
  });

  test("a token that is both an input and an output is one row", () => {
    handleAddOrderV3(addOrderV3(RAINDEX_A, [TOKEN_X], [TOKEN_X], 70));
    assert.entityCount("RaindexToken", 1);
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_X), "firstSeenBlock", "70");
  });

  test("an order before the first deposit sets the first block; the deposit adds no row", () => {
    handleAddOrderV3(addOrderV3(RAINDEX_A, [TOKEN_Y], [TOKEN_X], 80));
    handleDepositV2(depositV2(RAINDEX_A, TOKEN_X, 90));
    assert.entityCount("RaindexToken", 2);
    assert.fieldEquals("RaindexToken", id(RAINDEX_A, TOKEN_X), "firstSeenBlock", "80");
  });
});
