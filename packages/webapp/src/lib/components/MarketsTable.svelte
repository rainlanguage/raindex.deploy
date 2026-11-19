<script lang="ts">
  import type { RaindexMarketSnapshot } from "@rainlanguage/raindex";
  import {
    formatPrice,
    formatVolume,
    shortAddress,
  } from "$lib/market-api/format";
  import { getNetworkName } from "@rainlanguage/ui-components";
  import TokenPairLogos from "./TokenPairLogos.svelte";

  export let markets: RaindexMarketSnapshot[];
  export let selectedTickerId: string | undefined;
  export let onSelect: (tickerId: string) => void;

  const chainName = (chainId: number) =>
    getNetworkName(chainId) ?? `Chain ${chainId}`;
</script>

<div
  class="overflow-hidden rounded-lg border border-gray-200 dark:border-gray-700"
>
  <table
    class="w-full table-fixed text-left text-sm text-gray-600 dark:text-gray-300"
  >
    <thead
      class="border-b border-gray-200 bg-gray-50 text-xs uppercase tracking-wide text-gray-500 dark:border-gray-700 dark:bg-gray-800/60 dark:text-gray-400"
    >
      <tr>
        <th class="w-[50%] px-3 py-3 font-medium sm:px-4 md:w-[28%] xl:w-[24%]"
          >Market</th
        >
        <th class="w-[30%] px-3 py-3 font-medium sm:px-4 md:w-[20%] xl:w-[16%]"
          >Last price</th
        >
        <th class="hidden w-[21%] px-4 py-3 font-medium lg:table-cell"
          >24h range</th
        >
        <th
          class="hidden w-[24%] px-4 py-3 font-medium md:table-cell xl:w-[21%]"
          >24h volume</th
        >
        <th class="hidden w-[8%] px-4 py-3 text-right font-medium xl:table-cell"
          >Trades</th
        >
        <th class="w-[20%] px-2 py-3 sm:px-4 md:w-[14%] xl:w-[10%]"
          ><span class="sr-only">Inspect market</span></th
        >
      </tr>
    </thead>
    <tbody
      class="divide-y divide-gray-100 bg-white dark:divide-gray-800 dark:bg-gray-900"
    >
      {#each markets as snapshot (snapshot.market.id)}
        <tr
          class:selected={selectedTickerId === snapshot.market.tickerId}
          class="transition-colors hover:bg-gray-50 dark:hover:bg-gray-800/40 [&.selected]:bg-primary-50 dark:[&.selected]:bg-primary-950/30"
          data-testid="market-row"
        >
          <td class="px-3 py-3 sm:px-4">
            <div class="flex items-center gap-3">
              <TokenPairLogos
                base={snapshot.market.base}
                quote={snapshot.market.quote}
              />
              <div class="min-w-0">
                <div
                  class="truncate font-semibold text-gray-900 dark:text-white"
                >
                  {snapshot.market.base.symbol}{" "}<span
                    class="font-normal text-gray-400">/</span
                  >{" "}{snapshot.market.quote.symbol}
                </div>
                <div
                  class="mt-0.5 hidden text-xs text-gray-500 dark:text-gray-400 sm:block"
                >
                  {chainName(snapshot.market.chainId)} · {shortAddress(
                    snapshot.market.base.address,
                  )}
                </div>
              </div>
            </div>
          </td>
          <td
            class="truncate px-3 py-3 font-mono tabular-nums text-gray-900 dark:text-gray-100 sm:px-4"
            title={snapshot.stats.lastPrice}
          >
            {formatPrice(snapshot.stats.lastPrice)}
            <span class="hidden sm:inline">{snapshot.market.quote.symbol}</span>
          </td>
          <td
            class="hidden min-w-0 px-4 py-3 font-mono text-xs tabular-nums lg:table-cell"
          >
            <div
              class="truncate"
              title={`${snapshot.stats.low24h ?? "—"} — ${snapshot.stats.high24h ?? "—"}`}
            >
              {formatPrice(snapshot.stats.low24h)}
              <span class="mx-1 text-gray-400">—</span>
              {formatPrice(snapshot.stats.high24h)}
            </div>
          </td>
          <td class="hidden min-w-0 px-4 py-3 md:table-cell">
            <div
              class="truncate font-mono tabular-nums text-gray-900 dark:text-gray-100"
              title={snapshot.stats.targetVolume24h}
            >
              {formatVolume(snapshot.stats.targetVolume24h)}
              {snapshot.market.quote.symbol}
            </div>
            <div
              class="mt-0.5 truncate font-mono text-xs tabular-nums text-gray-500"
              title={snapshot.stats.baseVolume24h}
            >
              {formatVolume(snapshot.stats.baseVolume24h)}
              {snapshot.market.base.symbol}
            </div>
          </td>
          <td
            class="hidden px-4 py-3 text-right font-mono tabular-nums xl:table-cell"
          >
            {snapshot.stats.tradeCount24h.toLocaleString()}
          </td>
          <td class="px-2 py-3 text-right sm:px-4">
            <button
              type="button"
              class="whitespace-nowrap rounded-md border border-gray-200 px-2 py-1.5 text-xs font-medium text-gray-700 transition-colors hover:border-primary-300 hover:text-primary-700 focus:outline-none focus:ring-2 focus:ring-primary-500 focus:ring-offset-2 dark:border-gray-700 dark:text-gray-300 dark:hover:border-primary-700 dark:hover:text-primary-300 dark:focus:ring-offset-gray-900 sm:px-3"
              on:click={() => onSelect(snapshot.market.tickerId)}
              aria-pressed={selectedTickerId === snapshot.market.tickerId}
              aria-label={selectedTickerId === snapshot.market.tickerId
                ? `${snapshot.market.base.symbol} / ${snapshot.market.quote.symbol} depth selected`
                : `View ${snapshot.market.base.symbol} / ${snapshot.market.quote.symbol} orderbook depth`}
            >
              {#if selectedTickerId === snapshot.market.tickerId}
                Selected
              {:else}
                <span class="sm:hidden">Depth</span><span
                  class="hidden sm:inline">View depth</span
                >
              {/if}
            </button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
