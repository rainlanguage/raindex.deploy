<script lang="ts">
	import type { RaindexMarketToken } from '@rainlanguage/raindex';

	export let base: RaindexMarketToken;
	export let quote: RaindexMarketToken;
	export let large = false;

	const initials = (symbol: string) => symbol.trim().slice(0, 3).toUpperCase();
	const hideBrokenImage = (event: Event) => {
		(event.currentTarget as HTMLImageElement).hidden = true;
	};
</script>

<div class="flex shrink-0 -space-x-2" aria-hidden="true" data-testid="token-pair-logos">
	{#each [base, quote] as token, index (token.address)}
		<div
			class:size-10={large}
			class:size-7={!large}
			class:z-10={index === 0}
			class="relative flex shrink-0 items-center justify-center overflow-hidden rounded-full border border-gray-200 bg-gray-100 font-semibold text-gray-600 ring-2 ring-white dark:border-gray-700 dark:bg-gray-800 dark:text-gray-300 dark:ring-gray-900"
			title={token.name}
		>
			<span class:text-xs={large} class:text-[9px]={!large}>{initials(token.symbol)}</span>
			{#if token.logoUri}
				<img
					src={token.logoUri}
					alt=""
					class="absolute inset-0 size-full rounded-full object-cover"
					on:error={hideBrokenImage}
				/>
			{/if}
		</div>
	{/each}
</div>
