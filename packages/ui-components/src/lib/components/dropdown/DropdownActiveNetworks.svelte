<script lang="ts">
	import type { AppStoresInterface } from '$lib/types/appStores';
	import DropdownCheckbox from './DropdownCheckbox.svelte';
	import { getNetworkName } from '$lib/utils/getNetworkName';
	import { useRaindexClient } from '$lib/hooks/useRaindexClient';
	import type { NetworkCfg, NetworkSyncStatus } from '@rainlanguage/raindex';

	const raindexClient = useRaindexClient();

	export let selectedChainIds: AppStoresInterface['selectedChainIds'];
	export let localDbStatuses: Map<number, NetworkSyncStatus> | undefined = undefined;

	let dropdownOptions: Record<string, string> = {};
	let configuredNetworks: NetworkCfg[] = [];

	function getNetworkOptionLabel(
		chainId: number,
		networks: NetworkCfg[],
		statuses: Map<number, NetworkSyncStatus> | undefined
	): string {
		const networkName = getNetworkName(chainId, networks) ?? `Chain ${chainId}`;
		if (!statuses) return networkName;

		const localDbStatus = statuses.get(chainId)?.status;
		const source =
			localDbStatus === 'active'
				? 'Local DB'
				: localDbStatus === 'syncing'
					? 'Syncing'
					: localDbStatus === 'failure'
						? 'Sync failed'
						: 'Subgraph';
		return `${networkName} · ${source}`;
	}

	$: {
		const uniqueChainIds = raindexClient.getUniqueChainIds();
		const networks = raindexClient.getAllNetworks();
		configuredNetworks = Array.from(networks.value?.values() ?? []);
		if (uniqueChainIds.error) {
			dropdownOptions = {};
		} else {
			dropdownOptions = Object.fromEntries(
				uniqueChainIds.value.map((chainId) => [
					String(chainId),
					getNetworkOptionLabel(chainId, configuredNetworks, localDbStatuses)
				])
			);
		}
	}

	function handleStatusChange(event: CustomEvent<Record<string, string>>) {
		const chainIds = Object.keys(event.detail).map(Number);
		selectedChainIds.set(chainIds);
	}

	let value: Record<string, string> = {};
	$: {
		value = Object.fromEntries(
			$selectedChainIds.map((chainId) => [
				String(chainId),
				getNetworkOptionLabel(chainId, configuredNetworks, localDbStatuses)
			])
		);
	}
</script>

<div data-testid="subgraphs-dropdown">
	<DropdownCheckbox
		options={dropdownOptions}
		on:change={handleStatusChange}
		label="Networks"
		showAllLabel={false}
		onlyTitle={true}
		{value}
	/>
</div>
