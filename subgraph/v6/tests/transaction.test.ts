import {
  test,
  assert,
  clearStore,
  describe,
  afterEach,
  newMockEvent,
  clearInBlockStore,
} from "matchstick-as";
import { createTransactionEntity } from "../src/transaction";
import { Bytes } from "@graphprotocol/graph-ts";

describe("Transaction", () => {
  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("createTransactionEntity()", () => {
    let event = newMockEvent();

    createTransactionEntity(event);

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

  test("has no problem with multiple events in the same transaction", () => {
    let event1 = newMockEvent();
    let event2 = newMockEvent();
    event2.transaction = event1.transaction;

    assert.bytesEquals(event1.transaction.hash, event2.transaction.hash);

    // these two events share the same transaction
    createTransactionEntity(event1);
    createTransactionEntity(event2);

    // only one transaction entity should be created
    assert.entityCount("Transaction", 1);

    // now let's create an event with a unique transaction
    let event3 = newMockEvent();
    event3.transaction.hash = event3.transaction.hash.concat(
      Bytes.fromHexString("0x01")
    );

    createTransactionEntity(event3);

    assert.assertTrue(event3.transaction.hash != event1.transaction.hash);

    // now we should have two transaction entities
    assert.entityCount("Transaction", 2);
  });
});
