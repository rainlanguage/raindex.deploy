import {
  test,
  assert,
  clearStore,
  describe,
  afterEach,
  clearInBlockStore,
} from "matchstick-as";
import { BigInt, Address, crypto, Bytes } from "@graphprotocol/graph-ts";
import { createDepositEntity } from "../src/deposit";
import { createDepositEvent } from "./event-mocks.test";
import { vaultEntityId } from "../src/vault";
import { createMockERC20Functions } from "./erc20.test";
import { FLOAT_100, FLOAT_0 } from "./float.test";

describe("Deposits", () => {
  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("createDepositEntity()", () => {
    const sender = "0x1234567890123456789012345678901234567890";
    const token = "0x0987654321098765432109876543210987654321";
    const vaultId =
      "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    createMockERC20Functions(Address.fromString(token));

    const senderAddr = Address.fromString(sender);
    const tokenAddr = Address.fromString(token);
    const vaultIdBytes = Bytes.fromHexString(vaultId);

    const event = createDepositEvent(
      senderAddr,
      tokenAddr,
      vaultIdBytes,
      BigInt.fromI32(100)
    );
    createDepositEntity(event, FLOAT_0, FLOAT_100, FLOAT_100);

    const id = crypto.keccak256(
      event.address.concat(
        event.transaction.hash.concatI32(event.logIndex.toI32())
      )
    );
    const vaultEId = vaultEntityId(
      event.address,
      senderAddr,
      vaultIdBytes,
      tokenAddr
    );

    assert.entityCount("Deposit", 1);
    assert.fieldEquals(
      "Deposit",
      id.toHexString(),
      "amount",
      FLOAT_100.toHexString()
    );
    assert.fieldEquals(
      "Deposit",
      id.toHexString(),
      "sender",
      sender.toLowerCase()
    );
    assert.fieldEquals(
      "Deposit",
      id.toHexString(),
      "vault",
      vaultEId.toHexString()
    );
    assert.fieldEquals(
      "Deposit",
      id.toHexString(),
      "transaction",
      event.transaction.hash.toHex()
    );
    assert.fieldEquals(
      "Deposit",
      id.toHexString(),
      "oldVaultBalance",
      FLOAT_0.toHexString()
    );
    assert.fieldEquals(
      "Deposit",
      id.toHexString(),
      "newVaultBalance",
      FLOAT_100.toHexString()
    );
    assert.fieldEquals(
      "Deposit",
      id.toHexString(),
      "timestamp",
      event.block.timestamp.toString()
    );

    assert.entityCount("Transaction", 1);
    assert.fieldEquals(
      "Transaction",
      event.transaction.hash.toHex(),
      "blockNumber",
      event.block.number.toString()
    );
    assert.fieldEquals(
      "Transaction",
      event.transaction.hash.toHex(),
      "timestamp",
      event.block.timestamp.toString()
    );
    assert.fieldEquals(
      "Transaction",
      event.transaction.hash.toHex(),
      "from",
      event.transaction.from.toHex()
    );
  });
});
