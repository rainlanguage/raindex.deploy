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
  Deposit,
  DepositV2,
} from "../generated/RaindexDeposits/RaindexDeposits";
import { handleDeposit, handleDepositV2 } from "../src/tokens";

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
});
