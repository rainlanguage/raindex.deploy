import {
  test,
  assert,
  clearStore,
  describe,
  afterEach,
  clearInBlockStore,
  beforeEach,
} from "matchstick-as";
import { handleVaultBalanceChange, vaultEntityId } from "../src/vault";
import { Bytes, BigInt, Address } from "@graphprotocol/graph-ts";
import { createDepositEvent, createWithdrawEvent } from "./event-mocks.test";
import { createMockERC20Functions } from "./erc20.test";
import {
  createMockDecimalFloatFunctions,
  FLOAT_100,
  FLOAT_200,
  FLOAT_NEG_100,
  FLOAT_0,
} from "./float.test";

describe("Vault balance changes", () => {
  beforeEach(createMockDecimalFloatFunctions);

  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("handleVaultBalanceChange()", () => {
    createMockERC20Functions(
      Address.fromString("0x1234567890123456789012345678901234567890")
    );

    let owner = "0x0987654321098765432109876543210987654321";

    let raindex =
      "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    let vaultId =
      "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let token = "0x1234567890123456789012345678901234567890";

    handleVaultBalanceChange(
      Bytes.fromHexString(raindex),
      Bytes.fromHexString(vaultId),
      Bytes.fromHexString(token),
      FLOAT_100,
      Bytes.fromHexString(owner)
    );

    let vaultEId = vaultEntityId(
      Bytes.fromHexString(raindex),
      Bytes.fromHexString(owner),
      Bytes.fromHexString(vaultId),
      Bytes.fromHexString(token)
    );

    assert.entityCount("Vault", 1);
    assert.fieldEquals(
      "Vault",
      vaultEId.toHexString(),
      "balance",
      FLOAT_100.toHexString()
    );
    assert.fieldEquals("Vault", vaultEId.toHexString(), "token", token);
    assert.fieldEquals("Vault", vaultEId.toHexString(), "vaultId", vaultId);
    assert.fieldEquals("Vault", vaultEId.toHexString(), "owner", owner);
  });

  test("handleVaultDeposit()", () => {
    let token = "0x1234567890123456789012345678901234567890";
    createMockERC20Functions(Address.fromString(token));

    let sender = "0x0987654321098765432109876543210987654321";
    let vaultId =
      "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let event = createDepositEvent(
      Address.fromString(sender),
      Address.fromString(token),
      Bytes.fromHexString(vaultId),
      BigInt.fromI32(100)
    );
    handleVaultBalanceChange(
      event.address,
      event.params.vaultId,
      event.params.token,
      FLOAT_100,
      event.params.sender
    );

    let vaultEId = vaultEntityId(
      event.address,
      Address.fromString(sender),
      Bytes.fromHexString(vaultId),
      Address.fromString(token)
    );

    assert.entityCount("Vault", 1);
    assert.fieldEquals(
      "Vault",
      vaultEId.toHexString(),
      "balance",
      FLOAT_100.toHexString()
    );
    assert.fieldEquals("Vault", vaultEId.toHexString(), "token", token);
    assert.fieldEquals("Vault", vaultEId.toHexString(), "vaultId", vaultId);
    assert.fieldEquals("Vault", vaultEId.toHexString(), "owner", sender);
  });

  test("handleVaultWithdraw()", () => {
    let token = "0x1234567890123456789012345678901234567890";
    createMockERC20Functions(Address.fromString(token));

    let sender = "0x0987654321098765432109876543210987654321";
    let vaultId =
      "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    // first we need to deposit
    let depositEvent = createDepositEvent(
      Address.fromString(sender),
      Address.fromString(token),
      Bytes.fromHexString(vaultId),
      BigInt.fromI32(200)
    );

    handleVaultBalanceChange(
      depositEvent.address,
      depositEvent.params.vaultId,
      depositEvent.params.token,
      FLOAT_200,
      depositEvent.params.sender
    );

    // then we withdraw
    let event = createWithdrawEvent(
      Address.fromString(sender),
      Address.fromString(token),
      Bytes.fromHexString(vaultId),
      FLOAT_200,
      FLOAT_100,
      BigInt.fromI32(100)
    );
    handleVaultBalanceChange(
      event.address,
      event.params.vaultId,
      event.params.token,
      FLOAT_NEG_100,
      event.params.sender
    );

    let vaultEId = vaultEntityId(
      event.address,
      Address.fromString(sender),
      Bytes.fromHexString(vaultId),
      Address.fromString(token)
    );

    assert.entityCount("Vault", 1);
    assert.fieldEquals(
      "Vault",
      vaultEId.toHexString(),
      "balance",
      FLOAT_100.toHexString()
    );
    assert.fieldEquals("Vault", vaultEId.toHexString(), "token", token);
    assert.fieldEquals("Vault", vaultEId.toHexString(), "vaultId", vaultId);
    assert.fieldEquals("Vault", vaultEId.toHexString(), "owner", sender);
  });

  test("If vault does not exist, create it", () => {
    assert.entityCount("Vault", 0);

    createMockERC20Functions(
      Address.fromString("0x1234567890123456789012345678901234567890")
    );

    let sender = "0x0987654321098765432109876543210987654321";
    let token = "0x1234567890123456789012345678901234567890";
    let vaultHexId =
      "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let event = createDepositEvent(
      Address.fromString(sender),
      Address.fromString(token),
      Bytes.fromHexString(vaultHexId),
      BigInt.fromI32(100)
    );

    handleVaultBalanceChange(
      event.address,
      event.params.vaultId,
      event.params.token,
      FLOAT_100,
      event.params.sender
    );

    let vaultId = vaultEntityId(
      event.address,
      Address.fromString(sender),
      Bytes.fromHexString(vaultHexId),
      Address.fromString(token)
    );

    assert.entityCount("Vault", 1);
    assert.fieldEquals(
      "Vault",
      vaultId.toHexString(),
      "balance",
      FLOAT_100.toHexString()
    );
    assert.fieldEquals("Vault", vaultId.toHexString(), "token", token);
    assert.fieldEquals("Vault", vaultId.toHexString(), "vaultId", vaultHexId);
    assert.fieldEquals("Vault", vaultId.toHexString(), "owner", sender);
  });

  test("handleVaultBalanceChange returns 0 if vault doesn't exist yet", () => {
    createMockERC20Functions(
      Address.fromString("0x1234567890123456789012345678901234567890")
    );

    let sender = "0x0987654321098765432109876543210987654321";
    let token = "0x1234567890123456789012345678901234567890";
    let vaultHexId =
      "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let balanceChange = handleVaultBalanceChange(
      Address.fromString(sender),
      Bytes.fromHexString(vaultHexId),
      Bytes.fromHexString(token),
      FLOAT_100,
      Bytes.fromHexString(sender)
    );

    assert.bytesEquals(balanceChange.oldVaultBalance, FLOAT_0);
  });

  test("handleVaultBalanceChange returns old balance if vault exists", () => {
    createMockERC20Functions(
      Address.fromString("0x1234567890123456789012345678901234567890")
    );

    let sender = "0x0987654321098765432109876543210987654321";
    let token = "0x1234567890123456789012345678901234567890";
    let vaultHexId =
      "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    handleVaultBalanceChange(
      Address.fromString(sender),
      Bytes.fromHexString(vaultHexId),
      Bytes.fromHexString(token),
      FLOAT_100,
      Bytes.fromHexString(sender)
    );

    let balanceChange = handleVaultBalanceChange(
      Address.fromString(sender),
      Bytes.fromHexString(vaultHexId),
      Bytes.fromHexString(token),
      FLOAT_100,
      Bytes.fromHexString(sender)
    );

    assert.bytesEquals(balanceChange.oldVaultBalance, FLOAT_100);
  });
});
