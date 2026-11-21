import { Bytes, ethereum, crypto } from "@graphprotocol/graph-ts";

export function eventId(event: ethereum.Event): Bytes {
  let bytes = event.address.concat(
    event.transaction.hash.concat(
      Bytes.fromByteArray(Bytes.fromBigInt(event.logIndex))
    )
  );
  return Bytes.fromByteArray(crypto.keccak256(bytes));
}
