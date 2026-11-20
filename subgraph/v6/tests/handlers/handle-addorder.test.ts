import {
  test,
  clearStore,
  describe,
  afterEach,
  clearInBlockStore,
  assert,
} from "matchstick-as";
import { Bytes, BigInt, Address } from "@graphprotocol/graph-ts";
import { Evaluable, IOV2, createAddOrderEvent } from "../event-mocks.test";
import { makeOrderId } from "../../src/order";
import { vaultEntityId } from "../../src/vault";
import { createMockERC20Functions } from "../erc20.test";
import { handleAddOrder } from "../../src/handlers";
import { FLOAT_0 } from "../float.test";

describe("Add and remove orders", () => {
  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("handleAddOrder()", () => {
    const inputToken = "0x1234567890123456789012345678901234567890";
    createMockERC20Functions(Address.fromString(inputToken));

    const outputToken = "0x0987654321098765432109876543210987654321";
    createMockERC20Functions(Address.fromString(outputToken));

    let owner = Address.fromString(
      "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    );

    const inputVaultId =
      "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    const outputVaultId =
      "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    let input = new IOV2(
      Address.fromString(inputToken),
      Bytes.fromHexString(inputVaultId)
    );
    let output = new IOV2(
      Address.fromString(outputToken),
      Bytes.fromHexString(outputVaultId)
    );

    let event = createAddOrderEvent(
      owner,
      Address.fromString("0x0987654321098765432109876543210987654321"),
      [input],
      [output],
      Bytes.fromHexString("0x1234567890123456789012345678901234567890"),
      new Evaluable(
        Address.fromString("0x1234567890123456789012345678901234567890"),
        Address.fromString("0x0987654321098765432109876543210987654321"),
        Bytes.fromHexString("0x1234567890123456789012345678901234567890")
      )
    );

    handleAddOrder(event);

    // we should have an raindex entity
    assert.entityCount("Raindex", 1);
    assert.fieldEquals(
      "Raindex",
      event.address.toHexString(),
      "id",
      event.address.toHexString()
    );

    let id = makeOrderId(
      event.address,
      Bytes.fromHexString("0x0987654321098765432109876543210987654321")
    );

    assert.entityCount("Order", 1);
    assert.fieldEquals("Order", id.toHexString(), "active", "true");
    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "orderHash",
      "0x0987654321098765432109876543210987654321"
    );
    assert.fieldEquals("Order", id.toHexString(), "owner", owner.toHexString());
    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "nonce",
      "0x1234567890123456789012345678901234567890"
    );
    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "orderBytes",
      "0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee00000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000140000000000000000000000000000000000000000000000000000000000000020012345678901234567890123456789012345678900000000000000000000000000000000000000000000000001234567890123456789012345678901234567890000000000000000000000000098765432109876543210987654321098765432100000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000014123456789012345678901234567890123456789000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000020000000000000000000000000123456789012345678901234567890123456789000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000020000000000000000000000000098765432109876543210987654321098765432100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    assert.fieldEquals(
      "Order",
      id.toHexString(),
      "timestampAdded",
      event.block.timestamp.toString()
    );

    // we should also have two new empty vaults
    assert.entityCount("Vault", 2);

    let inputVaultEId = vaultEntityId(
      event.address,
      owner,
      input.vaultId,
      input.token
    );
    assert.fieldEquals(
      "Vault",
      inputVaultEId.toHexString(),
      "balance",
      FLOAT_0.toHexString()
    );

    let outputVaultEId = vaultEntityId(
      event.address,
      owner,
      output.vaultId,
      output.token
    );
    assert.fieldEquals(
      "Vault",
      outputVaultEId.toHexString(),
      "balance",
      FLOAT_0.toHexString()
    );
  });
});
