import {
  test,
  clearStore,
  describe,
  afterEach,
  clearInBlockStore,
  assert,
} from "matchstick-as";
import { Bytes, BigInt, Address } from "@graphprotocol/graph-ts";
import {
  Evaluable,
  IOV2,
  createAddOrderEvent,
  createMetaEvent,
} from "../event-mocks.test";
import { handleAddOrder, makeOrderId } from "../../src/order";
import { handleMeta } from "../../src/handlers";
import { createMockERC20Functions } from "../erc20.test";

describe("Add and remove orders", () => {
  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("handleMeta() should not error if there is no order", () => {
    let sender = Address.fromString(
      "0x1234567890abcdef1234567890abcdef12345678"
    );
    let subject = Bytes.fromHexString(
      "0x0987654321098765432109876543210987654321"
    );
    let meta = Bytes.fromHexString(
      "0x1234567890abcdef1234567890abcdef12345678"
    );

    let event = createMetaEvent(sender, subject, meta);

    handleMeta(event);
  });

  test("handleMeta() should update the meta field of an order", () => {
    createMockERC20Functions(
      Address.fromString("0x1234567890123456789012345678901234567890")
    );

    let orderHash = Bytes.fromHexString(
      "0x0987654321098765432109876543210987654321"
    );
    // first we need to create an order
    let event = createAddOrderEvent(
      Address.fromString("0x1234567890123456789012345678901234567890"),
      orderHash,
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        ),
      ],
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        ),
      ],
      Bytes.fromHexString("0x1234567890123456789012345678901234567890"),
      new Evaluable(
        Address.fromString("0x1234567890123456789012345678901234567890"),
        Address.fromString("0x0987654321098765432109876543210987654321"),
        Bytes.fromHexString("0x1234567890123456789012345678901234567890")
      )
    );

    handleAddOrder(event);

    let metaEvent = createMetaEvent(
      // sender
      Address.fromBytes(
        Address.fromHexString("0x1234567890abcdef1234567890abcdef12345678")
      ),
      // subject
      orderHash,
      // meta
      Bytes.fromHexString("0x1234567890abcdef1234567890abcdef12345678")
    );

    handleMeta(metaEvent);

    // we should have an raindex entity
    assert.entityCount("Raindex", 1);
    assert.fieldEquals(
      "Raindex",
      event.address.toHexString(),
      "id",
      event.address.toHexString()
    );

    let id = makeOrderId(metaEvent.address, orderHash);

    // meta field on order should be updated
    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "meta",
      "0x1234567890abcdef1234567890abcdef12345678"
    );
  });
});
