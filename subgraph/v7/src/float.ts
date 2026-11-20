import { Address, Bytes, dataSource, log } from "@graphprotocol/graph-ts";
import { DecimalFloat } from "../generated/Raindex/DecimalFloat";

export type Float = Bytes;

const UNKNOWN_NETWORK_ADDRESS = Address.fromString(
  "0x0000000000000000000000000000000000000001"
);

export function getCalculator(): DecimalFloat {
  return DecimalFloat.bind(getDecimalFloatAddress());
}

export function getDecimalFloatAddress(): Address {
  let network = dataSource.network();
  if (network == "flare") {
    return Address.fromString("0x2F665EcE3345bF09197DAd22A50dFB623BD310A7");
  } else if (network == "base") {
    return Address.fromString("0x2F665EcE3345bF09197DAd22A50dFB623BD310A7");
  } else if (network == "bsc") {
    return Address.fromString("0xDbcb964760d021e18A31C9A731d8589c361E0E20");
  } else if (network == "arbitrum-one") {
    return Address.fromString("0x2265980d35d97F5f65C73e954D2022380bcA4A77");
  } else if (network == "matic") {
    return Address.fromString("0xb92aD1A33930aB64e0A7DC1AcD9EDDf9d4F8bc91");
  } else if (network == "linea") {
    return Address.fromString("0x83e4c7732e715b5E7310796A4A2a21d89f3FB59A");
  } else if (network == "mainnet") {
    return Address.fromString("0x83e4c7732e715b5E7310796A4A2a21d89f3FB59A");
  } else if (network == "robinhood") {
    return Address.fromString("0x799632d282178e770C7465cad54aDA1021A913D6");
  }

  // Every network in networks.json needs an entry above. Every Float value the
  // subgraph stores comes from an `eth_call` against this contract, so a
  // network that falls through here has no working mapping at all: the first
  // event that touches a Float reverts the call, the handler traps, and the
  // whole subgraph stops with an opaque indexing error. Name the missing
  // network instead of leaving that to be worked out from the outside.
  log.critical("No DecimalFloat deployment known for network: {}", [network]);
  return UNKNOWN_NETWORK_ADDRESS;
}
