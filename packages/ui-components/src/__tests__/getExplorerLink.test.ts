import { getExplorerLink } from "../lib/services/getExplorerLink";

vi.mock("viem/chains", () => ({
  mainnet: {
    id: 999,
    blockExplorers: {
      default: {
        url: "https://etherscan.io",
      },
    },
  },
}));

describe("getExplorerLink", () => {
  it("should return the explorer link", () => {
    expect(getExplorerLink("0x123", 999, "tx")).toBe(
      "https://etherscan.io/tx/0x123",
    );
  });
  it("should return an empty string if the chain is not found", () => {
    expect(getExplorerLink("0x123", 1, "tx")).toBe("");
  });
  it("returns the Robinhood Blockscout link for chain 4663", () => {
    expect(getExplorerLink("0xabc", 4663, "tx")).toBe(
      "https://robinhoodchain.blockscout.com/tx/0xabc",
    );
  });
});
