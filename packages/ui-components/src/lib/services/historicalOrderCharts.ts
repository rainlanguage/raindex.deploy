import type {
  RaindexTrade,
  RaindexTransaction,
  RaindexVaultBalanceChange,
  RaindexVaultToken,
} from "@rainlanguage/raindex";
import type { UTCTimestamp } from "lightweight-charts";
import { timestampSecondsToUTCTimestamp } from "../services/time";
import { sortBy } from "lodash";

export type HistoricalOrderChartData = {
  value: number;
  time: UTCTimestamp;
  color?: string;
}[];

export function prepareHistoricalOrderChartData(
  takeOrderEntities: RaindexTrade[],
  colorTheme: string,
) {
  const transformedData = takeOrderEntities.map((d) => ({
    value: Math.abs(
      Number(d.inputVaultBalanceChange.formattedAmount) /
        Number(d.outputVaultBalanceChange.formattedAmount),
    ),
    time: timestampSecondsToUTCTimestamp(BigInt(d.timestamp)),
    color: colorTheme == "dark" ? "#5178FF" : "#4E4AF6",
    outputAmount: Number(d.outputVaultBalanceChange.amount),
  }));

  // if we have multiple object in the array with the same timestamp, we need to merge them
  // we do this by taking the weighted average of the ioratio values for objects that share the same timestamp.
  const uniqueTimestamps = Array.from(
    new Set(transformedData.map((d) => d.time)),
  );
  const finalData: HistoricalOrderChartData = [];
  uniqueTimestamps.forEach((timestamp) => {
    const objectsWithSameTimestamp = transformedData.filter(
      (d) => d.time === timestamp,
    );
    if (objectsWithSameTimestamp.length > 1) {
      // calculate a weighted average of the ioratio values using the amount of the output token as the weight
      const ioratioSum = objectsWithSameTimestamp.reduce(
        (acc, d) => acc + d.value * d.outputAmount,
        0,
      );
      const outputAmountSum = objectsWithSameTimestamp.reduce(
        (acc, d) => acc + d.outputAmount,
        0,
      );
      // A zero output sum means every merged trade had zero output amount,
      // so there is no meaningful weighted ratio for the bucket; chart it
      // as zero rather than dividing by zero.
      const ioratioAverage =
        outputAmountSum === 0 ? 0 : ioratioSum / outputAmountSum;
      finalData.push({
        value: ioratioAverage,
        time: timestamp,
        color: objectsWithSameTimestamp[0].color,
      });
    } else {
      finalData.push(objectsWithSameTimestamp[0]);
    }
  });

  return sortBy(finalData, (d) => d.time);
}

if (import.meta.vitest) {
  const { it, expect } = import.meta.vitest;

  it("transforms and sorts data as expected", () => {
    const takeOrderEntities: RaindexTrade[] = [
      {
        id: "1",
        timestamp: BigInt(1632000000),
        transaction: {
          id: "transaction_id",
          from: "0xsender_address",
          timestamp: BigInt(1632000000),
          blockNumber: BigInt(0),
        } as unknown as RaindexTransaction,
        outputVaultBalanceChange: {
          amount: BigInt(100),
          formattedAmount: "100",
          vaultId: BigInt(1),
          __typename: "Withdraw",
          token: {
            id: "output_token",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "sender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        orderHash: "orderHash",
        inputVaultBalanceChange: {
          vaultId: BigInt(1),
          token: {
            id: "output_token",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          amount: BigInt(50),
          formattedAmount: "50",
          __typename: "Withdraw",
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "0xsender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        raindex: "0x00",
      } as unknown as RaindexTrade,
      {
        id: "2",
        timestamp: BigInt(1631000000),
        transaction: {
          id: "transaction_id",
          from: "0xsender_address",
          timestamp: BigInt(1631000000),
          blockNumber: BigInt(0),
        } as unknown as RaindexTransaction,
        outputVaultBalanceChange: {
          amount: BigInt(100),
          formattedAmount: "100",
          vaultId: BigInt(1),
          token: {
            id: "output_token",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          __typename: "Withdraw",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "0xsender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        orderHash: "orderHash",
        inputVaultBalanceChange: {
          vaultId: BigInt(1),
          token: {
            id: "output_token",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          amount: BigInt(50),
          formattedAmount: "50",
          __typename: "Withdraw",
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "0xsender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        raindex: "0x00",
      } as unknown as RaindexTrade,
      {
        id: "3",
        timestamp: BigInt(1630000000),
        transaction: {
          id: "transaction_id",
          from: "0xsender_address",
          timestamp: BigInt(1630000000),
          blockNumber: BigInt(0),
        } as unknown as RaindexTransaction,
        outputVaultBalanceChange: {
          amount: BigInt(100),
          formattedAmount: "100",
          vaultId: BigInt(1),
          token: {
            id: "output_token",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          __typename: "Withdraw",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "sender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        orderHash: "orderHash",
        inputVaultBalanceChange: {
          vaultId: BigInt(1),
          token: {
            id: "output_token",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          amount: BigInt(50),
          formattedAmount: "50",
          __typename: "Withdraw",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "sender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        raindex: "0x00",
      } as unknown as RaindexTrade,
    ];

    const result = prepareHistoricalOrderChartData(takeOrderEntities, "dark");

    expect(result.length).toEqual(3);
    expect(result[0].value).toEqual(0.5);
    expect(result[0].time).toEqual(1630000000);
    expect(result[1].value).toEqual(0.5);
    expect(result[1].time).toEqual(1631000000);
    expect(result[2].value).toEqual(0.5);
    expect(result[2].time).toEqual(1632000000);

    // check the color
    expect(result[0].color).toEqual("#5178FF");
    expect(result[1].color).toEqual("#5178FF");
    expect(result[2].color).toEqual("#5178FF");
  });

  it("handles the case where multiple trades have the same timestamp", () => {
    const takeOrderEntities: RaindexTrade[] = [
      {
        id: "1",
        timestamp: BigInt(1632000000),
        transaction: {
          id: "transaction_id",
          from: "0xsender_address",
          timestamp: BigInt(1632000000),
          blockNumber: BigInt(0),
        } as unknown as RaindexTransaction,
        outputVaultBalanceChange: {
          amount: BigInt(100),
          formattedAmount: "100",
          vaultId: BigInt(1),
          token: {
            id: "output_token",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "0xsender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          __typename: "Withdraw",
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        orderHash: "orderHash",
        inputVaultBalanceChange: {
          vaultId: BigInt(1),
          token: {
            id: "output_token",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          amount: BigInt(50),
          formattedAmount: "50",
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "0xsender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        raindex: "0x00",
      } as unknown as RaindexTrade,
      {
        id: "2",
        timestamp: BigInt(1632000000),
        transaction: {
          id: "transaction_id",
          from: "0xsender_address",
          timestamp: BigInt(1632000000),
          blockNumber: BigInt(0),
        } as unknown as RaindexTransaction,
        outputVaultBalanceChange: {
          amount: BigInt(200),
          formattedAmount: "200",
          vaultId: BigInt(1),
          token: {
            id: "output_token",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "0xsender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        orderHash: "orderHash",
        inputVaultBalanceChange: {
          vaultId: BigInt(1),
          token: {
            id: "output_token",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          amount: BigInt(50),
          formattedAmount: "50",
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "0xsender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        raindex: "0x00",
      } as unknown as RaindexTrade,
      {
        id: "3",
        timestamp: BigInt(1632000000),
        transaction: {
          id: "transaction_id",
          from: "0xsender_address",
          timestamp: BigInt(1632000000),
          blockNumber: BigInt(0),
        } as unknown as RaindexTransaction,
        outputVaultBalanceChange: {
          amount: BigInt(400),
          formattedAmount: "400",
          vaultId: BigInt(1),
          token: {
            id: "output_token",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "0xsender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        orderHash: "orderHash",
        inputVaultBalanceChange: {
          vaultId: BigInt(1),
          token: {
            id: "output_token_id",
            address: "0xoutput_token",
            name: "output_token",
            symbol: "output_token",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          amount: BigInt(50),
          formattedAmount: "50",
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          timestamp: BigInt(0),
          transaction: {
            id: "transaction_id",
            from: "0xsender_address",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        raindex: "0x00",
      } as unknown as RaindexTrade,
    ];

    const result = prepareHistoricalOrderChartData(takeOrderEntities, "dark");

    // calculate the weighted average of the ioratio values
    const ioratioSum = 0.5 * 100 + 0.25 * 200 + 0.125 * 400;
    const outputAmountSum = 100 + 200 + 400;
    const ioratioAverage = ioratioSum / outputAmountSum;

    expect(result.length).toEqual(1);
    expect(result[0].value).toEqual(ioratioAverage);
  });

  it("charts zero when same-timestamp outputAmounts sum to zero", () => {
    // outputVaultBalanceChange.amount (BigInt) = 0 for both trades → outputAmountSum = 0,
    // so the bucket has no meaningful weighted ratio and its value is 0.
    const makeZeroAmountTrade = (
      id: `0x${string}`,
      inputFormattedAmount: string,
    ) =>
      ({
        id,
        timestamp: BigInt(1632000000),
        transaction: {
          id: "tx",
          from: "0xsender",
          timestamp: BigInt(1632000000),
          blockNumber: BigInt(0),
        } as unknown as RaindexTransaction,
        outputVaultBalanceChange: {
          amount: BigInt(0),
          formattedAmount: "0",
          vaultId: BigInt(1),
          __typename: "Withdraw",
          token: {
            id: "tok",
            address: "0xtok",
            name: "tok",
            symbol: "tok",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          timestamp: BigInt(0),
          transaction: {
            id: "tx",
            from: "0xsender",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        orderHash: "0xorderHash" as `0x${string}`,
        inputVaultBalanceChange: {
          vaultId: BigInt(1),
          token: {
            id: "tok",
            address: "0xtok",
            name: "tok",
            symbol: "tok",
            decimals: BigInt(1),
          } as unknown as RaindexVaultToken,
          amount: BigInt(0),
          formattedAmount: inputFormattedAmount,
          __typename: "Withdraw",
          newBalance: BigInt(0),
          formattedNewBalance: "0",
          oldBalance: BigInt(0),
          formattedOldBalance: "0",
          timestamp: BigInt(0),
          transaction: {
            id: "tx",
            from: "0xsender",
            timestamp: BigInt(1632000000),
            blockNumber: BigInt(0),
          } as unknown as RaindexTransaction,
          raindex: "0x1",
        } as unknown as RaindexVaultBalanceChange,
        raindex: "0x00",
      }) as unknown as RaindexTrade;

    const takeOrderEntities: RaindexTrade[] = [
      makeZeroAmountTrade("0x1", "200"),
      makeZeroAmountTrade("0x2", "400"),
    ];

    const result = prepareHistoricalOrderChartData(takeOrderEntities, "dark");
    expect(result.length).toEqual(1);
    expect(result[0].value).toEqual(0);
  });

  // Minimal trade builder: prepareHistoricalOrderChartData only reads
  // timestamp, the two formattedAmount values and the output amount.
  const makeTrade = (args: {
    timestamp: number;
    inputFormatted: string;
    outputFormatted: string;
    outputAmount: bigint;
  }): RaindexTrade =>
    ({
      timestamp: BigInt(args.timestamp),
      inputVaultBalanceChange: {
        formattedAmount: args.inputFormatted,
      } as unknown as RaindexVaultBalanceChange,
      outputVaultBalanceChange: {
        formattedAmount: args.outputFormatted,
        amount: args.outputAmount,
      } as unknown as RaindexVaultBalanceChange,
    }) as unknown as RaindexTrade;

  it("uses the light theme colour when colorTheme is not 'dark'", () => {
    const trades = [
      makeTrade({
        timestamp: 1632000000,
        inputFormatted: "50",
        outputFormatted: "100",
        outputAmount: BigInt(100),
      }),
    ];

    const result = prepareHistoricalOrderChartData(trades, "light");

    expect(result.length).toEqual(1);
    expect(result[0].color).toEqual("#4E4AF6");
  });

  it("returns the absolute value of the io ratio (the output vault balance change is negative)", () => {
    // The output vault balance change is debited, so its formattedAmount is negative;
    // the raw input/output ratio is negative (50 / -100 = -0.5) and Math.abs yields
    // the positive charted value.
    const trades = [
      makeTrade({
        timestamp: 1632000000,
        inputFormatted: "50",
        outputFormatted: "-100",
        outputAmount: BigInt(100),
      }),
    ];

    const result = prepareHistoricalOrderChartData(trades, "dark");

    expect(result.length).toEqual(1);
    expect(result[0].value).toEqual(0.5);
  });

  it("keeps a single trade's value as-is even when its output amount is zero", () => {
    // A lone trade at a timestamp must go through the single-trade branch and
    // be preserved verbatim. The merge (weighted-average) branch would divide
    // by the zero output amount and produce NaN, so a zero output amount
    // distinguishes the two branches.
    const trades = [
      makeTrade({
        timestamp: 1632000000,
        inputFormatted: "50",
        outputFormatted: "100",
        outputAmount: BigInt(0),
      }),
    ];

    const result = prepareHistoricalOrderChartData(trades, "dark");

    expect(result.length).toEqual(1);
    expect(result[0].value).toEqual(0.5);
    expect(Number.isNaN(result[0].value)).toBe(false);
  });
}
