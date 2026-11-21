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
  createRemoveOrderEvent,
} from "../event-mocks.test";
import { makeOrderId } from "../../src/order";
import { createMockERC20Functions } from "../erc20.test";
import { handleAddOrder, handleRemoveOrder } from "../../src/handlers";

describe("Add and remove orders", () => {
  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("handleRemoveOrder()", () => {
    createMockERC20Functions(
      Address.fromString("0x1234567890123456789012345678901234567890")
    );
    createMockERC20Functions(
      Address.fromString("0x1234567890123456789012345678901234567890")
    );

    // First we need to add an order
    let event = createAddOrderEvent(
      Address.fromString("0x1234567890123456789012345678901234567890"),
      Address.fromString("0x0987654321098765432109876543210987654321"),
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

    event.address = Address.fromString(
      "0x1234567890123456789012345678901234567890"
    );

    handleAddOrder(event);

    // Now we can remove the order
    let removeEvent = createRemoveOrderEvent(
      Address.fromString("0x1234567890123456789012345678901234567890"),
      Bytes.fromHexString("0x0987654321098765432109876543210987654321"),
      Address.fromString("0x1234567890123456789012345678901234567890"),
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        ),
      ],
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString(
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
          )
        ),
      ],
      Bytes.fromHexString("0x1234567890123456789012345678901234567890"),
      new Evaluable(
        Address.fromString("0x1234567890123456789012345678901234567890"),
        Address.fromString("0x0987654321098765432109876543210987654321"),
        Bytes.fromHexString("0x1234567890123456789012345678901234567890")
      )
    );

    removeEvent.address = Address.fromString(
      "0x1234567890123456789012345678901234567890"
    );

    handleRemoveOrder(removeEvent);

    // we should have an raindex entity
    assert.entityCount("Raindex", 1);
    assert.fieldEquals(
      "Raindex",
      event.address.toHexString(),
      "id",
      event.address.toHexString()
    );

    assert.entityCount("Order", 1);

    let id = makeOrderId(
      removeEvent.address,
      Bytes.fromHexString("0x0987654321098765432109876543210987654321")
    );

    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "active",
      "false",
      "Order should be inactive after removeOrder event"
    );

    // if we add the order again, it should be active
    handleAddOrder(event);

    assert.entityCount("Order", 1);

    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "active",
      "true",
      "Order should be active after second addOrder event"
    );

    // if we remove the order again, it should be inactive

    handleRemoveOrder(removeEvent);

    assert.entityCount("Order", 1);

    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "active",
      "false",
      "Order should be inactive after second removeOrder event"
    );
  });
});
