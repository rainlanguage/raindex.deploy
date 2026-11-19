<script lang="ts">
  import type { Readable } from "svelte/store";
  import { type RaindexOrder } from "@rainlanguage/raindex";
  import { createQuery } from "@tanstack/svelte-query";
  import { QKEY_ORDER_TRADES_LIST } from "../../queries/keys";
  import {
    CHART_COLORS,
    extractPairsFromTrades,
    findPairIndex,
    flipTradingPair,
    formatChartTimestamp,
    getDefaultPair,
    getTokenLabel,
    transformPairTrades,
    type TradingPair,
  } from "../../services/pairTradesChartData";
  import {
    TIME_DELTA_24_HOURS,
    TIME_DELTA_7_DAYS,
    TIME_DELTA_30_DAYS,
    TIME_DELTA_1_YEAR,
  } from "../../services/time";
  import { useLocalTime } from "../../storesGeneric/useLocalTime";
  import {
    Button,
    ButtonGroup,
    Dropdown,
    DropdownItem,
    Spinner,
  } from "flowbite-svelte";
  import { ChevronDownSolid } from "flowbite-svelte-icons";
  import {
    createChart,
    CrosshairMode,
    type IChartApi,
    type UTCTimestamp,
    type DeepPartial,
    type TimeScaleOptions,
  } from "lightweight-charts";
  import { onDestroy, onMount } from "svelte";

  export let order: RaindexOrder;
  export let lightweightChartsTheme: Readable<Record<string, unknown>>;

  let chartElement: HTMLElement | undefined = undefined;
  let chart: IChartApi | undefined;
  let priceSeries: ReturnType<IChartApi["addLineSeries"]> | undefined;
  let buyVolumeSeries: ReturnType<IChartApi["addHistogramSeries"]> | undefined;
  let sellVolumeSeries: ReturnType<IChartApi["addHistogramSeries"]> | undefined;

  const TIME_OPTIONS = [
    { delta: TIME_DELTA_1_YEAR, label: "1Y" },
    { delta: TIME_DELTA_30_DAYS, label: "30D" },
    { delta: TIME_DELTA_7_DAYS, label: "7D" },
    { delta: TIME_DELTA_24_HOURS, label: "24H" },
  ] as const;

  let timeDelta = TIME_DELTA_1_YEAR;
  let pairs: TradingPair[] = [];
  let selectedPairIndex = 0;
  let dropdownOpen = false;

  $: selectedPair = pairs[selectedPairIndex] ?? null;

  $: query = createQuery({
    queryKey: [QKEY_ORDER_TRADES_LIST, order.id],
    queryFn: async () => {
      const data = await order.getTradesList(undefined, undefined, undefined);
      if (data.error) throw new Error(data.error.readableMsg);
      return data.value.trades;
    },
  });

  $: trades = $query?.data ?? [];

  $: {
    if (trades.length > 0) {
      const isFirstLoad = pairs.length === 0;
      const extractedPairs = extractPairsFromTrades(trades);

      for (const newPair of extractedPairs) {
        if (findPairIndex(pairs, newPair) === -1) {
          pairs = [...pairs, newPair];
        }
      }

      if (isFirstLoad && pairs.length > 0) {
        const defaultPair = getDefaultPair(trades);
        if (defaultPair) {
          const idx = findPairIndex(pairs, defaultPair);
          if (idx >= 0) selectedPairIndex = idx;
        }
      }
    }
  }

  $: chartData = selectedPair
    ? transformPairTrades({
        trades,
        baseTokenAddress: selectedPair.baseToken.address,
        quoteTokenAddress: selectedPair.quoteToken.address,
        timeDeltaSeconds: timeDelta,
      })
    : null;

  $: pairLabel = selectedPair
    ? `${getTokenLabel(selectedPair.baseToken)}/${getTokenLabel(selectedPair.quoteToken)}`
    : "Select pair";

  function handleFlipPair() {
    if (!selectedPair) return;
    const flipped = flipTradingPair(selectedPair);
    const existingIdx = findPairIndex(pairs, flipped);
    if (existingIdx >= 0) {
      selectedPairIndex = existingIdx;
    } else {
      pairs = [...pairs, flipped];
      selectedPairIndex = pairs.length - 1;
    }
  }

  function selectPair(index: number) {
    selectedPairIndex = index;
    dropdownOpen = false;
  }

  function setupChart() {
    if (!chartElement) return;

    chart = createChart(chartElement, {
      autoSize: true,
      leftPriceScale: {
        visible: true,
        borderVisible: false,
      },
      rightPriceScale: {
        visible: true,
        borderVisible: false,
      },
      crosshair: {
        mode: CrosshairMode.Normal,
      },
      timeScale: {
        tickMarkFormatter: (time: number) =>
          formatChartTimestamp(time, timeDelta, $useLocalTime),
      },
    });

    buyVolumeSeries = chart.addHistogramSeries({
      color: CHART_COLORS.BUY_VOLUME_TRANSPARENT,
      priceFormat: { type: "volume" },
      priceScaleId: "left",
    });

    sellVolumeSeries = chart.addHistogramSeries({
      color: CHART_COLORS.SELL_VOLUME_TRANSPARENT,
      priceFormat: { type: "volume" },
      priceScaleId: "left",
    });

    buyVolumeSeries.createPriceLine({
      price: 0,
      color: CHART_COLORS.ZERO_LINE,
      lineWidth: 1,
      lineStyle: 0,
      axisLabelVisible: false,
    });

    sellVolumeSeries.createPriceLine({
      price: 0,
      color: CHART_COLORS.ZERO_LINE,
      lineWidth: 1,
      lineStyle: 0,
      axisLabelVisible: false,
    });

    priceSeries = chart.addLineSeries({
      color: CHART_COLORS.PRICE_LINE,
      lineWidth: 2,
      priceScaleId: "right",
      priceFormat: {
        type: "price",
        precision: 6,
        minMove: 0.000001,
      },
    });

    chart.priceScale("left").applyOptions({
      scaleMargins: { top: 0.1, bottom: 0.1 },
    });

    chart.priceScale("right").applyOptions({
      scaleMargins: { top: 0.1, bottom: 0.1 },
    });

    setChartOptions();
    updateChartData();
  }

  function setChartOptions() {
    if (!chart) return;
    chart.applyOptions({
      ...$lightweightChartsTheme,
      autoSize: true,
    });
  }

  function updateChartData() {
    if (!chart || !priceSeries || !buyVolumeSeries || !sellVolumeSeries) return;

    if (chartData) {
      priceSeries.setData(chartData.pricePoints);
      buyVolumeSeries.setData(chartData.buyVolumePoints);
      sellVolumeSeries.setData(chartData.sellVolumePoints);
    } else {
      priceSeries.setData([]);
      buyVolumeSeries.setData([]);
      sellVolumeSeries.setData([]);
    }

    setTimeScale();
  }

  function applyTickMarkFormatter(
    useLocal: boolean = $useLocalTime,
    delta: number = timeDelta,
  ) {
    if (!chart) return;

    chart.timeScale().applyOptions({
      tickMarkFormatter: (time: number) =>
        formatChartTimestamp(time, delta, useLocal),
    } as DeepPartial<TimeScaleOptions>);
  }

  function setTimeScale(delta: number = timeDelta) {
    if (!chart) return;

    applyTickMarkFormatter($useLocalTime, delta);

    if (chartData && chartData.pricePoints.length > 0) {
      const now = Math.floor(Date.now() / 1000) as UTCTimestamp;
      const from = (now - delta) as UTCTimestamp;
      chart.timeScale().setVisibleRange({ from, to: now });
    } else {
      chart.timeScale().fitContent();
    }
  }

  function destroyChart() {
    if (chart) {
      chart.remove();
      chart = undefined;
      priceSeries = undefined;
      buyVolumeSeries = undefined;
      sellVolumeSeries = undefined;
    }
  }

  $: if (chartElement && trades.length > 0 && selectedPair && !chart)
    setupChart();
  $: if (chart && chartData) updateChartData();
  $: if (chart && $lightweightChartsTheme) setChartOptions();
  // Timezone preference only refreshes labels — do not reset pan/zoom.
  $: if (chart) applyTickMarkFormatter($useLocalTime, timeDelta);
  // Time-range selection still resets the visible window to the selected delta.
  $: if (chart) setTimeScale(timeDelta);

  onMount(() => {
    if (trades.length > 0 && selectedPair) setupChart();
  });

  onDestroy(() => {
    destroyChart();
  });
</script>

<div
  class="flex h-full flex-col overflow-hidden rounded-lg border bg-gray-50 dark:border-none dark:bg-gray-700"
>
  <div
    class="flex w-full flex-wrap items-center justify-between gap-2 border-b p-3 dark:border-gray-700"
  >
    <div class="flex items-center gap-2">
      <div
        class="text-xl text-gray-900 dark:text-white"
        data-testid="chart-title"
      >
        Trades
      </div>
      {#if $query?.isLoading}
        <Spinner class="h-4 w-4" color="white" data-testid="chart-spinner" />
      {/if}
    </div>

    <div class="flex flex-wrap items-center gap-2">
      {#if pairs.length > 0}
        <div class="flex items-center gap-1">
          <Button
            color="alternative"
            size="xs"
            class="flex items-center gap-1"
            data-testid="pair-selector"
          >
            {pairLabel}
            <ChevronDownSolid class="h-3 w-3" />
          </Button>
          <Dropdown bind:open={dropdownOpen} data-testid="pair-dropdown">
            {#each pairs as pair, idx}
              <DropdownItem
                on:click={() => selectPair(idx)}
                data-testid="pair-option-{idx}"
              >
                {getTokenLabel(pair.baseToken)}/{getTokenLabel(pair.quoteToken)}
              </DropdownItem>
            {/each}
          </Dropdown>
          <Button
            color="alternative"
            size="xs"
            on:click={handleFlipPair}
            title="Flip pair"
            data-testid="flip-button"
          >
            <svg
              class="h-4 w-4"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4"
              />
            </svg>
          </Button>
        </div>
      {/if}

      {#if trades.length > 0}
        <ButtonGroup class="bg-gray-800" data-testid="time-filters">
          {#each TIME_OPTIONS as option}
            <Button
              on:click={() => (timeDelta = option.delta)}
              color={timeDelta === option.delta ? "primary" : "alternative"}
              size="xs"
              class="px-2 py-1">{option.label}</Button
            >
          {/each}
        </ButtonGroup>
      {/if}
    </div>
  </div>

  <div
    class="relative flex w-full grow items-center justify-center bg-white dark:bg-gray-800"
  >
    {#if $query?.isLoading}
      <div class="text-gray-800 dark:text-gray-400">Loading...</div>
    {:else if $query?.isError}
      <div class="text-red-500" data-testid="chart-error">
        Error: {$query?.error?.message ?? "Failed to load trades"}
      </div>
    {:else if trades.length === 0 || !selectedPair}
      <div class="text-gray-800 dark:text-gray-400" data-testid="chart-empty">
        No trades found
      </div>
    {:else}
      <div
        bind:this={chartElement}
        class="h-full w-full overflow-hidden"
        data-testid="chart-element"
      ></div>
      {#if chartData && chartData.pricePoints.length === 0}
        <div
          class="absolute inset-0 flex items-center justify-center bg-white/80 dark:bg-gray-800/80"
          data-testid="chart-no-data"
        >
          <span class="text-gray-800 dark:text-gray-400"
            >No trades found for selected pair and time range</span
          >
        </div>
      {/if}
    {/if}
  </div>
</div>
