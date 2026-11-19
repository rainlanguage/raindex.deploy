<script lang="ts">
	import { Button, Dropdown, Label, Checkbox, Input } from 'flowbite-svelte';
	import { ChevronDownSolid, SearchSolid } from 'flowbite-svelte-icons';
	import { isEmpty } from 'lodash';
	import type { Address, NetworkCfg, RaindexVaultToken } from '@rainlanguage/raindex';
	import type { AppStoresInterface } from '../../types/appStores';
	import type { Readable } from 'svelte/store';
	import type { QueryObserverResult } from '@tanstack/svelte-query';
	import { getTokenDisplayName } from '../../utils/tokens';
	import { getNetworkName } from '$lib/utils/getNetworkName';

	export let tokensQuery: Readable<QueryObserverResult<RaindexVaultToken[], Error>>;
	export let activeTokens: AppStoresInterface['activeTokens'];
	export let selectedTokens: Address[];
	export let configuredNetworks: NetworkCfg[] = [];

	export let label: string = 'Filter by tokens';
	export let allLabel: string = 'All tokens';
	export let emptyMessage: string = 'No tokens available';
	export let loadingMessage: string = 'Loading tokens...';

	let filteredTokens: RaindexVaultToken[] = [];
	let searchTerm: string = '';
	let selectedIndex = 0; // Selected item index

	$: availableTokens = $tokensQuery?.data || [];

	$: selectedCount = selectedTokens.length;

	$: allSelected = selectedCount === availableTokens.length && availableTokens.length > 0;
	$: buttonText =
		selectedCount === 0
			? 'Select tokens'
			: allSelected
				? allLabel
				: `${selectedCount} token${selectedCount > 1 ? 's' : ''}`;

	// Filter tokens based on search term and remove duplicates
	$: {
		let tokensToFilter = availableTokens;
		// Remove duplicates by address and chain
		const getKey = (token: RaindexVaultToken) => `${token.address}-${token.chainId}`;
		const uniqueTokensMap = new Map<string, RaindexVaultToken>();
		tokensToFilter.forEach((token) => {
			const key = getKey(token);
			if (token.address && !uniqueTokensMap.has(key)) {
				uniqueTokensMap.set(key, token);
			}
		});
		const uniqueTokens = Array.from(uniqueTokensMap.values());

		if (searchTerm.trim() === '') {
			filteredTokens = uniqueTokens;
		} else {
			const term = searchTerm.toLowerCase();
			filteredTokens = uniqueTokens.filter(
				(token) =>
					token.symbol?.toLowerCase().includes(term) ||
					token.name?.toLowerCase().includes(term) ||
					token.address?.toLowerCase().includes(term)
			);
			// Select first element in the list automatically if there are any results
			selectedIndex = filteredTokens.length > 0 ? 0 : -1;
		}
	}

	$: sortedFilteredTokens = [...filteredTokens].sort((a, b) => {
		const aSelected = selectedTokens.includes(a.address);
		const bSelected = selectedTokens.includes(b.address);
		if (aSelected === bSelected) return 0;
		return aSelected ? -1 : 1;
	});

	function updateSelectedTokens(newSelection: Address[]) {
		activeTokens.set(newSelection);
	}

	function toggleToken({ address }: RaindexVaultToken) {
		if (!address) return;

		const isSelected = selectedTokens.includes(address);
		const newSelection = isSelected
			? $activeTokens.filter((addr) => addr !== address)
			: [...$activeTokens, address];

		updateSelectedTokens(newSelection);
	}

	function handleKeyDown(event: KeyboardEvent) {
		if (!sortedFilteredTokens.length) return;

		switch (event.key) {
			case 'Enter':
				event.preventDefault();
				if (sortedFilteredTokens.length > 0) {
					const tokenToToggle = sortedFilteredTokens[selectedIndex];
					if (tokenToToggle) {
						toggleToken(tokenToToggle);
					}
				}
				break;
			case 'ArrowDown':
				event.preventDefault();
				selectedIndex = Math.min(selectedIndex + 1, sortedFilteredTokens.length - 1);
				break;
			case 'ArrowUp':
				event.preventDefault();
				selectedIndex = Math.max(selectedIndex - 1, 0);
				break;
			case 'Escape':
				searchTerm = '';
				selectedIndex = -1;
				break;
		}
	}
</script>

<div class="flex flex-col gap-x-2">
	<Label>{label}</Label>
	<div>
		<Button
			color="alternative"
			class="flex w-full justify-between overflow-hidden pl-2 pr-0 text-left"
			data-testid="dropdown-tokens-filter-button"
			aria-label="Select tokens to filter"
			aria-expanded="false"
			aria-haspopup="listbox"
		>
			<div class="w-[90px] overflow-hidden text-ellipsis whitespace-nowrap">
				{buttonText}
			</div>
			<ChevronDownSolid class="mx-2 h-3 w-3 text-black dark:text-white" />
		</Button>

		<Dropdown
			class="max-h-[75vh] w-full min-w-60 overflow-y-auto py-0"
			data-testid="dropdown-tokens-filter"
		>
			{#if $tokensQuery.isLoading}
				<div class="ml-2 w-full rounded-lg p-3">{loadingMessage}</div>
			{:else if $tokensQuery.isError}
				<div class="ml-2 w-full rounded-lg p-3 text-red-500">
					Cannot load tokens list: {$tokensQuery.error?.message || 'Unknown error'}
				</div>
			{:else if isEmpty(availableTokens)}
				<div class="ml-2 w-full rounded-lg p-3">{emptyMessage}</div>
			{:else}
				<!-- Search input -->
				<div class="sticky top-0 bg-white p-3 dark:bg-gray-800">
					<Input
						placeholder="Search tokens..."
						bind:value={searchTerm}
						autofocus
						on:keydown={handleKeyDown}
						data-testid="tokens-filter-search"
					>
						<SearchSolid slot="left" class="h-4 w-4 text-gray-500" />
					</Input>
				</div>

				{#if isEmpty(filteredTokens)}
					<div class="ml-2 w-full rounded-lg p-3">No tokens match your search</div>
				{:else}
					{#each sortedFilteredTokens as token, index (`${token.address}-${token.chainId}`)}
						<Checkbox
							data-testid="dropdown-tokens-filter-option"
							class="w-full rounded-lg p-3 hover:bg-gray-100 dark:hover:bg-gray-600 {selectedIndex ===
							index
								? 'bg-blue-100 dark:bg-blue-900'
								: ''}"
							on:click={() => toggleToken(token)}
							checked={!!(token.address && selectedTokens.includes(token.address))}
						>
							<div class="ml-2 flex w-full">
								<div class="flex-1 text-sm font-medium">{getTokenDisplayName(token)}</div>
								<div class="text-xs text-gray-500">
									{getNetworkName(token.chainId, configuredNetworks) ?? `Chain ${token.chainId}`}
								</div>
							</div>
						</Checkbox>
					{/each}
				{/if}
			{/if}
		</Dropdown>
	</div>
</div>
