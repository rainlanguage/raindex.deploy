import { createMockedFunction } from "matchstick-as";
import { Bytes, BigInt, ethereum } from "@graphprotocol/graph-ts";
import { getDecimalFloatAddress } from "../src/float";

export const FLOAT_0 = Bytes.fromHexString(
  "0x0000000000000000000000000000000000000000000000000000000000000000"
);
export const FLOAT_1 = Bytes.fromHexString(
  "0x0000000000000000000000000000000000000000000000000000000000000001"
);
export const FLOAT_2 = Bytes.fromHexString(
  "0x0000000000000000000000000000000000000000000000000000000000000002"
);
export const FLOAT_3 = Bytes.fromHexString(
  "0x0000000000000000000000000000000000000000000000000000000000000003"
);
export const FLOAT_5 = Bytes.fromHexString(
  "0x0000000000000000000000000000000000000000000000000000000000000005"
);
export const FLOAT_10 = Bytes.fromHexString(
  "0x000000000000000000000000000000000000000000000000000000000000000a"
);
export const FLOAT_11 = Bytes.fromHexString(
  "0x000000000000000000000000000000000000000000000000000000000000000b"
);
export const FLOAT_15 = Bytes.fromHexString(
  "0x000000000000000000000000000000000000000000000000000000000000000f"
);
export const FLOAT_20 = Bytes.fromHexString(
  "0x0000000000000000000000000000000000000000000000000000000000000014"
);
export const FLOAT_150 = Bytes.fromHexString(
  "0x0000000000000000000000000000000000000000000000000000000000000096"
);
export const FLOAT_100 = Bytes.fromHexString(
  "0x0000000000000000000000000000000000000000000000000000000000000064"
);
export const FLOAT_200 = Bytes.fromHexString(
  "0x00000000000000000000000000000000000000000000000000000000000000c8"
);
export const FLOAT_300 = Bytes.fromHexString(
  "0x000000000000000000000000000000000000000000000000000000000000012c"
);
export const FLOAT_700 = Bytes.fromHexString(
  "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
);
export const FLOAT_800 = Bytes.fromHexString(
  "0x0000000000000000000000000000000000000000000000000000000000000320"
);
export const FLOAT_900 = Bytes.fromHexString(
  "0x0000000000000000000000000000000000000000000000000000000000000384"
);
export const FLOAT_1000 = Bytes.fromHexString(
  "0x00000000000000000000000000000000000000000000000000000000000003e8"
);
export const FLOAT_1100 = Bytes.fromHexString(
  "0x000000000000000000000000000000000000000000000000000000000000044c"
);

export const FLOAT_NEG_1 = Bytes.fromHexString(
  "0x00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
);
export const FLOAT_NEG_10 = Bytes.fromHexString(
  "0x00000000fffffffffffffffffffffffffffffffffffffffffffffffffffffff6"
);
export const FLOAT_NEG_20 = Bytes.fromHexString(
  "0x00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffec"
);
export const FLOAT_NEG_100 = Bytes.fromHexString(
  "0x00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffff9c"
);
export const FLOAT_NEG_200 = Bytes.fromHexString(
  "0x00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffff38"
);

export function createMockDecimalFloatFunctions(): void {
  createMockedFunction(
    getDecimalFloatAddress(),
    "parse",
    "parse(string):(bytes4,bytes32)"
  )
    .withArgs([ethereum.Value.fromString("0")])
    .returns([
      ethereum.Value.fromFixedBytes(Bytes.fromHexString("0x00000000")),
      ethereum.Value.fromFixedBytes(FLOAT_0),
    ]);

  function fromFixedDecimal(amount: BigInt, result: Bytes): void {
    createMockedFunction(
      getDecimalFloatAddress(),
      "fromFixedDecimalLossless",
      "fromFixedDecimalLossless(uint256,uint8):(bytes32)"
    )
      .withArgs([
        ethereum.Value.fromUnsignedBigInt(amount),
        ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(18)),
      ])
      .returns([ethereum.Value.fromFixedBytes(result)]);
  }
  fromFixedDecimal(BigInt.fromI32(100), FLOAT_100);
  fromFixedDecimal(BigInt.fromI32(200), FLOAT_200);
  fromFixedDecimal(BigInt.fromI32(300), FLOAT_300);
  fromFixedDecimal(BigInt.fromI32(1000), FLOAT_1000);

  function minusMock(input: Bytes, output: Bytes): void {
    createMockedFunction(
      getDecimalFloatAddress(),
      "minus",
      "minus(bytes32):(bytes32)"
    )
      .withArgs([ethereum.Value.fromFixedBytes(input)])
      .returns([ethereum.Value.fromFixedBytes(output)]);
  }
  minusMock(FLOAT_1, FLOAT_NEG_1);
  minusMock(FLOAT_10, FLOAT_NEG_10);
  minusMock(FLOAT_20, FLOAT_NEG_20);
  minusMock(FLOAT_100, FLOAT_NEG_100);
  minusMock(FLOAT_200, FLOAT_NEG_200);

  function addMock(a: Bytes, b: Bytes, sum: Bytes): void {
    createMockedFunction(
      getDecimalFloatAddress(),
      "add",
      "add(bytes32,bytes32):(bytes32)"
    )
      .withArgs([
        ethereum.Value.fromFixedBytes(a),
        ethereum.Value.fromFixedBytes(b),
      ])
      .returns([ethereum.Value.fromFixedBytes(sum)]);
  }
  addMock(FLOAT_0, FLOAT_NEG_20, FLOAT_NEG_20);
  addMock(FLOAT_0, FLOAT_NEG_10, FLOAT_NEG_10);
  addMock(FLOAT_0, FLOAT_NEG_1, FLOAT_NEG_1);
  addMock(FLOAT_0, FLOAT_1, FLOAT_1);
  addMock(FLOAT_0, FLOAT_5, FLOAT_5);
  addMock(FLOAT_0, FLOAT_10, FLOAT_10);
  addMock(FLOAT_0, FLOAT_15, FLOAT_15);
  addMock(FLOAT_0, FLOAT_100, FLOAT_100);
  addMock(FLOAT_0, FLOAT_200, FLOAT_200);
  addMock(FLOAT_0, FLOAT_300, FLOAT_300);
  addMock(FLOAT_0, FLOAT_1000, FLOAT_1000);
  addMock(FLOAT_10, FLOAT_1, FLOAT_11);
  addMock(FLOAT_100, FLOAT_100, FLOAT_200);
  addMock(FLOAT_100, FLOAT_200, FLOAT_300);
  addMock(FLOAT_200, FLOAT_NEG_100, FLOAT_100);
  addMock(FLOAT_300, FLOAT_NEG_200, FLOAT_100);
  addMock(FLOAT_900, FLOAT_NEG_200, FLOAT_700);
  addMock(FLOAT_900, FLOAT_200, FLOAT_1100);
  addMock(FLOAT_1000, FLOAT_NEG_100, FLOAT_900);

  function subMock(a: Bytes, b: Bytes, diff: Bytes): void {
    createMockedFunction(
      getDecimalFloatAddress(),
      "sub",
      "sub(bytes32,bytes32):(bytes32)"
    )
      .withArgs([
        ethereum.Value.fromFixedBytes(a),
        ethereum.Value.fromFixedBytes(b),
      ])
      .returns([ethereum.Value.fromFixedBytes(diff)]);
  }
  subMock(FLOAT_10, FLOAT_10, FLOAT_0);
  subMock(FLOAT_20, FLOAT_15, FLOAT_5);
  subMock(FLOAT_300, FLOAT_100, FLOAT_200);
  subMock(FLOAT_300, FLOAT_200, FLOAT_100);
  subMock(FLOAT_900, FLOAT_200, FLOAT_700);
  subMock(FLOAT_1000, FLOAT_100, FLOAT_900);
  subMock(FLOAT_1000, FLOAT_200, FLOAT_800);

  function gtMock(a: Bytes, b: Bytes, result: boolean): void {
    createMockedFunction(
      getDecimalFloatAddress(),
      "gt",
      "gt(bytes32,bytes32):(bool)"
    )
      .withArgs([
        ethereum.Value.fromFixedBytes(a),
        ethereum.Value.fromFixedBytes(b),
      ])
      .returns([ethereum.Value.fromBoolean(result)]);
  }
  gtMock(FLOAT_5, FLOAT_0, true);
  gtMock(FLOAT_0, FLOAT_0, false);
}
