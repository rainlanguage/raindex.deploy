import { Bytes, BigInt, Address } from "@graphprotocol/graph-ts";
import { makeClearEventId, handleClear, getOrdersHash } from "../../src/clear";
import {
  IOV2,
  Evaluable,
  createOrder,
  createClearEvent,
} from "../event-mocks.test";
import {
  test,
  assert,
  describe,
  afterEach,
  clearStore,
  clearInBlockStore,
} from "matchstick-as";

const alice = Address.fromString("0x850c40aBf6e325231ba2DeD1356d1f2c267e63Ce");
const bob = Address.fromString("0x813aef302Ebad333EDdef619C6f8eD7FeF51BA7c");

const aliceVaultId = Bytes.fromHexString(
  "0xa1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1"
);
const bobVaultId = Bytes.fromHexString(
  "0xb0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0"
);
const aliceBountyVaultId = Bytes.fromHexString(
  "0xabababababababababababababababababababababababababababababababab"
);
const bobBountyVaultId = Bytes.fromHexString(
  "0xabababababababababababababababababababababababababababababababab"
);

const token1 = Address.fromString("0x12e605bc104e93B45e1aD99F9e555f659051c2BB");
const token2 = Address.fromString("0x12e605bc104e93B45e1aD99F9e555f659051c2Bc");
const token3 = Address.fromString("0x12e605bc104e93B45e1aD99F9e555f659051c2Ba");

describe("Handle Clear", () => {
  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("handleClear()", () => {
    let evaluable = new Evaluable(
      Address.fromString("0x5fB33D710F8B58DE4c9fDEC703B5c2487a5219d6"),
      Address.fromString("0x84c6e7F5A1e5dD89594Cc25BEf4722A1b8871aE6"),
      Bytes.fromHexString("0x1234567890123456789012345678901234567890")
    );
    let nonce = Bytes.fromHexString(
      "0xbce73059f54ada335f7283df99f81d42a3f2d09527eade865627e26cd756b748"
    );

    let event = createClearEvent(
      alice,
      createOrder(
        alice,
        evaluable,
        [new IOV2(token1, aliceVaultId)],
        [new IOV2(token2, aliceVaultId)],
        nonce
      ),
      createOrder(
        bob,
        evaluable,
        [new IOV2(token2, bobVaultId)],
        [new IOV2(token3, bobVaultId), new IOV2(token1, bobVaultId)],
        nonce
      ),
      BigInt.fromI32(0),
      BigInt.fromI32(0),
      BigInt.fromI32(0),
      BigInt.fromI32(1),
      aliceBountyVaultId,
      bobBountyVaultId
    );

    let id = makeClearEventId(event).toHexString();
    let orderHashes = getOrdersHash(event);
    handleClear(event);

    assert.entityCount("ClearTemporaryData", 1);

    // alice
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "aliceAddress",
      alice.toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "aliceOrderHash",
      orderHashes[0].toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "aliceInputToken",
      token1.toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "aliceOutputToken",
      token2.toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "aliceInputVaultId",
      aliceVaultId.toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "aliceOutputVaultId",
      aliceVaultId.toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "aliceBounty",
      aliceBountyVaultId.toHexString()
    );

    // bob
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "bobAddress",
      bob.toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "bobAddress",
      bob.toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "bobOrderHash",
      orderHashes[1].toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "bobInputToken",
      token2.toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "bobOutputToken",
      token1.toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "bobInputVaultId",
      bobVaultId.toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "bobOutputVaultId",
      bobVaultId.toHexString()
    );
    assert.fieldEquals(
      "ClearTemporaryData",
      id,
      "bobBounty",
      bobBountyVaultId.toHexString()
    );
  });
});
