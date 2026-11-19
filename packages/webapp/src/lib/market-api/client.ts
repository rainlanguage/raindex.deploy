import { env } from "$env/dynamic/public";
import type { RaindexMarketSnapshot } from "@rainlanguage/raindex";

type MarketApiErrorBody = {
  request_id?: string;
  error?: {
    code?: string;
    message?: string;
  };
};

export const DEFAULT_MARKET_API_URL = "https://api.raindex.finance";

export class MarketApiError extends Error {
  constructor(
    message: string,
    public readonly status?: number,
    public readonly requestId?: string,
  ) {
    super(message);
    this.name = "MarketApiError";
  }
}

export function marketApiUrl(
  configured = env.PUBLIC_MARKET_DATA_API_URL,
): string {
  return (configured?.trim() || DEFAULT_MARKET_API_URL).replace(/\/$/, "");
}

async function readError(response: Response): Promise<MarketApiError> {
  const body = await response
    .clone()
    .json()
    .catch(() => undefined) as MarketApiErrorBody | undefined;
  return new MarketApiError(
    body?.error?.message ||
      `Market API request failed with status ${response.status}`,
    response.status,
    body?.request_id,
  );
}

async function getSnapshots(
  path: string,
  fetcher: typeof fetch = fetch,
  baseUrl = marketApiUrl(),
  signal?: AbortSignal,
): Promise<RaindexMarketSnapshot[]> {
  const response = await fetcher(`${baseUrl}${path}`, {
    headers: { Accept: "application/json" },
    signal,
  });
  if (!response.ok) throw await readError(response);
  const snapshots: unknown = await response.json();
  if (!Array.isArray(snapshots)) {
    throw new MarketApiError("Market API returned an invalid response");
  }
  return snapshots as RaindexMarketSnapshot[];
}

export function getMarkets(
  fetcher?: typeof fetch,
  baseUrl?: string,
  signal?: AbortSignal,
): Promise<RaindexMarketSnapshot[]> {
  return getSnapshots("/v1/markets", fetcher, baseUrl, signal);
}

export async function getMarket(
  tickerId: string,
  fetcher?: typeof fetch,
  baseUrl?: string,
  signal?: AbortSignal,
): Promise<RaindexMarketSnapshot> {
  const snapshots = await getSnapshots(
    `/v1/markets?ticker_id=${encodeURIComponent(tickerId)}`,
    fetcher,
    baseUrl,
    signal,
  );
  const market = snapshots[0];
  if (!market) throw new MarketApiError(`Unknown market: ${tickerId}`, 404);
  return market;
}
