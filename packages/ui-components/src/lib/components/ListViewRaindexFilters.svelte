<script lang="ts">
	import { useRaindexClient } from '$lib/hooks/useRaindexClient';
	import type { QueryObserverResult } from '@tanstack/svelte-query';
	import type { Readable } from 'svelte/store';
	import DropdownActiveNetworks from './dropdown/DropdownActiveNetworks.svelte';
	import { page } from '$app/stores';
	import { isEmpty } from 'lodash';
	import { Alert } from 'flowbite-svelte';
	import type { Address, NetworkSyncStatus, RaindexVaultToken } from '@rainlanguage/raindex';
	import CheckboxActiveOrders from './checkbox/CheckboxActiveOrders.svelte';
	import DropdownTokensFilter from './dropdown/DropdownTokensFilter.svelte';
	import DropdownRaindexesFilter from './dropdown/DropdownRaindexesFilter.svelte';
	import InputOrderHash from './input/InputOrderHash.svelte';
	import InputOwnerFilter from './input/InputOwnerFilter.svelte';
	import CheckboxZeroBalanceVault from './CheckboxZeroBalanceVault.svelte';
	import CheckboxInactiveOrdersVault from './CheckboxInactiveOrdersVault.svelte';
	import type { AppStoresInterface } from '$lib/types/appStores';

	export let hideZeroBalanceVaults: AppStoresInterface['hideZeroBalanceVaults'];
	export let hideInactiveOrdersVaults: AppStoresInterface['hideInactiveOrdersVaults'];
	export let selectedChainIds: AppStoresInterface['selectedChainIds'];
	export let showInactiveOrders: AppStoresInterface['showInactiveOrders'];
	export let orderHash: AppStoresInterface['orderHash'];
	export let activeTokens: AppStoresInterface['activeTokens'];
	export let selectedTokens: Address[];
	export let tokensQuery: Readable<QueryObserverResult<RaindexVaultToken[], Error>>;
	export let activeRaindexAddresses: AppStoresInterface['activeRaindexAddresses'];
	export let selectedRaindexAddresses: Address[];
	export let ownerFilter: AppStoresInterface['ownerFilter'];
	export let localDbStatuses: Map<number, NetworkSyncStatus> | undefined = undefined;

	$: isVaultsPage = $page.url.pathname === '/vaults';
	$: isOrdersPage = $page.url.pathname === '/orders';

	const raindexClient = useRaindexClient();

	$: networks = raindexClient.getAllNetworks();
	$: configuredNetworks = Array.from(networks.value?.values() ?? []);
</script>

<div
	class="grid w-full items-center gap-4 md:flex md:justify-end lg:min-w-[600px]"
	style="grid-template-columns: repeat(2, minmax(0, 1fr));"
>
	{#if networks.error || isEmpty(networks.value)}
		<Alert color="gray" data-testid="no-networks-alert" class="w-full">
			No networks added to <a class="underline" href="/settings">settings</a>
		</Alert>
	{:else}
		{#if isVaultsPage}
			<div class="mt-4 w-full lg:w-auto">
				<CheckboxZeroBalanceVault {hideZeroBalanceVaults} />
			</div>
			<div class="mt-4 w-full lg:w-auto">
				<CheckboxInactiveOrdersVault {hideInactiveOrdersVaults} />
			</div>
		{/if}

		{#if isOrdersPage}
			<InputOrderHash {orderHash} />
			<div class="mt-4">
				<CheckboxActiveOrders {showInactiveOrders} />
			</div>
		{/if}
		<InputOwnerFilter {ownerFilter} />
		<DropdownTokensFilter
			{tokensQuery}
			{activeTokens}
			{selectedTokens}
			{configuredNetworks}
			label="Tokens"
		/>
		<DropdownRaindexesFilter
			{activeRaindexAddresses}
			{selectedRaindexAddresses}
			selectedChainIds={$selectedChainIds}
			{configuredNetworks}
			label="Raindexes"
		/>
		<DropdownActiveNetworks {selectedChainIds} {localDbStatuses} />
	{/if}
</div>
