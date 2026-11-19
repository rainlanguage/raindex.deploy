<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { derived, writable } from 'svelte/store';
	import { getMarket } from '$lib/market-api/client';
	import { formatPrice, formatSpread, formatVolume, shortAddress } from '$lib/market-api/format';
	import TokenPairLogos from './TokenPairLogos.svelte';

	export let tickerId: string | undefined;

	const selectedTickerId = writable<string | undefined>();
	$: $selectedTickerId = tickerId;
	const detailQuery = createQuery(
		derived(selectedTickerId, ($tickerId) => ({
			queryKey: ['market-api', 'market', $tickerId],
			queryFn: ({ signal }: { signal: AbortSignal }) =>
				getMarket($tickerId as string, undefined, undefined, signal),
			enabled: Boolean($tickerId),
			staleTime: 60_000,
			refetchInterval: 60_000,
			retry: 1
		}))
	);

	$: snapshot = $detailQuery.data;
</script>

<aside
	class="min-h-[28rem] rounded-lg border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900"
	aria-label="Selected market details"
>
	{#if !tickerId}
		<div class="flex min-h-[28rem] flex-col items-center justify-center px-8 text-center">
			<div class="mb-4 flex size-12 items-center justify-center rounded-lg border border-gray-200 bg-gray-50 font-mono text-lg text-gray-400 dark:border-gray-700 dark:bg-gray-800">↕</div>
			<h2 class="font-semibold text-gray-900 dark:text-white">Inspect executable depth</h2>
			<p class="mt-2 max-w-xs text-sm leading-6 text-gray-500 dark:text-gray-400">
				Select a market to quote its live Raindex orders. Overview statistics remain cached and available independently.
			</p>
		</div>
	{:else if $detailQuery.isPending}
		<div class="space-y-4 p-5" aria-label="Loading market depth">
			<div class="h-6 w-40 animate-pulse rounded bg-gray-200 dark:bg-gray-800"></div>
			<div class="grid grid-cols-3 gap-2">
				{#each [0, 1, 2] as skeleton (skeleton)}
					<div class="h-20 animate-pulse rounded-md bg-gray-100 dark:bg-gray-800"></div>
				{/each}
			</div>
			<div class="h-56 animate-pulse rounded-md bg-gray-100 dark:bg-gray-800"></div>
		</div>
	{:else if $detailQuery.isError && !snapshot}
		<div class="flex min-h-[28rem] flex-col items-center justify-center px-8 text-center" role="alert">
			<h2 class="font-semibold text-gray-900 dark:text-white">Depth is temporarily unavailable</h2>
			<p class="mt-2 max-w-xs text-sm leading-6 text-gray-500 dark:text-gray-400">
				The service could not complete this market’s executable quote. Cached market statistics are unaffected.
			</p>
			<button
				type="button"
				class="mt-4 rounded-md bg-primary-600 px-3 py-2 text-sm font-medium text-white hover:bg-primary-700 focus:outline-none focus:ring-2 focus:ring-primary-500 focus:ring-offset-2 dark:focus:ring-offset-gray-900"
				on:click={() => $detailQuery.refetch()}
			>
				Retry quote
			</button>
		</div>
	{:else if snapshot}
		<div class="border-b border-gray-200 p-5 dark:border-gray-700">
			<div class="flex flex-col items-start gap-3 sm:flex-row sm:justify-between sm:gap-4">
				<div class="flex min-w-0 items-center gap-3">
					<TokenPairLogos base={snapshot.market.base} quote={snapshot.market.quote} large />
					<div class="min-w-0">
						<p class="text-xs font-medium uppercase tracking-wide text-gray-500">Executable market</p>
						<h2 class="mt-1 truncate text-xl font-semibold text-gray-900 dark:text-white">
							{snapshot.market.base.symbol} / {snapshot.market.quote.symbol}
						</h2>
					{#if $detailQuery.isRefetchError}
						<p
							class="mt-1.5 inline-flex items-center gap-1.5 text-xs font-medium text-amber-700 dark:text-amber-300"
							role="status"
							title="Showing the last successful quote because the latest refresh failed."
						>
							<span class="size-1.5 rounded-full bg-amber-500" aria-hidden="true"></span>
							Cached quote · refresh failed
						</p>
					{/if}
					</div>
				</div>
				<span class="rounded border border-gray-200 px-2 py-1 text-xs text-gray-500 dark:border-gray-700 dark:text-gray-400">Chain {snapshot.market.chainId}</span>
			</div>
			<div class="mt-5 grid grid-cols-3 overflow-hidden rounded-md border border-gray-200 dark:border-gray-700">
				<div class="p-3">
					<div class="text-xs font-medium text-emerald-600 dark:text-emerald-400">Best bid</div>
					<div class="mt-1 truncate font-mono text-sm tabular-nums text-gray-900 dark:text-white" title={snapshot.orderbook.bestBid}>
						{formatPrice(snapshot.orderbook.bestBid)}
					</div>
				</div>
				<div class="border-x border-gray-200 p-3 text-center dark:border-gray-700">
					<div class="text-xs font-medium text-gray-500">Spread</div>
					<div class="mt-1 font-mono text-sm tabular-nums text-gray-900 dark:text-white">
						{formatSpread(snapshot.orderbook.bestBid, snapshot.orderbook.bestAsk)}
					</div>
				</div>
				<div class="p-3 text-right">
					<div class="text-xs font-medium text-rose-600 dark:text-rose-400">Best ask</div>
					<div class="mt-1 truncate font-mono text-sm tabular-nums text-gray-900 dark:text-white" title={snapshot.orderbook.bestAsk}>
						{formatPrice(snapshot.orderbook.bestAsk)}
					</div>
				</div>
			</div>
		</div>

		<div class="grid grid-cols-2 gap-px border-b border-gray-200 bg-gray-200 dark:border-gray-700 dark:bg-gray-700">
			<div class="bg-white p-4 dark:bg-gray-900">
				<div class="text-xs text-gray-500">Last price</div>
				<div class="mt-1 truncate font-mono text-sm tabular-nums text-gray-900 dark:text-white" title={snapshot.stats.lastPrice}>{formatPrice(snapshot.stats.lastPrice)}</div>
			</div>
			<div class="bg-white p-4 dark:bg-gray-900">
				<div class="text-xs text-gray-500">24h trades</div>
				<div class="mt-1 font-mono text-sm tabular-nums text-gray-900 dark:text-white">{snapshot.stats.tradeCount24h.toLocaleString()}</div>
			</div>
			<div class="bg-white p-4 dark:bg-gray-900">
				<div class="text-xs text-gray-500">Base volume</div>
				<div class="mt-1 truncate font-mono text-sm tabular-nums text-gray-900 dark:text-white" title={snapshot.stats.baseVolume24h}>{formatVolume(snapshot.stats.baseVolume24h)} {snapshot.market.base.symbol}</div>
			</div>
			<div class="bg-white p-4 dark:bg-gray-900">
				<div class="text-xs text-gray-500">Quote volume</div>
				<div class="mt-1 truncate font-mono text-sm tabular-nums text-gray-900 dark:text-white" title={snapshot.stats.targetVolume24h}>{formatVolume(snapshot.stats.targetVolume24h)} {snapshot.market.quote.symbol}</div>
			</div>
			<div class="col-span-2 bg-white p-4 dark:bg-gray-900">
				<div class="text-xs text-gray-500">24h range</div>
				<div class="mt-1 truncate font-mono text-sm tabular-nums text-gray-900 dark:text-white" title={`${snapshot.stats.low24h ?? '—'} — ${snapshot.stats.high24h ?? '—'} ${snapshot.market.quote.symbol}`}>
					<span title={snapshot.stats.low24h}>{formatPrice(snapshot.stats.low24h)}</span>
					<span class="mx-2 text-gray-400">—</span>
					<span title={snapshot.stats.high24h}>{formatPrice(snapshot.stats.high24h)}</span>
					{snapshot.market.quote.symbol}
				</div>
			</div>
		</div>

		<div class="p-5">
			<div class="mb-3 flex items-center justify-between">
				<h3 class="text-sm font-semibold text-gray-900 dark:text-white">Orderbook</h3>
				<span class="text-xs text-gray-500">Top 10 per side</span>
			</div>
			<div class="grid grid-cols-2 gap-4">
				{#each [{ label: 'Bids', levels: snapshot.orderbook.bids, color: 'text-emerald-600 dark:text-emerald-400' }, { label: 'Asks', levels: snapshot.orderbook.asks, color: 'text-rose-600 dark:text-rose-400' }] as side}
					<div>
						<div class="mb-2 grid grid-cols-2 text-xs text-gray-500">
							<span class={side.color}>{side.label}</span><span class="text-right">Size</span>
						</div>
						<div class="space-y-1.5">
							{#each side.levels.slice(0, 10) as level}
								<div class="grid grid-cols-2 gap-2 font-mono text-xs tabular-nums" title={`Price ${level.price}, size ${level.baseQuantity}`}>
									<span class="truncate text-gray-900 dark:text-gray-100">{formatPrice(level.price)}</span>
									<span class="truncate text-right text-gray-500">{formatVolume(level.baseQuantity)}</span>
								</div>
							{/each}
							{#if side.levels.length === 0}<p class="text-xs text-gray-400">No executable levels</p>{/if}
						</div>
					</div>
				{/each}
			</div>
		</div>

		<div class="border-t border-gray-200 p-5 dark:border-gray-700">
			<div class="mb-3 flex items-center justify-between">
				<h3 class="text-sm font-semibold text-gray-900 dark:text-white">Recent trades</h3>
				<span class="text-xs text-gray-500">Latest 8</span>
			</div>
			{#if snapshot.recentTrades.length > 0}
				<div class="space-y-2">
					<div class="grid grid-cols-[3rem_1fr_1fr] gap-2 text-xs text-gray-500">
						<span>Side</span><span>Price</span><span class="text-right">Size</span>
					</div>
					{#each snapshot.recentTrades.slice(0, 8) as trade (trade.tradeId)}
						<div class="grid grid-cols-[3rem_1fr_1fr] gap-2 font-mono text-xs tabular-nums" title={new Date(trade.timestamp * 1000).toLocaleString()}>
							<span class={`uppercase ${trade.side === 'buy' ? 'text-emerald-600 dark:text-emerald-400' : 'text-rose-600 dark:text-rose-400'}`}>{trade.side}</span>
							<span class="truncate text-gray-900 dark:text-gray-100" title={trade.price}>{formatPrice(trade.price)}</span>
							<span class="truncate text-right text-gray-500" title={trade.baseVolume}>{formatVolume(trade.baseVolume)}</span>
						</div>
					{/each}
				</div>
			{:else}
				<p class="text-xs text-gray-400">No trades in the current 24-hour window.</p>
			{/if}
		</div>

		{#if snapshot.errors.length > 0}
			<div class="border-t border-amber-200 bg-amber-50 px-5 py-4 text-xs text-amber-900 dark:border-amber-900/60 dark:bg-amber-950/20 dark:text-amber-200">
				{#each snapshot.errors as issue}
					<p><span class="font-medium capitalize">{issue.source} {issue.severity}:</span> {issue.message}</p>
				{/each}
			</div>
		{/if}

		<details class="border-t border-gray-200 p-5 text-xs dark:border-gray-700">
			<summary class="cursor-pointer font-medium text-gray-700 focus:outline-none focus:ring-2 focus:ring-primary-500 dark:text-gray-300">Exact API values</summary>
			<p class="mt-2 leading-5 text-gray-500 dark:text-gray-400">
				Unrounded values returned by the Raindex market API.
			</p>
			<dl class="mt-3 grid gap-3 text-gray-500 dark:text-gray-400">
				{#each [
					['Last price', snapshot.stats.lastPrice ?? '—'],
					['24h low', snapshot.stats.low24h ?? '—'],
					['24h high', snapshot.stats.high24h ?? '—'],
					['Base volume', snapshot.stats.baseVolume24h],
					['Quote volume', snapshot.stats.targetVolume24h],
					['Best bid', snapshot.orderbook.bestBid ?? '—'],
					['Best ask', snapshot.orderbook.bestAsk ?? '—']
				] as value}
					<div class="min-w-0">
						<dt class="font-medium text-gray-700 dark:text-gray-300">{value[0]}</dt>
						<dd class="mt-0.5 break-all font-mono tabular-nums">{value[1]}</dd>
					</div>
				{/each}
			</dl>
		</details>

		<details class="border-t border-gray-200 p-5 text-xs dark:border-gray-700">
			<summary class="cursor-pointer font-medium text-gray-700 focus:outline-none focus:ring-2 focus:ring-primary-500 dark:text-gray-300">Market provenance</summary>
			<dl class="mt-3 space-y-2 text-gray-500 dark:text-gray-400">
				<div><dt class="inline font-medium">Ticker:</dt> <dd class="inline font-mono" title={snapshot.market.tickerId}>{shortAddress(snapshot.market.tickerId)}</dd></div>
				<div><dt class="inline font-medium">Base:</dt> <dd class="inline font-mono" title={snapshot.market.base.address}>{shortAddress(snapshot.market.base.address)}</dd></div>
				<div><dt class="inline font-medium">Quote:</dt> <dd class="inline font-mono" title={snapshot.market.quote.address}>{shortAddress(snapshot.market.quote.address)}</dd></div>
				<div><dt class="inline font-medium">Observed:</dt> <dd class="inline">{new Date(snapshot.observedAt * 1000).toLocaleString()}</dd></div>
			</dl>
		</details>
	{/if}
</aside>
