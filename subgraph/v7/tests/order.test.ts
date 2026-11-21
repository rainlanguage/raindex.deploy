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
} from "./event-mocks.test";
import {
  createAddOrderEntity,
  createOrderEntity,
  createRemoveOrderEntity,
  makeOrderId,
} from "../src/order";
import { eventId } from "../src/interfaces/event";
import { Order } from "../generated/schema";
import { createMockERC20Functions } from "./erc20.test";

describe("Add and remove orders", () => {
  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("createAddOrderEvent()", () => {
    let event = createAddOrderEvent(
      Address.fromString("0x1234567890123456789012345678901234567890"),
      Address.fromString("0x0987654321098765432109876543210987654321"),
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          )
        ),
      ],
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString(
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
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

    assert.addressEquals(
      event.params.sender,
      Address.fromString("0x1234567890123456789012345678901234567890")
    );
    assert.bytesEquals(
      event.params.orderHash,
      Bytes.fromHexString("0x0987654321098765432109876543210987654321")
    );
    assert.addressEquals(
      Address.fromString("0x1234567890123456789012345678901234567890"),
      event.params.order.owner
    );
    assert.bytesEquals(
      event.params.order.nonce,
      Bytes.fromHexString("0x1234567890123456789012345678901234567890")
    );
    assert.addressEquals(
      event.params.order.evaluable.interpreter,
      Address.fromString("0x1234567890123456789012345678901234567890")
    );
    assert.addressEquals(
      event.params.order.evaluable.store,
      Address.fromString("0x0987654321098765432109876543210987654321")
    );
    assert.bytesEquals(
      event.params.order.evaluable.bytecode,
      Bytes.fromHexString("0x1234567890123456789012345678901234567890")
    );

    let input = event.params.order.validInputs[0];
    assert.addressEquals(
      input.token,
      Address.fromString("0x1234567890123456789012345678901234567890")
    );
    assert.bytesEquals(
      input.vaultId,
      Bytes.fromHexString(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
      )
    );

    let output = event.params.order.validOutputs[0];
    assert.addressEquals(
      output.token,
      Address.fromString("0x1234567890123456789012345678901234567890")
    );
    assert.bytesEquals(
      output.vaultId,
      Bytes.fromHexString(
        "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
      )
    );
  });

  test("createOrderEntity()", () => {
    createMockERC20Functions(
      Address.fromString("0x1234567890123456789012345678901234567890")
    );
    createMockERC20Functions(
      Address.fromString("0x1234567890123456789012345678901234567890")
    );

    let event = createAddOrderEvent(
      Address.fromString("0x1234567890123456789012345678901234567890"),
      Address.fromString("0x0987654321098765432109876543210987654321"),
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          )
        ),
      ],
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString(
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
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

    let id = makeOrderId(
      event.address,
      Bytes.fromHexString("0x0987654321098765432109876543210987654321")
    );

    createOrderEntity(event);

    assert.entityCount("Order", 1);
    assert.fieldEquals("Order", id.toHexString(), "active", "true");
    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "orderHash",
      "0x0987654321098765432109876543210987654321"
    );
    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "owner",
      "0x1234567890123456789012345678901234567890"
    );
    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "nonce",
      "0x1234567890123456789012345678901234567890"
    );
    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "timestampAdded",
      event.block.timestamp.toString()
    );

    let order = Order.load(id)!;
    let inputs = order.inputs;
    assert.i32Equals(inputs.length, 1, "inputs length");
    let outputs = order.outputs;
    assert.i32Equals(outputs.length, 1, "outputs length");
  });

  test("createAddOrderEntity()", () => {
    let event = createAddOrderEvent(
      Address.fromString("0x1234567890123456789012345678901234567890"),
      Address.fromString("0x0987654321098765432109876543210987654321"),
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          )
        ),
      ],
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString(
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
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

    createAddOrderEntity(event);

    assert.entityCount("AddOrder", 1);

    let id = eventId(event);

    let orderid = makeOrderId(event.address, event.params.orderHash);

    assert.fieldEquals(
      "AddOrder",
      id.toHexString(),
      "order",
      orderid.toHexString()
    );

    assert.fieldEquals(
      "AddOrder",
      id.toHexString(),
      "sender",
      "0x1234567890123456789012345678901234567890"
    );

    assert.fieldEquals(
      "AddOrder",
      id.toHexString(),
      "transaction",
      event.transaction.hash.toHex()
    );
  });

  test("createRemoveOrderEntity()", () => {
    let event = createRemoveOrderEvent(
      Address.fromString("0x1234567890123456789012345678901234567890"),
      Bytes.fromHexString("0x0987654321098765432109876543210987654321"),
      Address.fromString("0x1234567890123456789012345678901234567890"),
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          )
        ),
      ],
      [
        new IOV2(
          Address.fromString("0x1234567890123456789012345678901234567890"),
          Bytes.fromHexString(
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
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

    createRemoveOrderEntity(event);

    assert.entityCount("RemoveOrder", 1);

    let id = eventId(event);

    let orderid = makeOrderId(event.address, event.params.orderHash);

    assert.fieldEquals(
      "RemoveOrder",
      id.toHexString(),
      "order",
      orderid.toHexString()
    );

    assert.fieldEquals(
      "RemoveOrder",
      id.toHexString(),
      "sender",
      "0x1234567890123456789012345678901234567890"
    );

    assert.fieldEquals(
      "RemoveOrder",
      id.toHexString(),
      "transaction",
      event.transaction.hash.toHex()
    );
  });
});
