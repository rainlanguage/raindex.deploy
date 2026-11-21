import { test, assert, describe, afterEach, dataSourceMock } from "matchstick-as";
import { dataSource } from "@graphprotocol/graph-ts";
import { getDecimalFloatAddress } from "../src/float";

// Keep in step with subgraph/networks.json: every network the subgraph is
// deployed to must resolve to the DecimalFloat deployment on that chain.
// A network with no entry cannot index a single Float-bearing event, so a
// missing entry is a broken deployment rather than a degraded one.
const NETWORK_ADDRESSES: string[][] = [
  ["flare", "0x2f665ece3345bf09197dad22a50dfb623bd310a7"],
  ["base", "0x2f665ece3345bf09197dad22a50dfb623bd310a7"],
  ["bsc", "0xdbcb964760d021e18a31c9a731d8589c361e0e20"],
  ["arbitrum-one", "0x2265980d35d97f5f65c73e954d2022380bca4a77"],
  ["matic", "0xb92ad1a33930ab64e0a7dc1acd9eddf9d4f8bc91"],
  ["linea", "0x83e4c7732e715b5e7310796a4a2a21d89f3fb59a"],
  ["mainnet", "0x83e4c7732e715b5e7310796a4a2a21d89f3fb59a"],
  ["robinhood", "0x799632d282178e770c7465cad54ada1021a913d6"],
];

describe("getDecimalFloatAddress", () => {
  afterEach(() => {
    dataSourceMock.resetValues();
  });

  test("resolves the DecimalFloat deployment for every supported network", () => {
    for (let i = 0; i < NETWORK_ADDRESSES.length; i++) {
      let network = NETWORK_ADDRESSES[i][0];
      let expected = NETWORK_ADDRESSES[i][1];
      dataSourceMock.setNetwork(network);
      assert.stringEquals(network, dataSource.network());
      assert.stringEquals(expected, getDecimalFloatAddress().toHexString());
    }
  });

  // A network with no entry is not covered by a test: `log.critical` aborts
  // the whole matchstick process rather than throwing, so it cannot be
  // asserted on from inside the suite.
});
