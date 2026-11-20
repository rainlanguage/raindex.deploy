import {
  test,
  assert,
  clearStore,
  describe,
  afterEach,
  clearInBlockStore,
} from "matchstick-as";
import { BigInt, Address, Bytes, ethereum } from "@graphprotocol/graph-ts";
import { Evaluable, IOV2, createTakeOrderEvent } from "./event-mocks.test";
import { createTakeOrderEntity } from "../src/takeorder";
import { eventId } from "../src/interfaces/event";
import { createMockERC20Functions } from "./erc20.test";
import { FLOAT_2, FLOAT_3 } from "./float.test";

describe("Deposits", () => {
  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("createTakeOrderEvent()", () => {
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

    assert.addressEquals(
      event.params.sender,
      Address.fromString("0x1111111111111111111111111111111111111111")
    );
    assert.addressEquals(
      event.params.config.order.owner,
      Address.fromString("0x2222222222222222222222222222222222222222")
    );
    assert.bytesEquals(
      event.params.config.order.nonce,
      Bytes.fromHexString("0x5555555555555555555555555555555555555555")
    );
    assert.addressEquals(
      event.params.config.order.evaluable.interpreter,
      Address.fromString("0x6666666666666666666666666666666666666666")
    );
    assert.addressEquals(
      event.params.config.order.evaluable.store,
      Address.fromString("0x7777777777777777777777777777777777777777")
    );
    assert.bytesEquals(
      event.params.config.order.evaluable.bytecode,
      Bytes.fromHexString("0x8888888888888888888888888888888888888888")
    );
    let input = event.params.config.order.validInputs[0];
    assert.addressEquals(
      input.token,
      Address.fromString("0x3333333333333333333333333333333333333333")
    );
    assert.bytesEquals(
      input.vaultId,
      Bytes.fromHexString(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
      )
    );
    let output = event.params.config.order.validOutputs[0];
    assert.addressEquals(
      output.token,
      Address.fromString("0x4444444444444444444444444444444444444444")
    );

    assert.bytesEquals(
      output.vaultId,
      Bytes.fromHexString(
        "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
      )
    );
    assert.bytesEquals(
      event.params.input,
      Bytes.fromHexString(
        "0x0000000000000000000000000000000000000000000000000000000000000001"
      )
    );
    assert.bytesEquals(
      event.params.output,
      Bytes.fromHexString(
        "0x0000000000000000000000000000000000000000000000000000000000000001"
      )
    );
  });

  test("createTakeOrderEntity()", () => {
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
        "0x0000000000000000000000000000000000000000000000000000000000000002"
      ),
      Bytes.fromHexString(
        "0x0000000000000000000000000000000000000000000000000000000000000003"
      )
    );

    createTakeOrderEntity(event);

    assert.entityCount("TakeOrder", 1);

    let id = eventId(event).toHexString();

    assert.fieldEquals(
      "TakeOrder",
      id,
      "sender",
      "0x1111111111111111111111111111111111111111"
    );
    assert.fieldEquals("TakeOrder", id, "inputAmount", FLOAT_2.toHexString());
    assert.fieldEquals("TakeOrder", id, "outputAmount", FLOAT_3.toHexString());
    assert.fieldEquals(
      "TakeOrder",
      id,
      "takeOrderConfigBytes",
      ethereum.encode(event.parameters[1].value)!.toHexString()
    );
  });
});
