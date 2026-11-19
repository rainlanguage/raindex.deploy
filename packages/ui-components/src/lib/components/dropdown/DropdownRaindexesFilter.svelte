<script lang="ts">
	import { Button, Dropdown, Label, Checkbox, Input } from 'flowbite-svelte';
	import { ChevronDownSolid, SearchSolid } from 'flowbite-svelte-icons';
	import { isEmpty } from 'lodash';
	import type { Address, NetworkCfg, RaindexCfg } from '@rainlanguage/raindex';
	import type { AppStoresInterface } from '../../types/appStores';
	import { getNetworkName } from '$lib/utils/getNetworkName';
	import { useRaindexClient } from '$lib/hooks/useRaindexClient';

	export let activeRaindexAddresses: AppStoresInterface['activeRaindexAddresses'];
	export let selectedRaindexAddresses: Address[];
	export let selectedChainIds: number[];
	export let configuredNetworks: NetworkCfg[] = [];

	export let label: string = 'Filter by raindex';
	export let allLabel: string = 'All raindexes';
	export let emptyMessage: string = 'No raindexes available';

	interface RaindexItem {
		key: string;
		address: Address;
		label: string | undefined;
		chainId: number;
	}

	const raindexClient = useRaindexClient();

	let filteredRaindexes: RaindexItem[] = [];
	let searchTerm: string = '';
	let selectedIndex = 0;

	$: raindexesResult = raindexClient.getAllRaindexes();
	$: raindexesMap = raindexesResult?.value ?? new Map<string, RaindexCfg>();
	$: raindexesError = raindexesResult?.error;

	$: availableRaindexes = (() => {
		const items: RaindexItem[] = [];
		raindexesMap.forEach((cfg: RaindexCfg, key: string) => {
			if (selectedChainIds.length === 0 || selectedChainIds.includes(cfg.network.chainId)) {
				items.push({
					key,
					address: cfg.address as Address,
					label: cfg.label,
					chainId: cfg.network.chainId
				});
			}
		});
		return items;
	})();

	$: selectedCount = selectedRaindexAddresses.length;

	$: allSelected = selectedCount === availableRaindexes.length && availableRaindexes.length > 0;
	$: buttonText =
		selectedCount === 0
			? 'Select raindexes'
			: allSelected
				? allLabel
				: `${selectedCount} raindex${selectedCount > 1 ? 'es' : ''}`;

	$: {
		if (searchTerm.trim() === '') {
			filteredRaindexes = availableRaindexes;
			selectedIndex = 0;
		} else {
			const term = searchTerm.toLowerCase();
			filteredRaindexes = availableRaindexes.filter(
				(raindex) =>
					raindex.label?.toLowerCase().includes(term) ||
					raindex.address?.toLowerCase().includes(term) ||
					raindex.key?.toLowerCase().includes(term)
			);
			selectedIndex = filteredRaindexes.length > 0 ? 0 : -1;
		}
	}

	$: sortedFilteredRaindexes = [...filteredRaindexes].sort((a, b) => {
		const aSelected = selectedRaindexAddresses.some(
			(addr) => addr.toLowerCase() === a.address.toLowerCase()
		);
		const bSelected = selectedRaindexAddresses.some(
			(addr) => addr.toLowerCase() === b.address.toLowerCase()
		);
		if (aSelected === bSelected) return 0;
		return aSelected ? -1 : 1;
	});

	function getDisplayName(raindex: RaindexItem): string {
		const truncatedAddr = `${raindex.address.slice(0, 6)}...${raindex.address.slice(-4)}`;
		return raindex.label ? `${raindex.label} (${truncatedAddr})` : truncatedAddr;
	}

	function updateSelectedRaindexes(newSelection: Address[]) {
		activeRaindexAddresses.set(newSelection);
	}

	function toggleRaindex({ address }: RaindexItem) {
		if (!address) return;

		const normalizedAddress = address.toLowerCase() as Address;
		const isSelected = selectedRaindexAddresses.some(
			(addr) => addr.toLowerCase() === normalizedAddress
		);
		const newSelection = isSelected
			? $activeRaindexAddresses.filter((addr) => addr.toLowerCase() !== normalizedAddress)
			: [...$activeRaindexAddresses, normalizedAddress];

		updateSelectedRaindexes(newSelection);
	}

	function handleKeyDown(event: KeyboardEvent) {
		if (!sortedFilteredRaindexes.length) return;

		switch (event.key) {
			case 'Enter':
				event.preventDefault();
				if (sortedFilteredRaindexes.length > 0) {
					const raindexToToggle = sortedFilteredRaindexes[selectedIndex];
					if (raindexToToggle) {
						toggleRaindex(raindexToToggle);
					}
				}
				break;
			case 'ArrowDown':
				event.preventDefault();
				selectedIndex = Math.min(selectedIndex + 1, sortedFilteredRaindexes.length - 1);
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
			data-testid="dropdown-raindexes-filter-button"
			aria-label="Select raindexes to filter"
			aria-expanded="false"
			aria-haspopup="listbox"
		>
			<div class="w-[110px] overflow-hidden text-ellipsis whitespace-nowrap">
				{buttonText}
			</div>
			<ChevronDownSolid class="mx-2 h-3 w-3 text-black dark:text-white" />
		</Button>

		<Dropdown
			class="max-h-[75vh] w-full min-w-60 overflow-y-auto py-0"
			data-testid="dropdown-raindexes-filter"
		>
			{#if raindexesError}
				<div class="ml-2 w-full rounded-lg p-3 text-red-500">
					Cannot load raindexes list: {raindexesError.readableMsg || 'Unknown error'}
				</div>
			{:else if isEmpty(availableRaindexes)}
				<div class="ml-2 w-full rounded-lg p-3">{emptyMessage}</div>
			{:else}
				<div class="sticky top-0 bg-white p-3 dark:bg-gray-800">
					<Input
						placeholder="Search raindexes..."
						bind:value={searchTerm}
						autofocus
						on:keydown={handleKeyDown}
						data-testid="raindexes-filter-search"
					>
						<SearchSolid slot="left" class="h-4 w-4 text-gray-500" />
					</Input>
				</div>

				{#if isEmpty(filteredRaindexes)}
					<div class="ml-2 w-full rounded-lg p-3">No raindexes match your search</div>
				{:else}
					{#each sortedFilteredRaindexes as raindex, index (`${raindex.address}-${raindex.chainId}`)}
						<Checkbox
							data-testid="dropdown-raindexes-filter-option"
							class="w-full rounded-lg p-3 hover:bg-gray-100 dark:hover:bg-gray-600 {selectedIndex ===
							index
								? 'bg-blue-100 dark:bg-blue-900'
								: ''}"
							on:click={() => toggleRaindex(raindex)}
							checked={!!(
								raindex.address &&
								selectedRaindexAddresses.some(
									(addr) => addr.toLowerCase() === raindex.address.toLowerCase()
								)
							)}
						>
							<div class="ml-2 flex w-full">
								<div class="flex-1 text-sm font-medium">{getDisplayName(raindex)}</div>
								<div class="text-xs text-gray-500">
									{getNetworkName(raindex.chainId, configuredNetworks) ??
										`Chain ${raindex.chainId}`}
								</div>
							</div>
						</Checkbox>
					{/each}
				{/if}
			{/if}
		</Dropdown>
	</div>
</div>
