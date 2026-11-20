import {
  test,
  assert,
  clearStore,
  describe,
  afterEach,
  clearInBlockStore,
  beforeEach,
} from "matchstick-as";
import { BigInt, Address, crypto, Bytes } from "@graphprotocol/graph-ts";
import { createWithdrawalEntity } from "../src/withdraw";
import { createWithdrawEvent } from "./event-mocks.test";
import { vaultEntityId } from "../src/vault";
import { createMockERC20Functions } from "./erc20.test";
import {
  createMockDecimalFloatFunctions,
  FLOAT_100,
  FLOAT_200,
  FLOAT_300,
  FLOAT_NEG_100,
} from "./float.test";
import { getCalculator } from "../src/float";

describe("Withdrawals", () => {
  beforeEach(createMockDecimalFloatFunctions);

  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("createWithdrawalEntity()", () => {
    const token = "0x0987654321098765432109876543210987654321";
    createMockERC20Functions(Address.fromString(token));

    const sender = "0x1234567890123456789012345678901234567890";
    const vaultId =
      "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let event = createWithdrawEvent(
      Address.fromString(sender),
      Address.fromString(token),
      Bytes.fromHexString(vaultId),
      FLOAT_200,
      FLOAT_100,
      BigInt.fromI32(100)
    );

    let oldVaultBalance = FLOAT_300;
    let calculator = getCalculator();
    createWithdrawalEntity(calculator, event, oldVaultBalance);

    let id = crypto.keccak256(
      event.address.concat(
        event.transaction.hash.concatI32(event.logIndex.toI32())
      )
    );
    let vaultEId = vaultEntityId(
      event.address,
      Address.fromString(sender),
      Bytes.fromHexString(vaultId),
      Address.fromString(token)
    );

    assert.entityCount("Withdrawal", 1);
    assert.fieldEquals(
      "Withdrawal",
      id.toHexString(),
      "amount",
      FLOAT_NEG_100.toHexString()
    );
    assert.fieldEquals(
      "Withdrawal",
      id.toHexString(),
      "targetAmount",
      FLOAT_200.toHexString()
    );
    assert.fieldEquals("Withdrawal", id.toHexString(), "sender", sender);
    assert.fieldEquals(
      "Withdrawal",
      id.toHexString(),
      "vault",
      vaultEId.toHexString()
    );
    assert.fieldEquals(
      "Withdrawal",
      id.toHexString(),
      "transaction",
      event.transaction.hash.toHex()
    );
    assert.fieldEquals(
      "Withdrawal",
      id.toHexString(),
      "oldVaultBalance",
      FLOAT_300.toHexString()
    );
    assert.fieldEquals(
      "Withdrawal",
      id.toHexString(),
      "newVaultBalance",
      FLOAT_200.toHexString()
    );
    assert.fieldEquals(
      "Withdrawal",
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
  });
});
