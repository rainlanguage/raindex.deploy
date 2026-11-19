import { fireEvent, render, screen } from "@testing-library/svelte";
import type { RaindexMarketSnapshot } from "@rainlanguage/raindex";
import { tick } from "svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Page from "./+page.svelte";

const queryMock = vi.hoisted(() => {
  let state: Record<string, unknown>;
  const subscribers = new Set<(value: Record<string, unknown>) => void>();
  return {
    reset() {
      state = {
        data: [],
        isPending: false,
        isError: false,
        isFetching: false,
        isRefetchError: false,
        refetch: vi.fn(),
      };
      for (const subscriber of subscribers) subscriber(state);
    },
    setData(data: RaindexMarketSnapshot[]) {
      state = { ...state, data };
      for (const subscriber of subscribers) subscriber(state);
    },
    subscribe(run: (value: Record<string, unknown>) => void) {
      subscribers.add(run);
      run(state);
      return () => subscribers.delete(run);
    },
  };
});

vi.mock("$app/stores", () => ({
  page: {
    subscribe(run: (value: { url: URL }) => void) {
      run({ url: new URL("http://localhost/markets") });
      return () => undefined;
    },
  },
}));

vi.mock("$env/dynamic/public", () => ({ env: {} }));

vi.mock("@rainlanguage/ui-components", async () => {
  const MockComponent = (
    await import("../../lib/__mocks__/MockComponent.svelte")
  ).default;
  return { PageHeader: MockComponent, getNetworkName: () => "Base" };
});

vi.mock("@tanstack/svelte-query", () => ({
  createQuery: () => ({ subscribe: queryMock.subscribe }),
}));

vi.mock("$lib/components/MarketDetailPanel.svelte", async () => ({
  default: (await import("../../lib/__mocks__/MockComponent.svelte")).default,
}));

function snapshot(
  symbol: string,
  tradeCount24h: number,
): RaindexMarketSnapshot {
  const addressDigit = symbol === "OLD" ? "1" : "2";
  const tickerId = `${symbol.toLowerCase()}_usdc`;
  return {
    market: {
      id: `8453:${tickerId}`,
      tickerId,
      chainId: 8453,
      base: {
        chainId: 8453,
        address: `0x${addressDigit.repeat(40)}`,
        name: symbol,
        symbol,
        decimals: 18,
        variants: [],
      },
      quote: {
        chainId: 8453,
        address: "0x3333333333333333333333333333333333333333",
        name: "USD Coin",
        symbol: "USDC",
        decimals: 6,
        variants: [],
      },
      raindexAddresses: [],
    },
    orderbook: { bids: [], asks: [] },
    stats: {
      lastPrice: "10",
      baseVolume24h: "0",
      targetVolume24h: "0",
      tradeCount24h,
    },
    recentTrades: [],
    observedAt: 100,
    errors: [],
  };
}

describe("Raindex market data page", () => {
  beforeEach(() => queryMock.reset());

  it("treats an empty active market set as a healthy state", () => {
    render(Page);

    expect(
      screen.getByRole("heading", { name: "Raindex market data" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "No active Raindex markets" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "No active Raindex orders currently provide a direct pair with the configured quote token.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("Market service is unavailable"),
    ).not.toBeInTheDocument();
  });

  it("counts only markets with trades in the last 24 hours", () => {
    queryMock.setData([snapshot("OLD", 0), snapshot("LIVE", 2)]);
    render(Page);

    expect(screen.getByText("Traded 24h").parentElement).toHaveTextContent("1");
  });

  it("clears a selected market after it leaves the active overview", async () => {
    const selected = snapshot("OLD", 1);
    const remaining = snapshot("LIVE", 1);
    queryMock.setData([selected, remaining]);
    render(Page);

    await fireEvent.click(
      screen.getByRole("button", { name: "View OLD / USDC orderbook depth" }),
    );
    expect(
      screen.getByRole("button", { name: "OLD / USDC depth selected" }),
    ).toBeInTheDocument();

    queryMock.setData([remaining]);
    await tick();

    expect(
      screen.queryByRole("button", { name: "OLD / USDC depth selected" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "View LIVE / USDC orderbook depth" }),
    ).toBeInTheDocument();
  });
});
