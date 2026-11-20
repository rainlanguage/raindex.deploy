import {
  test,
  assert,
  clearStore,
  describe,
  afterEach,
  clearInBlockStore,
  createMockedFunction,
} from "matchstick-as";
import { BigInt, Address, ethereum, Bytes } from "@graphprotocol/graph-ts";
import { createERC20Entity } from "../src/erc20";

describe("Deposits", () => {
  afterEach(() => {
    clearStore();
    clearInBlockStore();
  });

  test("createERC20Entity()", () => {
    let address = Address.fromString(
      "0x1234567890123456789012345678901234567890"
    );
    createMockERC20Functions(address);
    createERC20Entity(Bytes.fromByteArray(address));

    assert.entityCount("ERC20", 1);
  });
});

export function createMockERC20Functions(address: Address): void {
  createMockedFunction(address, "name", "name():(string)")
    .withArgs([])
    .returns([ethereum.Value.fromString("Test")]);
  createMockedFunction(address, "symbol", "symbol():(string)")
    .withArgs([])
    .returns([ethereum.Value.fromString("TST")]);
  createMockedFunction(address, "decimals", "decimals():(uint8)")
    .withArgs([])
    .returns([ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(18))]);
}
