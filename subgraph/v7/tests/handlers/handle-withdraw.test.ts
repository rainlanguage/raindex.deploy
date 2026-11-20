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
import { createDepositEvent, createWithdrawEvent } from "../event-mocks.test";
import { handleDeposit, handleWithdraw } from "../../src/handlers";
import { vaultEntityId } from "../../src/vault";
import { Withdrawal, Vault } from "../../generated/schema";
import { eventId } from "../../src/interfaces/event";
import { createMockERC20Functions } from "../erc20.test";
import {
  createMockDecimalFloatFunctions,
  FLOAT_100,
  FLOAT_1000,
  FLOAT_150,
  FLOAT_200,
  FLOAT_300,
  FLOAT_700,
  FLOAT_800,
  FLOAT_900,
  FLOAT_NEG_100,
  FLOAT_NEG_200,
} from "../float.test";

describe("Handle withdraw", () => {
  beforeEach(createMockDecimalFloatFunctions);

  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("handleWithdraw()", () => {
    let token = "0x1234567890123456789012345678901234567890";
    createMockERC20Functions(Address.fromString(token));

    let sender = "0x0987654321098765432109876543210987654321";
    let vaultId =
      "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    // first we make a deposit
    let depositEvent = createDepositEvent(
      Address.fromString(sender),
      Address.fromString(token),
      Bytes.fromHexString(vaultId),
      BigInt.fromI32(1000)
    );
    handleDeposit(depositEvent);

    // then we make a withdraw
    let event = createWithdrawEvent(
      Address.fromString(sender),
      Address.fromString(token),
      Bytes.fromHexString(vaultId),
      FLOAT_150,
      FLOAT_100,
      BigInt.fromI32(100)
    );

    handleWithdraw(event);

    // we should have a raindex entity
    assert.entityCount(
      "Raindex",
      1,
      "Expected exactly 1 Raindex entity after first withdraw"
    );
    assert.fieldEquals(
      "Raindex",
      event.address.toHexString(),
      "id",
      event.address.toHexString(),
      "Raindex entity id does not match event address"
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
    assert.assertNotNull(
      vault,
      "Vault entity should not be null after first withdraw"
    );
    if (vault == null) {
      return;
    }
    assert.bytesEquals(
      vault.balance,
      FLOAT_900,
      "Vault balance should be FLOAT_900 after first withdraw"
    );
    assert.bytesEquals(
      vault.owner,
      event.params.sender,
      "Vault owner does not match event sender"
    );
    assert.bytesEquals(
      vault.token,
      event.params.token,
      "Vault token does not match event token"
    );
    assert.bytesEquals(
      vault.vaultId,
      event.params.vaultId,
      "Vault vaultId does not match event vaultId"
    );

    // check withdraw entity
    let withdraw = Withdrawal.load(eventId(event));

    assert.assertNotNull(
      withdraw,
      "Withdrawal entity should not be null after first withdraw"
    );
    if (withdraw == null) {
      return;
    }
    assert.bytesEquals(
      withdraw.sender,
      event.params.sender,
      "Withdrawal sender does not match event sender"
    );
    assert.bytesEquals(
      withdraw.amount,
      FLOAT_NEG_100,
      "Withdrawal amount should be FLOAT_NEG_100 after first withdraw"
    );
    assert.bytesEquals(
      withdraw.oldVaultBalance,
      FLOAT_1000,
      "Withdrawal oldVaultBalance should be FLOAT_1000 after first withdraw"
    );
    assert.bytesEquals(
      withdraw.newVaultBalance,
      FLOAT_900,
      "Withdrawal newVaultBalance should be FLOAT_900 after first withdraw"
    );

    // make another withdraw, same token, same vaultId
    event = createWithdrawEvent(
      Address.fromString(sender),
      Address.fromString(token),
      Bytes.fromHexString(vaultId),
      FLOAT_200,
      FLOAT_200,
      BigInt.fromI32(200)
    );

    handleWithdraw(event);

    // check vault entity
    vault = Vault.load(
      vaultEntityId(
        event.address,
        event.params.sender,
        event.params.vaultId,
        event.params.token
      )
    );
    assert.assertNotNull(
      vault,
      "Vault entity should not be null after second withdraw"
    );
    if (vault == null) {
      return;
    }
    assert.bytesEquals(
      vault.balance,
      FLOAT_700,
      "Vault balance should be FLOAT_700 after second withdraw"
    );
    assert.bytesEquals(
      vault.owner,
      event.params.sender,
      "Vault owner does not match event sender after second withdraw"
    );
    assert.bytesEquals(
      vault.token,
      event.params.token,
      "Vault token does not match event token after second withdraw"
    );
    assert.bytesEquals(
      vault.vaultId,
      event.params.vaultId,
      "Vault vaultId does not match event vaultId after second withdraw"
    );

    // check withdraw entity
    withdraw = Withdrawal.load(eventId(event));

    assert.assertNotNull(
      withdraw,
      "Withdrawal entity should not be null after second withdraw"
    );
    if (withdraw == null) {
      return;
    }
    assert.bytesEquals(
      withdraw.sender,
      event.params.sender,
      "Withdrawal sender does not match event sender after second withdraw"
    );
    assert.bytesEquals(
      withdraw.amount,
      FLOAT_NEG_200,
      "Withdrawal amount should be FLOAT_NEG_200 after second withdraw"
    );
    assert.bytesEquals(
      withdraw.oldVaultBalance,
      FLOAT_900,
      `Withdrawal oldVaultBalance should be ${FLOAT_900.toHexString()} after ` +
        `second withdraw, instead got ${withdraw.oldVaultBalance.toHexString()}`
    );
    assert.bytesEquals(
      withdraw.newVaultBalance,
      FLOAT_700,
      "Withdrawal newVaultBalance should be FLOAT_700 after second withdraw"
    );
    assert.bigIntEquals(
      withdraw.timestamp,
      event.block.timestamp,
      "Withdrawal timestamp does not match event block timestamp after second withdraw"
    );

    createMockERC20Functions(Address.fromString(sender));

    // deposit different token, same vaultId
    depositEvent = createDepositEvent(
      Address.fromString(sender),
      Address.fromString(sender),
      Bytes.fromHexString(vaultId),
      BigInt.fromI32(300)
    );

    handleDeposit(depositEvent);

    // make a withdraw for the new token
    event = createWithdrawEvent(
      Address.fromString(sender),
      Address.fromString(sender),
      Bytes.fromHexString(vaultId),
      FLOAT_300,
      FLOAT_200,
      BigInt.fromI32(200)
    );

    handleWithdraw(event);

    vault = Vault.load(
      vaultEntityId(
        event.address,
        event.params.sender,
        event.params.vaultId,
        event.params.token
      )
    );

    assert.assertNotNull(
      vault,
      "Vault entity should not be null after withdraw for new token"
    );
    if (vault == null) {
      return;
    }

    assert.bytesEquals(
      vault.balance,
      FLOAT_100,
      `Vault balance should be ${FLOAT_100.toHexString()} instead got ${vault.balance.toHexString()}`
    );
    assert.bytesEquals(
      vault.owner,
      event.params.sender,
      "Vault owner does not match event sender after withdraw for new token"
    );
    assert.bytesEquals(
      vault.token,
      event.params.token,
      "Vault token does not match event token after withdraw for new token"
    );
    assert.bytesEquals(
      vault.vaultId,
      event.params.vaultId,
      "Vault vaultId does not match event vaultId after withdraw for new token"
    );

    // check withdraw entity
    withdraw = Withdrawal.load(eventId(event));

    assert.assertNotNull(
      withdraw,
      "Withdrawal entity should not be null after withdraw for new token"
    );
    if (withdraw == null) {
      return;
    }
    assert.bytesEquals(
      withdraw.sender,
      event.params.sender,
      "Withdrawal sender does not match event sender after withdraw for new token"
    );
    assert.bytesEquals(
      withdraw.amount,
      FLOAT_NEG_200,
      "Withdrawal amount should be FLOAT_NEG_200 after withdraw for new token"
    );
    assert.bytesEquals(
      withdraw.oldVaultBalance,
      FLOAT_300,
      "Withdrawal oldVaultBalance should be FLOAT_300 after withdraw for new token"
    );
    assert.bytesEquals(
      withdraw.newVaultBalance,
      FLOAT_100,
      `Withdrawal newVaultBalance should be ${FLOAT_100.toHexString()} ` +
        `instead got ${withdraw.newVaultBalance.toHexString()}`
    );
    assert.bigIntEquals(
      withdraw.timestamp,
      event.block.timestamp,
      "Withdrawal timestamp does not match event block timestamp after withdraw for new token"
    );
  });
});
