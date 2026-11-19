import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import MarketsTable from "./MarketsTable.svelte";
import type { RaindexMarketSnapshot } from "@rainlanguage/raindex";

const snapshot: RaindexMarketSnapshot = {
  market: {
    id: "8453:base:quote",
    tickerId: "0xbase_0xquote",
    chainId: 8453,
    base: {
      chainId: 8453,
      address: "0x1111111111111111111111111111111111111111",
      name: "Base token",
      symbol: "BASE",
      decimals: 18,
      logoUri: "https://assets.example/base.png",
      variants: [],
    },
    quote: {
      chainId: 8453,
      address: "0x2222222222222222222222222222222222222222",
      name: "USD Coin",
      symbol: "USDC",
      decimals: 6,
      logoUri: "https://assets.example/usdc.png",
      variants: [],
    },
    raindexAddresses: [],
  },
  orderbook: { bids: [], asks: [] },
  stats: {
    lastPrice: "184.2283035796",
    high24h: "190",
    low24h: "176",
    baseVolume24h: "617.44",
    targetVolume24h: "112854.02",
    tradeCount24h: 190,
  },
  recentTrades: [],
  observedAt: 100,
  errors: [],
};

describe("MarketsTable", () => {
  it("shows active market statistics and selects executable depth", async () => {
    const onSelect = vi.fn();
    render(MarketsTable, {
      props: { markets: [snapshot], selectedTickerId: undefined, onSelect },
    });

    const row = screen.getByTestId("market-row");
    expect(row).toHaveTextContent(/BASE.*USDC/);
    expect(row).toHaveTextContent("112.854K USDC");
    expect(row).toHaveTextContent("190");
    expect(
      row.querySelector('img[src="https://assets.example/base.png"]'),
    ).toBeInTheDocument();
    expect(
      row.querySelector('img[src="https://assets.example/usdc.png"]'),
    ).toBeInTheDocument();

    await fireEvent.click(
      screen.getByRole("button", { name: "View BASE / USDC orderbook depth" }),
    );
    expect(onSelect).toHaveBeenCalledWith(snapshot.market.tickerId);
  });
});
