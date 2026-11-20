import {
  test,
  assert,
  clearStore,
  describe,
  afterEach,
  clearInBlockStore,
  beforeEach,
} from "matchstick-as";
import { BigInt, Address, Bytes } from "@graphprotocol/graph-ts";
import { createDepositEvent } from "../event-mocks.test";
import { handleDeposit } from "../../src/handlers";
import { vaultEntityId } from "../../src/vault";
import { Deposit, Vault } from "../../generated/schema";
import { eventId } from "../../src/interfaces/event";
import { createMockERC20Functions } from "../erc20.test";
import {
  createMockDecimalFloatFunctions,
  FLOAT_100,
  FLOAT_200,
  FLOAT_300,
  FLOAT_0,
} from "../float.test";

describe("Handle deposit", () => {
  beforeEach(createMockDecimalFloatFunctions);

  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("handleDeposit()", () => {
    createMockERC20Functions(
      Address.fromString("0x1234567890123456789012345678901234567890")
    );

    let event = createDepositEvent(
      Address.fromString("0x0987654321098765432109876543210987654321"),
      Address.fromString("0x1234567890123456789012345678901234567890"),
      Bytes.fromHexString(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
      ),
      BigInt.fromI32(100)
    );

    handleDeposit(event);

    // we should have an raindex entity
    assert.entityCount("Raindex", 1);
    assert.fieldEquals(
      "Raindex",
      event.address.toHexString(),
      "id",
      event.address.toHexString()
    );

    // check vault entity
    let vault = Vault.load(
      vaultEntityId(
        event.address,
        event.params.sender,
        event.params.vaultId,
        event.params.token
      )
    );

    assert.assertNotNull(vault);
    if (vault == null) {
      return;
    }
    assert.bytesEquals(vault.balance, FLOAT_100);
    assert.bytesEquals(vault.owner, event.params.sender);
    assert.bytesEquals(vault.token, event.params.token);
    assert.bytesEquals(vault.vaultId, event.params.vaultId);

    // check deposit entity
    let deposit = Deposit.load(eventId(event));

    assert.assertNotNull(deposit);
    if (deposit == null) {
      return;
    }
    assert.bytesEquals(deposit.sender, event.params.sender);
    assert.bytesEquals(deposit.amount, FLOAT_100);
    assert.bytesEquals(deposit.oldVaultBalance, FLOAT_0);
    assert.bytesEquals(deposit.newVaultBalance, FLOAT_100);
    assert.bigIntEquals(deposit.timestamp, event.block.timestamp);

    // make another deposit, same token, same vaultId
    event = createDepositEvent(
      Address.fromString("0x0987654321098765432109876543210987654321"),
      Address.fromString("0x1234567890123456789012345678901234567890"),
      Bytes.fromHexString(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
      ),
      BigInt.fromI32(200)
    );

    handleDeposit(event);

    // check vault entity
    vault = Vault.load(
      vaultEntityId(
        event.address,
        event.params.sender,
        event.params.vaultId,
        event.params.token
      )
    );
    assert.assertNotNull(vault);
    if (vault == null) {
      return;
    }
    assert.bytesEquals(vault.balance, FLOAT_300);
    assert.bytesEquals(vault.owner, event.params.sender);
    assert.bytesEquals(vault.token, event.params.token);
    assert.bytesEquals(vault.vaultId, event.params.vaultId);

    // check deposit entity
    deposit = Deposit.load(eventId(event));

    assert.assertNotNull(deposit);
    if (deposit == null) {
      return;
    }
    assert.bytesEquals(deposit.sender, event.params.sender);
    assert.bytesEquals(deposit.amount, FLOAT_200);
    assert.bytesEquals(deposit.oldVaultBalance, FLOAT_100);
    assert.bytesEquals(deposit.newVaultBalance, FLOAT_300);
    assert.bigIntEquals(deposit.timestamp, event.block.timestamp);

    // make another deposit, different token, same vaultId
    createMockERC20Functions(
      Address.fromString("0x0987654321098765432109876543210987654321")
    );

    event = createDepositEvent(
      Address.fromString("0x0987654321098765432109876543210987654321"),
      Address.fromString("0x0987654321098765432109876543210987654321"),
      Bytes.fromHexString(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
      ),
      BigInt.fromI32(300)
    );

    handleDeposit(event);

    // check vault entity
    vault = Vault.load(
      vaultEntityId(
        event.address,
        event.params.sender,
        event.params.vaultId,
        event.params.token
      )
    );
    assert.assertNotNull(vault);
    if (vault == null) {
      return;
    }
    assert.bytesEquals(vault.balance, FLOAT_300);
    assert.bytesEquals(vault.owner, event.params.sender);
    assert.bytesEquals(vault.token, event.params.token);
    assert.bytesEquals(vault.vaultId, event.params.vaultId);

    // check deposit entity
    deposit = Deposit.load(eventId(event));

    assert.assertNotNull(deposit);
    if (deposit == null) {
      return;
    }
    assert.bytesEquals(deposit.sender, event.params.sender);
    assert.bytesEquals(deposit.amount, FLOAT_300);
    assert.bytesEquals(deposit.oldVaultBalance, FLOAT_0);
    assert.bytesEquals(deposit.newVaultBalance, FLOAT_300);
    assert.bigIntEquals(deposit.timestamp, event.block.timestamp);
  });
});
