<script lang="ts">
  import { page } from "$app/stores";
  import { createQuery } from "@tanstack/svelte-query";
  import { PageHeader } from "@rainlanguage/ui-components";
  import MarketsTable from "$lib/components/MarketsTable.svelte";
  import MarketDetailPanel from "$lib/components/MarketDetailPanel.svelte";
  import { getMarkets } from "$lib/market-api/client";

  let search = "";
  let selectedTickerId: string | undefined;

  const marketsQuery = createQuery({
    queryKey: ["market-api", "markets"],
    queryFn: ({ signal }) => getMarkets(undefined, undefined, signal),
    staleTime: 60_000,
    refetchInterval: 60_000,
    retry: 2,
  });

  $: markets = $marketsQuery.data ?? [];
  $: if (
    $marketsQuery.data &&
    selectedTickerId &&
    !$marketsQuery.data.some(
      (snapshot) => snapshot.market.tickerId === selectedTickerId,
    )
  ) {
    selectedTickerId = undefined;
  }
  $: normalizedSearch = search.trim().toLowerCase();
  $: filteredMarkets = markets
    .filter((snapshot) => {
      if (!normalizedSearch) return true;
      const { market } = snapshot;
      return [
        market.base.symbol,
        market.base.name,
        market.base.address,
        market.quote.symbol,
        market.tickerId,
        String(market.chainId),
      ].some((value) => value.toLowerCase().includes(normalizedSearch));
    })
    .sort(
      (a, b) =>
        Number(Boolean(b.stats.lastPrice)) -
          Number(Boolean(a.stats.lastPrice)) ||
        a.market.base.symbol.localeCompare(b.market.base.symbol),
    );
  $: activeMarketCount = markets.filter(
    (snapshot) => snapshot.stats.tradeCount24h > 0,
  ).length;
  $: networkCount = new Set(markets.map((snapshot) => snapshot.market.chainId))
    .size;
  $: latestObservation = markets.reduce(
    (latest, snapshot) => Math.max(latest, snapshot.observedAt),
    0,
  );
</script>

<PageHeader title="Markets" pathname={$page.url.pathname} />

<section class="min-w-0 overflow-x-hidden" aria-labelledby="markets-heading">
  <div class="mb-5 border-b border-gray-200 pb-5 dark:border-gray-700">
    <div class="flex flex-col justify-between gap-4 lg:flex-row lg:items-end">
      <h1
        id="markets-heading"
        class="text-2xl font-semibold tracking-tight text-gray-900 dark:text-white"
      >
        Raindex market data
      </h1>
      <div
        class="grid grid-cols-3 divide-x divide-gray-200 text-right dark:divide-gray-700"
      >
        <div class="px-4 first:pl-0">
          <div
            class="font-mono text-lg font-semibold tabular-nums text-gray-900 dark:text-white"
          >
            {markets.length}
          </div>
          <div class="text-xs text-gray-500">Markets</div>
        </div>
        <div class="px-4">
          <div
            class="font-mono text-lg font-semibold tabular-nums text-gray-900 dark:text-white"
          >
            {activeMarketCount}
          </div>
          <div class="text-xs text-gray-500">Traded 24h</div>
        </div>
        <div class="px-4 pr-0">
          <div
            class="font-mono text-lg font-semibold tabular-nums text-gray-900 dark:text-white"
          >
            {networkCount}
          </div>
          <div class="text-xs text-gray-500">Networks</div>
        </div>
      </div>
    </div>
  </div>

  <div
    class="mb-4 flex flex-col justify-between gap-3 sm:flex-row sm:items-center"
  >
    <label class="relative block w-full sm:max-w-sm">
      <span class="sr-only">Search markets</span>
      <svg
        class="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-gray-400"
        viewBox="0 0 20 20"
        fill="none"
        aria-hidden="true"
      >
        <path
          d="m14.5 14.5 3 3m-1.75-8.25a6.5 6.5 0 1 1-13 0 6.5 6.5 0 0 1 13 0Z"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
        />
      </svg>
      <input
        bind:value={search}
        type="search"
        placeholder="Search symbol, address, or chain ID"
        class="w-full rounded-md border border-gray-200 bg-gray-50 py-2 pl-9 pr-3 text-sm text-gray-900 placeholder:text-gray-400 focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500 dark:border-gray-700 dark:bg-gray-800 dark:text-white"
      />
    </label>
    <div class="flex items-center justify-between gap-3 sm:justify-end">
      {#if $marketsQuery.isRefetchError && markets.length > 0}
        <span
          class="inline-flex items-center gap-1.5 text-xs font-medium text-amber-700 dark:text-amber-300"
          role="status"
          title="Showing the last successful market snapshot because the latest refresh failed."
        >
          <span class="size-1.5 rounded-full bg-amber-500" aria-hidden="true"
          ></span>
          Cached · refresh failed
        </span>
      {/if}
      {#if latestObservation > 0}
        <span class="text-xs text-gray-500"
          >Observed {new Date(
            latestObservation * 1000,
          ).toLocaleTimeString()}</span
        >
      {/if}
      <button
        type="button"
        class="rounded-md border border-gray-200 px-3 py-2 text-sm font-medium text-gray-700 hover:border-primary-300 hover:text-primary-700 focus:outline-none focus:ring-2 focus:ring-primary-500 focus:ring-offset-2 disabled:cursor-wait disabled:opacity-60 dark:border-gray-700 dark:text-gray-300 dark:hover:border-primary-700 dark:hover:text-primary-300 dark:focus:ring-offset-gray-900"
        disabled={$marketsQuery.isFetching}
        on:click={() => $marketsQuery.refetch()}
      >
        {$marketsQuery.isFetching ? "Refreshing…" : "Refresh"}
      </button>
    </div>
  </div>

  {#if $marketsQuery.isPending}
    <div
      class="rounded-lg border border-gray-200 p-5 dark:border-gray-700"
      aria-label="Loading markets"
    >
      <div class="space-y-3">
        {#each [0, 1, 2, 3, 4, 5, 6] as skeleton (skeleton)}
          <div
            class="h-14 animate-pulse rounded-md bg-gray-100 dark:bg-gray-800"
          ></div>
        {/each}
      </div>
    </div>
  {:else if $marketsQuery.isError && markets.length === 0}
    <div
      class="rounded-lg border border-red-200 bg-red-50 p-6 text-center dark:border-red-900/60 dark:bg-red-950/20"
      role="alert"
    >
      <h2 class="font-semibold text-gray-900 dark:text-white">
        Market service is unavailable
      </h2>
      <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
        The cached market overview could not be loaded. Existing order, vault,
        and trade pages are unaffected.
      </p>
      <button
        type="button"
        class="mt-4 rounded-md bg-primary-600 px-3 py-2 text-sm font-medium text-white hover:bg-primary-700 focus:outline-none focus:ring-2 focus:ring-primary-500"
        on:click={() => $marketsQuery.refetch()}>Try again</button
      >
    </div>
  {:else if markets.length === 0}
    <div
      class="rounded-lg border border-gray-200 p-10 text-center dark:border-gray-700"
    >
      <h2 class="font-semibold text-gray-900 dark:text-white">
        No active Raindex markets
      </h2>
      <p class="mt-1 text-sm text-gray-500">
        No active Raindex orders currently provide a direct pair with the
        configured quote token.
      </p>
    </div>
  {:else}
    <div class="grid items-start gap-5 2xl:grid-cols-[minmax(0,1fr)_22rem]">
      <div class="min-w-0">
        {#if filteredMarkets.length > 0}
          <MarketsTable
            markets={filteredMarkets}
            {selectedTickerId}
            onSelect={(tickerId) => (selectedTickerId = tickerId)}
          />
        {:else}
          <div
            class="rounded-lg border border-gray-200 p-10 text-center dark:border-gray-700"
          >
            <h2 class="font-semibold text-gray-900 dark:text-white">
              No matching markets
            </h2>
            <p class="mt-1 text-sm text-gray-500">
              Try a token symbol, contract address, or chain ID.
            </p>
          </div>
        {/if}
      </div>
      <div
        class:hidden={!selectedTickerId}
        class:order-first={selectedTickerId}
        class="2xl:sticky 2xl:top-8 2xl:order-none 2xl:block"
      >
        <MarketDetailPanel tickerId={selectedTickerId} />
      </div>
    </div>
  {/if}
</section>
