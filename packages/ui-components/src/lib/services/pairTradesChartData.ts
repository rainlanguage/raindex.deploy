import type { RaindexTrade, RaindexVaultToken } from "@rainlanguage/raindex";
import type { UTCTimestamp } from "lightweight-charts";
import { sortBy } from "lodash";
import {
  TIME_DELTA_24_HOURS,
  TIME_DELTA_7_DAYS,
  TIME_DELTA_30_DAYS,
} from "./time";

export const BUCKET_SECONDS_24_HOURS = 900;
export const BUCKET_SECONDS_7_DAYS = 3600;
export const BUCKET_SECONDS_30_DAYS = 14400;
export const BUCKET_SECONDS_1_YEAR = 86400;

export const CHART_COLORS = {
  BUY_VOLUME: "#26a69a",
  BUY_VOLUME_TRANSPARENT: "rgba(38, 166, 154, 0.5)",
  SELL_VOLUME: "#ef5350",
  SELL_VOLUME_TRANSPARENT: "rgba(239, 83, 80, 0.5)",
  PRICE_LINE: "#5c6bc0",
  ZERO_LINE: "#888888",
} as const;

export function getBucketSecondsForTimeDelta(timeDeltaSeconds: number): number {
  if (timeDeltaSeconds <= TIME_DELTA_24_HOURS) return BUCKET_SECONDS_24_HOURS;
  if (timeDeltaSeconds <= TIME_DELTA_7_DAYS) return BUCKET_SECONDS_7_DAYS;
  if (timeDeltaSeconds <= TIME_DELTA_30_DAYS) return BUCKET_SECONDS_30_DAYS;
  return BUCKET_SECONDS_1_YEAR;
}

export type TradingPair = {
  baseToken: RaindexVaultToken;
  quoteToken: RaindexVaultToken;
};

export type PricePoint = {
  time: UTCTimestamp;
  value: number;
};

export type VolumePoint = {
  time: UTCTimestamp;
  value: number;
  color?: string;
};

export type PairTradesChartData = {
  pricePoints: PricePoint[];
  buyVolumePoints: VolumePoint[];
  sellVolumePoints: VolumePoint[];
};

type TradeAmounts = {
  baseAmount: number;
  quoteAmount: number;
  isBuy: boolean;
};

function getTokenAddressLower(
  trade: RaindexTrade,
  which: "in" | "out",
): string {
  const change =
    which === "in"
      ? trade.inputVaultBalanceChange
      : trade.outputVaultBalanceChange;
  return change.token.address.toLowerCase();
}

function tradeMatchesPair(
  trade: RaindexTrade,
  baseTokenAddress: string,
  quoteTokenAddress: string,
): boolean {
  const inAddr = getTokenAddressLower(trade, "in");
  const outAddr = getTokenAddressLower(trade, "out");
  const baseLower = baseTokenAddress.toLowerCase();
  const quoteLower = quoteTokenAddress.toLowerCase();

  return (
    (inAddr === baseLower && outAddr === quoteLower) ||
    (inAddr === quoteLower && outAddr === baseLower)
  );
}

function getTradeAmounts(
  trade: RaindexTrade,
  baseTokenAddress: string,
): TradeAmounts | null {
  const inAddr = getTokenAddressLower(trade, "in");
  const baseLower = baseTokenAddress.toLowerCase();

  const inFormattedAmount = trade.inputVaultBalanceChange.formattedAmount;
  const outFormattedAmount = trade.outputVaultBalanceChange.formattedAmount;

  const inAmt = Math.abs(parseFloat(inFormattedAmount));
  const outAmt = Math.abs(parseFloat(outFormattedAmount));

  if (!Number.isFinite(inAmt) || !Number.isFinite(outAmt)) return null;

  const isBuy = inAddr === baseLower;

  return {
    baseAmount: isBuy ? inAmt : outAmt,
    quoteAmount: isBuy ? outAmt : inAmt,
    isBuy,
  };
}

export function extractPairsFromTrades(trades: RaindexTrade[]): TradingPair[] {
  const pairMap = new Map<string, TradingPair>();

  for (const trade of trades) {
    const inToken = trade.inputVaultBalanceChange.token;
    const outToken = trade.outputVaultBalanceChange.token;

    const inAddr = inToken.address.toLowerCase();
    const outAddr = outToken.address.toLowerCase();

    const [firstAddr, secondAddr] =
      inAddr < outAddr ? [inAddr, outAddr] : [outAddr, inAddr];
    const pairKey = `${firstAddr}-${secondAddr}`;

    if (!pairMap.has(pairKey)) {
      const [baseToken, quoteToken] =
        inAddr < outAddr ? [inToken, outToken] : [outToken, inToken];
      pairMap.set(pairKey, { baseToken, quoteToken });
    }
  }

  return Array.from(pairMap.values());
}

export function getDefaultPair(trades: RaindexTrade[]): TradingPair | null {
  if (trades.length === 0) return null;

  const sortedTrades = sortBy(trades, (t) => Number(t.timestamp));
  const oldestTrade = sortedTrades[0];
  const inToken = oldestTrade.inputVaultBalanceChange.token;
  const outToken = oldestTrade.outputVaultBalanceChange.token;

  const inAddr = inToken.address.toLowerCase();
  const outAddr = outToken.address.toLowerCase();

  if (inAddr < outAddr) {
    return { baseToken: inToken, quoteToken: outToken };
  } else {
    return { baseToken: outToken, quoteToken: inToken };
  }
}

export function getTokenLabel(token: RaindexVaultToken): string {
  if (token.symbol && token.symbol.trim() !== "") {
    return token.symbol;
  }
  const addr = token.address;
  if (addr.length >= 10) {
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
  }
  return addr;
}

export function pairsAreEqual(pairA: TradingPair, pairB: TradingPair): boolean {
  return (
    pairA.baseToken.address.toLowerCase() ===
      pairB.baseToken.address.toLowerCase() &&
    pairA.quoteToken.address.toLowerCase() ===
      pairB.quoteToken.address.toLowerCase()
  );
}

export function findPairIndex(
  pairs: TradingPair[],
  targetPair: TradingPair,
): number {
  return pairs.findIndex((p) => pairsAreEqual(p, targetPair));
}

export function flipTradingPair(pair: TradingPair): TradingPair {
  return {
    baseToken: pair.quoteToken,
    quoteToken: pair.baseToken,
  };
}

/**
 * Formats a chart axis tick.
 *
 * @param useLocalTime - when true, use the browser timezone; otherwise UTC
 */
export function formatChartTimestamp(
  timestampSeconds: number,
  timeDeltaSeconds: number,
  useLocalTime = false,
): string {
  const date = new Date(timestampSeconds * 1000);
  const day = useLocalTime ? date.getDate() : date.getUTCDate();
  const month = useLocalTime
    ? date.toLocaleString("en-US", { month: "short" })
    : date.toLocaleString("en-US", { month: "short", timeZone: "UTC" });
  const hours = (useLocalTime ? date.getHours() : date.getUTCHours())
    .toString()
    .padStart(2, "0");
  const minutes = (useLocalTime ? date.getMinutes() : date.getUTCMinutes())
    .toString()
    .padStart(2, "0");

  if (timeDeltaSeconds <= TIME_DELTA_24_HOURS) {
    return `${month} ${day} ${hours}:${minutes}`;
  } else if (timeDeltaSeconds <= TIME_DELTA_7_DAYS) {
    return `${month} ${day} ${hours}:00`;
  } else {
    return `${month} ${day}`;
  }
}

export type TransformPairTradesInput = {
  trades: RaindexTrade[];
  baseTokenAddress: string;
  quoteTokenAddress: string;
  timeDeltaSeconds: number;
};

export function transformPairTrades(
  input: TransformPairTradesInput,
): PairTradesChartData {
  const { trades, baseTokenAddress, quoteTokenAddress, timeDeltaSeconds } =
    input;
  const bucketSeconds = getBucketSecondsForTimeDelta(timeDeltaSeconds);

  const now = Math.floor(Date.now() / 1000);
  const cutoff = now - timeDeltaSeconds;

  const filteredTrades = trades.filter((trade) => {
    const ts = Number(trade.timestamp);
    if (ts < cutoff || ts > now) return false;
    return tradeMatchesPair(trade, baseTokenAddress, quoteTokenAddress);
  });

  const priceAggregator = new Map<
    number,
    { sumBase: number; sumQuote: number }
  >();
  const buyVolumeBuckets = new Map<number, number>();
  const sellVolumeBuckets = new Map<number, number>();

  for (const trade of filteredTrades) {
    const amounts = getTradeAmounts(trade, baseTokenAddress);
    if (!amounts) continue;

    const { baseAmount, quoteAmount, isBuy } = amounts;

    if (baseAmount === 0) continue;

    const ts = Number(trade.timestamp);

    const existing = priceAggregator.get(ts);
    if (existing) {
      priceAggregator.set(ts, {
        sumBase: existing.sumBase + baseAmount,
        sumQuote: existing.sumQuote + quoteAmount,
      });
    } else {
      priceAggregator.set(ts, { sumBase: baseAmount, sumQuote: quoteAmount });
    }

    const bucketTime = Math.floor(ts / bucketSeconds) * bucketSeconds;
    const volumeMap = isBuy ? buyVolumeBuckets : sellVolumeBuckets;
    const existingVol = volumeMap.get(bucketTime) ?? 0;
    volumeMap.set(bucketTime, existingVol + baseAmount);
  }

  const pricePoints: PricePoint[] = [];
  for (const [ts, agg] of priceAggregator.entries()) {
    if (agg.sumBase === 0) continue;
    const price = agg.sumQuote / agg.sumBase;
    if (!Number.isFinite(price)) continue;
    pricePoints.push({ time: ts as UTCTimestamp, value: price });
  }

  const buyVolumePoints: VolumePoint[] = [];
  for (const [bucketTime, vol] of buyVolumeBuckets.entries()) {
    buyVolumePoints.push({
      time: bucketTime as UTCTimestamp,
      value: vol,
      color: CHART_COLORS.BUY_VOLUME,
    });
  }

  const sellVolumePoints: VolumePoint[] = [];
  for (const [bucketTime, vol] of sellVolumeBuckets.entries()) {
    sellVolumePoints.push({
      time: bucketTime as UTCTimestamp,
      value: -vol,
      color: CHART_COLORS.SELL_VOLUME,
    });
  }

  return {
    pricePoints: sortBy(pricePoints, (p) => p.time),
    buyVolumePoints: sortBy(buyVolumePoints, (p) => p.time),
    sellVolumePoints: sortBy(sellVolumePoints, (p) => p.time),
  };
}
