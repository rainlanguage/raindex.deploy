import { describe, expect, it, vi } from "vitest";
import { getMarket, getMarkets, MarketApiError, marketApiUrl } from "./client";

vi.mock("$env/dynamic/public", () => ({ env: {} }));

const fetcher = (response: Response) =>
  vi.fn().mockResolvedValue(response) as unknown as typeof fetch;

describe("market API client", () => {
  it("uses the configured public URL without a trailing slash", () => {
    expect(marketApiUrl("https://markets.example/")).toBe(
      "https://markets.example",
    );
  });

  it("defaults to the production market API", () => {
    expect(marketApiUrl()).toBe("https://api.raindex.finance");
  });

  it("loads the cached market overview", async () => {
    const request = fetcher(
      Response.json([{ market: { tickerId: "base_quote" } }]),
    );

    const markets = await getMarkets(request, "https://markets.example");

    expect(markets).toHaveLength(1);
    expect(request).toHaveBeenCalledWith("https://markets.example/v1/markets", {
      headers: { Accept: "application/json" },
      signal: undefined,
    });
  });

  it("encodes ticker IDs when loading executable detail", async () => {
    const request = fetcher(
      Response.json([{ market: { tickerId: "BASE / QUOTE" } }]),
    );

    await getMarket("BASE / QUOTE", request, "https://markets.example");

    expect(request).toHaveBeenCalledWith(
      "https://markets.example/v1/markets?ticker_id=BASE%20%2F%20QUOTE",
      expect.any(Object),
    );
  });

  it("forwards cancellation to the market service", async () => {
    const request = fetcher(Response.json([]));
    const controller = new AbortController();

    await getMarkets(request, "https://markets.example", controller.signal);

    expect(request).toHaveBeenCalledWith(
      "https://markets.example/v1/markets",
      expect.objectContaining({ signal: controller.signal }),
    );
  });

  it("preserves structured service errors and request IDs", async () => {
    const request = fetcher(
      Response.json(
        {
          request_id: "request-1",
          error: {
            code: "UPSTREAM_UNAVAILABLE",
            message: "market data is unavailable",
          },
        },
        { status: 503 },
      ),
    );

    const error = await getMarkets(request, "https://markets.example").catch((
      reason,
    ) => reason);

    expect(error).toBeInstanceOf(MarketApiError);
    expect(error).toMatchObject({
      message: "market data is unavailable",
      status: 503,
      requestId: "request-1",
    });
  });
});
