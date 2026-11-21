import {
  test,
  clearStore,
  describe,
  afterEach,
  clearInBlockStore,
  newMockEvent,
  assert,
  beforeEach,
} from "matchstick-as";
import { BigInt, Address, Bytes, crypto } from "@graphprotocol/graph-ts";
import {
  Evaluable,
  IOV2,
  createAfterClearEvent,
  createTakeOrderEvent,
} from "./event-mocks.test";
import { eventId } from "../src/interfaces/event";
import {
  createTradeVaultBalanceChangeEntity,
  tradeVaultBalanceChangeId,
} from "../src/tradevaultbalancechange";
import { vaultEntityId } from "../src/vault";
import { orderHashFromTakeOrderEvent } from "../src/takeorder";
import { makeTradeId } from "../src/trade";
import { createMockERC20Functions } from "./erc20.test";
import { makeClearBountyId } from "../src/clear";
import {
  createMockDecimalFloatFunctions,
  FLOAT_1,
  FLOAT_10,
  FLOAT_11,
  FLOAT_15,
  FLOAT_20,
} from "./float.test";

describe("Deposits", () => {
  beforeEach(createMockDecimalFloatFunctions);

  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("tradeVaultBalanceChangeId()", () => {
    let event = newMockEvent();
    event.logIndex = BigInt.fromI32(2);
    event.transaction.hash = Bytes.fromHexString(
      "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
    );
    let vaultEntityId = Bytes.fromHexString(
      "0x1234567890abcdef1234567890abcdef12345678"
    );

    let tradeVaultBalanceChangeId = Bytes.fromByteArray(
      crypto.keccak256(eventId(event).concat(vaultEntityId))
    );

    assert.bytesEquals(
      tradeVaultBalanceChangeId,
      Bytes.fromHexString(
        "0x417b9a4b4f93565a22e3d13b93f7f467b0e84ef8dddcb52a718298e4e17df26f"
      )
    );
  });

  test("createTradeVaultBalanceChangeEntity()", () => {
    createMockERC20Functions(
      Address.fromString("0x3333333333333333333333333333333333333333")
    );
    createMockERC20Functions(
      Address.fromString("0x4444444444444444444444444444444444444444")
    );

    let owner = Address.fromString(
      "0x1111111111111111111111111111111111111111"
    );

    let event = createTakeOrderEvent(
      owner,
      Address.fromString("0x2222222222222222222222222222222222222222"),
      [
        new IOV2(
          Address.fromString("0x3333333333333333333333333333333333333333"),
          Bytes.fromHexString(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          )
        ),
      ],
      [
        new IOV2(
          Address.fromString("0x4444444444444444444444444444444444444444"),
          Bytes.fromHexString(
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
          )
        ),
      ],
      Bytes.fromHexString("0x5555555555555555555555555555555555555555"),
      new Evaluable(
        Address.fromString("0x6666666666666666666666666666666666666666"),
        Address.fromString("0x7777777777777777777777777777777777777777"),
        Bytes.fromHexString("0x8888888888888888888888888888888888888888")
      ),
      Bytes.fromHexString(
        "0x0000000000000000000000000000000000000000000000000000000000000001"
      ),
      Bytes.fromHexString(
        "0x0000000000000000000000000000000000000000000000000000000000000001"
      )
    );

    let oldVaultBalance = Bytes.fromHexString(
      "0x000000000000000000000000000000000000000000000000000000000000000a"
    );

    let _vaultEntityId = vaultEntityId(
      event.address,
      owner,
      Bytes.fromHexString(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
      ),
      Address.fromString("0x3333333333333333333333333333333333333333")
    );

    let orderHash = orderHashFromTakeOrderEvent(event);

    createTradeVaultBalanceChangeEntity(
      event,
      orderHash,
      _vaultEntityId,
      oldVaultBalance,
      Bytes.fromHexString(
        "0x0000000000000000000000000000000000000000000000000000000000000001"
      )
    );

    assert.entityCount("TradeVaultBalanceChange", 1);

    let id = tradeVaultBalanceChangeId(event, _vaultEntityId).toHexString();

    assert.fieldEquals(
      "TradeVaultBalanceChange",
      id,
      "amount",
      FLOAT_1.toHexString()
    );
    assert.fieldEquals(
      "TradeVaultBalanceChange",
      id,
      "oldVaultBalance",
      FLOAT_10.toHexString()
    );
    assert.fieldEquals(
      "TradeVaultBalanceChange",
      id,
      "newVaultBalance",
      FLOAT_11.toHexString()
    );
    assert.fieldEquals(
      "TradeVaultBalanceChange",
      id,
      "vault",
      _vaultEntityId.toHexString()
    );
    assert.fieldEquals(
      "TradeVaultBalanceChange",
      id,
      "trade",
      makeTradeId(event, orderHash).toHexString()
    );
    assert.fieldEquals(
      "TradeVaultBalanceChange",
      id,
      "timestamp",
      event.block.timestamp.toString()
    );
    assert.fieldEquals(
      "TradeVaultBalanceChange",
      id,
      "transaction",
      event.transaction.hash.toHex()
    );
  });

  test("TradeVaultBalanceChangeEntity id should not be equal to ClearBounty id", () => {
    const alice = Address.fromString(
      "0x850c40aBf6e325231ba2DeD1356d1f2c267e63Ce"
    );
    let aliceOutputAmount = FLOAT_10;
    let bobOutputAmount = FLOAT_20;
    let aliceInputAmount = FLOAT_15;
    let bobInputAmount = FLOAT_10;
    let vaultEntityId = Bytes.fromHexString(
      "0x1234567890abcdef1234567890abcdef12345678"
    );

    let event = createAfterClearEvent(
      alice,
      aliceOutputAmount,
      bobOutputAmount,
      aliceInputAmount,
      bobInputAmount
    );
    const id1 = tradeVaultBalanceChangeId(event, vaultEntityId);
    const id2 = makeClearBountyId(event, vaultEntityId);

    assert.assertTrue(id1 !== id2);
  });
});
