import {
  test,
  clearStore,
  describe,
  afterEach,
  clearInBlockStore,
  assert,
  beforeEach,
} from "matchstick-as";
import { Bytes, BigInt, Address } from "@graphprotocol/graph-ts";
import { Evaluable, IOV2, createTakeOrderEvent } from "../event-mocks.test";
import { createMockERC20Functions } from "../erc20.test";
import { handleTakeOrder } from "../../src/handlers";
import { createMockDecimalFloatFunctions, FLOAT_1 } from "../float.test";

describe("Add and remove orders", () => {
  beforeEach(createMockDecimalFloatFunctions);

  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("handleTakeOrder()", () => {
    createMockERC20Functions(
      Address.fromString("0x3333333333333333333333333333333333333333")
    );
    createMockERC20Functions(
      Address.fromString("0x4444444444444444444444444444444444444444")
    );

    let event = createTakeOrderEvent(
      Address.fromString("0x1111111111111111111111111111111111111111"),
      Address.fromString("0x2222222222222222222222222222222222222222"),
      [
        new IOV2(
          Address.fromString("0x3333333333333333333333333333333333333333"),
          Bytes.fromHexString("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        ),
      ],
      [
        new IOV2(
          Address.fromString("0x4444444444444444444444444444444444444444"),
          Bytes.fromHexString("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        ),
      ],
      Bytes.fromHexString("0x5555555555555555555555555555555555555555"),
      new Evaluable(
        Address.fromString("0x6666666666666666666666666666666666666666"),
        Address.fromString("0x7777777777777777777777777777777777777777"),
        Bytes.fromHexString("0x8888888888888888888888888888888888888888")
      ),
      FLOAT_1,
      FLOAT_1
    );

    handleTakeOrder(event);

    // After this, we should have:
    // - 1 TakeOrder
    // - 2 Vaults
    // - 2 TradeVaultBalanceChanges
    // - 1 Trade

    assert.entityCount("TakeOrder", 1);
    assert.entityCount("Vault", 2);
    assert.entityCount("TradeVaultBalanceChange", 2);
    assert.entityCount("Trade", 1);

    // we should have an raindex entity
    assert.entityCount("Raindex", 1);
    assert.fieldEquals(
      "Raindex",
      event.address.toHexString(),
      "id",
      event.address.toHexString()
    );
  });
});
