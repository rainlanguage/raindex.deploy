import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { RaindexTrade, RaindexVaultToken } from "@rainlanguage/raindex";
import {
  extractPairsFromTrades,
  getDefaultPair,
  getTokenLabel,
  transformPairTrades,
  getBucketSecondsForTimeDelta,
  pairsAreEqual,
  findPairIndex,
  flipTradingPair,
  formatChartTimestamp,
  BUCKET_SECONDS_24_HOURS,
  BUCKET_SECONDS_7_DAYS,
  BUCKET_SECONDS_30_DAYS,
  BUCKET_SECONDS_1_YEAR,
} from "../lib/services/pairTradesChartData";
import {
  TIME_DELTA_24_HOURS,
  TIME_DELTA_7_DAYS,
  TIME_DELTA_30_DAYS,
  TIME_DELTA_1_YEAR,
} from "../lib/services/time";

function createMockToken(
  address: string,
  symbol?: string,
  name?: string,
): RaindexVaultToken {
  return {
    id: address,
    address,
    symbol: symbol ?? "",
    name: name ?? "",
    decimals: BigInt(18),
    chainId: 1,
    free: false,
  } as unknown as RaindexVaultToken;
}

function createMockTrade(
  id: string,
  timestamp: number,
  inTokenAddress: string,
  outTokenAddress: string,
  inAmount: number,
  outAmount: number,
  inSymbol?: string,
  outSymbol?: string,
): RaindexTrade {
  return {
    id,
    timestamp: BigInt(timestamp),
    inputVaultBalanceChange: {
      token: createMockToken(inTokenAddress, inSymbol),
      amount: BigInt(inAmount),
      formattedAmount: String(inAmount),
      vaultId: BigInt(1),
    },
    outputVaultBalanceChange: {
      token: createMockToken(outTokenAddress, outSymbol),
      amount: BigInt(-outAmount),
      formattedAmount: String(-outAmount),
      vaultId: BigInt(2),
    },
    transaction: {
      id: "tx-" + id,
      from: "0x1234567890abcdef1234567890abcdef12345678",
      timestamp: BigInt(timestamp),
      blockNumber: BigInt(12345),
    },
    orderHash: "0xorderhash",
    raindex: "0xraindex",
  } as unknown as RaindexTrade;
}

describe("getBucketSecondsForTimeDelta", () => {
  it("returns correct bucket size for 24h", () => {
    expect(getBucketSecondsForTimeDelta(TIME_DELTA_24_HOURS)).toBe(
      BUCKET_SECONDS_24_HOURS,
    );
  });

  it("returns correct bucket size for 7d", () => {
    expect(getBucketSecondsForTimeDelta(TIME_DELTA_7_DAYS)).toBe(
      BUCKET_SECONDS_7_DAYS,
    );
  });

  it("returns correct bucket size for 30d", () => {
    expect(getBucketSecondsForTimeDelta(TIME_DELTA_30_DAYS)).toBe(
      BUCKET_SECONDS_30_DAYS,
    );
  });

  it("returns correct bucket size for 1y", () => {
    expect(getBucketSecondsForTimeDelta(TIME_DELTA_1_YEAR)).toBe(
      BUCKET_SECONDS_1_YEAR,
    );
  });
});

describe("extractPairsFromTrades", () => {
  it("returns empty array for no trades", () => {
    const result = extractPairsFromTrades([]);
    expect(result).toEqual([]);
  });

  it("extracts single pair from trades", () => {
    const trades = [
      createMockTrade("1", 1000, "0xAAA", "0xBBB", 100, 200, "AAA", "BBB"),
      createMockTrade("2", 2000, "0xAAA", "0xBBB", 50, 100, "AAA", "BBB"),
    ];

    const result = extractPairsFromTrades(trades);
    expect(result).toHaveLength(1);
    expect(result[0].baseToken.address.toLowerCase()).toBe("0xaaa");
    expect(result[0].quoteToken.address.toLowerCase()).toBe("0xbbb");
  });

  it("extracts multiple pairs from trades", () => {
    const trades = [
      createMockTrade("1", 1000, "0xAAA", "0xBBB", 100, 200),
      createMockTrade("2", 2000, "0xCCC", "0xDDD", 50, 100),
    ];

    const result = extractPairsFromTrades(trades);
    expect(result).toHaveLength(2);
  });

  it("handles reversed pair order (same pair different direction)", () => {
    const trades = [
      createMockTrade("1", 1000, "0xAAA", "0xBBB", 100, 200),
      createMockTrade("2", 2000, "0xBBB", "0xAAA", 50, 100),
    ];

    const result = extractPairsFromTrades(trades);
    expect(result).toHaveLength(1);
  });
});

describe("getDefaultPair", () => {
  it("returns null for empty trades", () => {
    expect(getDefaultPair([])).toBeNull();
  });

  it("returns pair from oldest trade", () => {
    const trades = [
      createMockTrade("1", 2000, "0xAAA", "0xBBB", 100, 200, "AAA", "BBB"),
      createMockTrade("2", 1000, "0xCCC", "0xDDD", 50, 100, "CCC", "DDD"),
    ];

    const result = getDefaultPair(trades);
    expect(result).not.toBeNull();
    expect(result!.baseToken.address.toLowerCase()).toBe("0xccc");
    expect(result!.quoteToken.address.toLowerCase()).toBe("0xddd");
  });
});

describe("getTokenLabel", () => {
  it("returns symbol when available", () => {
    const token = createMockToken(
      "0x1234567890123456789012345678901234567890",
      "ETH",
    );
    expect(getTokenLabel(token)).toBe("ETH");
  });

  it("returns short address when no symbol", () => {
    const token = createMockToken(
      "0x1234567890123456789012345678901234567890",
      "",
    );
    expect(getTokenLabel(token)).toBe("0x1234...7890");
  });

  it("returns full address if too short", () => {
    const token = createMockToken("0x1234", "");
    expect(getTokenLabel(token)).toBe("0x1234");
  });
});

describe("transformPairTrades", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(1700000000 * 1000));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns empty data when no trades match the pair", () => {
    const trades = [
      createMockTrade("1", 1699990000, "0xAAA", "0xBBB", 100, 200),
    ];

    const result = transformPairTrades({
      trades,
      baseTokenAddress: "0xCCC",
      quoteTokenAddress: "0xDDD",
      timeDeltaSeconds: TIME_DELTA_24_HOURS,
    });

    expect(result.pricePoints).toHaveLength(0);
    expect(result.buyVolumePoints).toHaveLength(0);
    expect(result.sellVolumePoints).toHaveLength(0);
  });

  it("filters trades outside time window", () => {
    const trades = [
      createMockTrade("1", 1699990000, "0xAAA", "0xBBB", 100, 200),
      createMockTrade("2", 1600000000, "0xAAA", "0xBBB", 50, 100),
    ];

    const result = transformPairTrades({
      trades,
      baseTokenAddress: "0xAAA",
      quoteTokenAddress: "0xBBB",
      timeDeltaSeconds: TIME_DELTA_24_HOURS,
    });

    expect(result.pricePoints).toHaveLength(1);
  });

  it("correctly identifies BUY trades (order received base)", () => {
    const trades = [
      createMockTrade("1", 1699990000, "0xAAA", "0xBBB", 100, 200),
    ];

    const result = transformPairTrades({
      trades,
      baseTokenAddress: "0xAAA",
      quoteTokenAddress: "0xBBB",
      timeDeltaSeconds: TIME_DELTA_24_HOURS,
    });

    expect(result.buyVolumePoints).toHaveLength(1);
    expect(result.sellVolumePoints).toHaveLength(0);
  });

  it("correctly identifies SELL trades (order gave base)", () => {
    const trades = [
      createMockTrade("1", 1699990000, "0xBBB", "0xAAA", 200, 100),
    ];

    const result = transformPairTrades({
      trades,
      baseTokenAddress: "0xAAA",
      quoteTokenAddress: "0xBBB",
      timeDeltaSeconds: TIME_DELTA_24_HOURS,
    });

    expect(result.buyVolumePoints).toHaveLength(0);
    expect(result.sellVolumePoints).toHaveLength(1);
  });

  it("calculates price as quote/base", () => {
    const trades = [
      createMockTrade("1", 1699990000, "0xAAA", "0xBBB", 100, 200),
    ];

    const result = transformPairTrades({
      trades,
      baseTokenAddress: "0xAAA",
      quoteTokenAddress: "0xBBB",
      timeDeltaSeconds: TIME_DELTA_24_HOURS,
    });

    expect(result.pricePoints).toHaveLength(1);
    expect(result.pricePoints[0].value).toBe(2);
  });

  it("merges trades with same timestamp", () => {
    const trades = [
      createMockTrade("1", 1699990000, "0xAAA", "0xBBB", 100, 200),
      createMockTrade("2", 1699990000, "0xAAA", "0xBBB", 100, 200),
    ];

    const result = transformPairTrades({
      trades,
      baseTokenAddress: "0xAAA",
      quoteTokenAddress: "0xBBB",
      timeDeltaSeconds: TIME_DELTA_24_HOURS,
    });

    expect(result.pricePoints).toHaveLength(1);
  });

  it("aggregates volume into buckets", () => {
    const bucketSeconds = BUCKET_SECONDS_24_HOURS;
    const bucketStart = Math.floor(1699990000 / bucketSeconds) * bucketSeconds;

    const trades = [
      createMockTrade("1", bucketStart + 100, "0xAAA", "0xBBB", 100, 200),
      createMockTrade("2", bucketStart + 200, "0xAAA", "0xBBB", 50, 100),
    ];

    const result = transformPairTrades({
      trades,
      baseTokenAddress: "0xAAA",
      quoteTokenAddress: "0xBBB",
      timeDeltaSeconds: TIME_DELTA_24_HOURS,
    });

    expect(result.buyVolumePoints).toHaveLength(1);
    expect(result.buyVolumePoints[0].value).toBe(150);
  });

  it("sell volume is negative (bars go down)", () => {
    const trades = [
      createMockTrade("1", 1699990000, "0xBBB", "0xAAA", 200, 100),
    ];

    const result = transformPairTrades({
      trades,
      baseTokenAddress: "0xAAA",
      quoteTokenAddress: "0xBBB",
      timeDeltaSeconds: TIME_DELTA_24_HOURS,
    });

    expect(result.sellVolumePoints).toHaveLength(1);
    expect(result.sellVolumePoints[0].value).toBeLessThan(0);
  });

  it("sorts price points by time", () => {
    const trades = [
      createMockTrade("1", 1699995000, "0xAAA", "0xBBB", 100, 200),
      createMockTrade("2", 1699990000, "0xAAA", "0xBBB", 50, 100),
    ];

    const result = transformPairTrades({
      trades,
      baseTokenAddress: "0xAAA",
      quoteTokenAddress: "0xBBB",
      timeDeltaSeconds: TIME_DELTA_24_HOURS,
    });

    expect(result.pricePoints[0].time).toBeLessThan(result.pricePoints[1].time);
  });
});

describe("pairsAreEqual", () => {
  it("returns true for identical pairs", () => {
    const pair = {
      baseToken: createMockToken("0xAAA"),
      quoteToken: createMockToken("0xBBB"),
    };
    expect(pairsAreEqual(pair, pair)).toBe(true);
  });

  it("returns true for case-insensitive address match", () => {
    const pair1 = {
      baseToken: createMockToken("0xAAA"),
      quoteToken: createMockToken("0xBBB"),
    };
    const pair2 = {
      baseToken: createMockToken("0xaaa"),
      quoteToken: createMockToken("0xbbb"),
    };
    expect(pairsAreEqual(pair1, pair2)).toBe(true);
  });

  it("returns false for different pairs", () => {
    const pair1 = {
      baseToken: createMockToken("0xAAA"),
      quoteToken: createMockToken("0xBBB"),
    };
    const pair2 = {
      baseToken: createMockToken("0xCCC"),
      quoteToken: createMockToken("0xDDD"),
    };
    expect(pairsAreEqual(pair1, pair2)).toBe(false);
  });

  it("returns false for swapped base/quote (order matters)", () => {
    const pair1 = {
      baseToken: createMockToken("0xAAA"),
      quoteToken: createMockToken("0xBBB"),
    };
    const pair2 = {
      baseToken: createMockToken("0xBBB"),
      quoteToken: createMockToken("0xAAA"),
    };
    expect(pairsAreEqual(pair1, pair2)).toBe(false);
  });
});

describe("findPairIndex", () => {
  it("returns index when pair exists", () => {
    const pairs = [
      {
        baseToken: createMockToken("0xAAA"),
        quoteToken: createMockToken("0xBBB"),
      },
      {
        baseToken: createMockToken("0xCCC"),
        quoteToken: createMockToken("0xDDD"),
      },
    ];
    expect(findPairIndex(pairs, pairs[1])).toBe(1);
  });

  it("returns -1 when pair not found", () => {
    const pairs = [
      {
        baseToken: createMockToken("0xAAA"),
        quoteToken: createMockToken("0xBBB"),
      },
    ];
    const target = {
      baseToken: createMockToken("0xCCC"),
      quoteToken: createMockToken("0xDDD"),
    };
    expect(findPairIndex(pairs, target)).toBe(-1);
  });

  it("finds pair with case-insensitive match", () => {
    const pairs = [
      {
        baseToken: createMockToken("0xAAA"),
        quoteToken: createMockToken("0xBBB"),
      },
    ];
    const target = {
      baseToken: createMockToken("0xaaa"),
      quoteToken: createMockToken("0xbbb"),
    };
    expect(findPairIndex(pairs, target)).toBe(0);
  });
});

describe("flipTradingPair", () => {
  it("swaps base and quote tokens", () => {
    const base = createMockToken("0xAAA", "AAA");
    const quote = createMockToken("0xBBB", "BBB");
    const result = flipTradingPair({ baseToken: base, quoteToken: quote });
    expect(result.baseToken.address).toBe("0xBBB");
    expect(result.quoteToken.address).toBe("0xAAA");
  });

  it("double flip returns equivalent pair", () => {
    const pair = {
      baseToken: createMockToken("0xAAA"),
      quoteToken: createMockToken("0xBBB"),
    };
    const flippedTwice = flipTradingPair(flipTradingPair(pair));
    expect(pairsAreEqual(pair, flippedTwice)).toBe(true);
  });

  it("does not mutate original pair", () => {
    const base = createMockToken("0xAAA");
    const quote = createMockToken("0xBBB");
    const original = { baseToken: base, quoteToken: quote };
    flipTradingPair(original);
    expect(original.baseToken.address).toBe("0xAAA");
    expect(original.quoteToken.address).toBe("0xBBB");
  });
});

describe("formatChartTimestamp", () => {
  it("formats with minutes for 24h delta in UTC by default", () => {
    const ts = Date.UTC(2024, 0, 15, 14, 30) / 1000;
    const result = formatChartTimestamp(ts, TIME_DELTA_24_HOURS);
    expect(result).toBe("Jan 15 14:30");
  });

  it("formats with hour precision for 7d delta in UTC by default", () => {
    const ts = Date.UTC(2024, 0, 15, 14, 30) / 1000;
    const result = formatChartTimestamp(ts, TIME_DELTA_7_DAYS);
    expect(result).toBe("Jan 15 14:00");
  });

  it("formats with day only for 30d delta in UTC by default", () => {
    const ts = Date.UTC(2024, 0, 15, 14, 30) / 1000;
    const result = formatChartTimestamp(ts, TIME_DELTA_30_DAYS);
    expect(result).toBe("Jan 15");
  });

  it("formats with day only for 1y delta in UTC by default", () => {
    const ts = Date.UTC(2024, 0, 15, 14, 30) / 1000;
    const result = formatChartTimestamp(ts, TIME_DELTA_1_YEAR);
    expect(result).toBe("Jan 15");
  });

  it("pads single-digit hours and minutes in UTC by default", () => {
    const ts = Date.UTC(2024, 0, 5, 9, 5) / 1000;
    const result = formatChartTimestamp(ts, TIME_DELTA_24_HOURS);
    expect(result).toBe("Jan 5 09:05");
  });

  it("formats in local time when useLocalTime is true", () => {
    const ts = new Date(2024, 0, 15, 14, 30).getTime() / 1000;
    const result = formatChartTimestamp(ts, TIME_DELTA_24_HOURS, true);
    expect(result).toBe("Jan 15 14:30");
  });
});
