<script lang="ts">
	import {
		localDbSyncGate,
		networkStatuses,
		raindexStatuses
	} from '$lib/stores/localDbStatus';
	import { getNetworkName, useRaindexClient } from '@rainlanguage/ui-components';
	import { createEventDispatcher } from 'svelte';
	import { readable, type Readable } from 'svelte/store';

	export let nonBlocking = false;
	export let selectedChainIds: Readable<number[]> = readable([]);
	export let emptyMessage = 'None found';
	// Svelte consumes this interface to type slot props.
	// eslint-disable-next-line @typescript-eslint/no-unused-vars
	interface $$Slots {
		default: { emptyMessage: string };
	}

	const dispatch = createEventDispatcher<{ synccomplete: void }>();
	const raindexClient = useRaindexClient();

	$: networksResult = raindexClient.getAllNetworks();
	$: configuredNetworks = Array.from(networksResult.value?.values() ?? []);
	$: localDbChainIds = Array.from(
		new Set([
			...Array.from($networkStatuses.keys()),
			...Array.from($raindexStatuses.values()).map((status) => status.raindexId.chainId)
		])
	);
	$: relevantChainIds =
		$selectedChainIds.length === 0
			? localDbChainIds
			: localDbChainIds.filter((chainId) => $selectedChainIds.includes(chainId));
	$: syncingChainIds = relevantChainIds.filter(
		(chainId) =>
			$networkStatuses.get(chainId)?.status === 'syncing' ||
			Array.from($raindexStatuses.values()).some(
				(status) => status.raindexId.chainId === chainId && status.status === 'syncing'
			)
	);
	$: failedChainIds = relevantChainIds.filter(
		(chainId) =>
			$networkStatuses.get(chainId)?.status === 'failure' ||
			Array.from($raindexStatuses.values()).some(
				(status) => status.raindexId.chainId === chainId && status.status === 'failure'
			)
	);
	$: syncingNetworkNames = syncingChainIds.map(
		(chainId) => getConfiguredNetworkName(chainId)
	);
	$: failedNetworkNames = failedChainIds.map(
		(chainId) => getConfiguredNetworkName(chainId)
	);
	$: resolvedEmptyMessage =
		syncingNetworkNames.length > 0
			? `Preparing data for ${formatNames(syncingNetworkNames)}…`
			: emptyMessage;

	let previouslySyncingChainIds = new Set<number>();
	$: if (nonBlocking) {
		const currentlySyncingChainIds = new Set(syncingChainIds);
		if (
			Array.from(previouslySyncingChainIds).some(
				(chainId) => !currentlySyncingChainIds.has(chainId)
			)
		) {
			dispatch('synccomplete');
		}
		previouslySyncingChainIds = currentlySyncingChainIds;
	}

	function formatNames(names: string[]): string {
		if (names.length <= 1) return names[0] ?? '';
		if (names.length === 2) return `${names[0]} and ${names[1]}`;
		return `${names.slice(0, -1).join(', ')}, and ${names.at(-1)}`;
	}

	function getConfiguredNetworkName(chainId: number): string {
		const configuredNetwork = configuredNetworks.find((network) => network.chainId === chainId);
		return (
			configuredNetwork?.label?.trim() ||
			getNetworkName(chainId) ||
			configuredNetwork?.key
				.split(/[-_\s]+/)
				.filter(Boolean)
				.map((part) => part.charAt(0).toUpperCase() + part.slice(1))
				.join(' ') ||
			`Chain ${chainId}`
		);
	}
</script>

{#if nonBlocking}
	{#if syncingNetworkNames.length > 0}
		<div
			data-testid="local-db-syncing-notice"
			class="mb-4 flex items-start gap-3 rounded-lg border border-sky-200 bg-sky-50 px-4 py-3 dark:border-sky-900/70 dark:bg-sky-950/30"
		>
			<div
				class="mt-0.5 h-4 w-4 shrink-0 animate-spin rounded-full border-2 border-sky-200 border-t-sky-600 dark:border-sky-900 dark:border-t-sky-400"
				aria-hidden="true"
			></div>
			<div>
				<p class="text-sm font-medium text-gray-900 dark:text-gray-100">
					Local database syncing: {formatNames(syncingNetworkNames)}
				</p>
				{#if $localDbSyncGate.status === 'syncing' && $localDbSyncGate.phaseMessage}
					<p class="mt-0.5 text-sm text-sky-700 dark:text-sky-300">
						{$localDbSyncGate.phaseMessage}
					</p>
				{/if}
			</div>
		</div>
	{:else if failedNetworkNames.length > 0}
		<div
			data-testid="local-db-sync-failure-notice"
			class="mb-4 rounded-lg border border-red-200 bg-red-50 px-4 py-3 dark:border-red-900/70 dark:bg-red-950/30"
		>
			<p class="text-sm font-medium text-gray-900 dark:text-gray-100">
				Local database sync failed for {formatNames(failedNetworkNames)}
			</p>
			{#if $localDbSyncGate.status === 'failure' && $localDbSyncGate.error}
				<p class="mt-1 text-sm text-red-700 dark:text-red-300">
					{$localDbSyncGate.error}
				</p>
			{/if}
		</div>
	{/if}
	<slot emptyMessage={resolvedEmptyMessage} />
{:else if $localDbSyncGate.status === 'syncing'}
	<div
		data-testid="local-db-syncing-notice"
		class="mx-auto mt-12 flex max-w-2xl flex-col items-center rounded-lg border border-sky-200 bg-sky-50 px-6 py-8 text-center shadow-sm dark:border-sky-900/70 dark:bg-sky-950/30"
	>
		<div
			class="mb-5 h-10 w-10 animate-spin rounded-full border-2 border-sky-200 border-t-sky-600 dark:border-sky-900 dark:border-t-sky-400"
			aria-hidden="true"
		></div>
		<h2 class="text-lg font-semibold text-gray-900 dark:text-gray-100">
			Local database syncing
		</h2>
		<p class="mt-2 max-w-lg text-sm leading-6 text-gray-600 dark:text-gray-300">
			We are preparing the local database. Orders and vaults will appear once the initial
			sync finishes.
		</p>
		{#if $localDbSyncGate.phaseMessage}
			<p class="mt-4 text-sm font-medium text-sky-700 dark:text-sky-300">
				{$localDbSyncGate.phaseMessage}
			</p>
		{/if}
	</div>
{:else if $localDbSyncGate.status === 'failure'}
	<div
		data-testid="local-db-sync-failure-notice"
		class="mx-auto mt-12 max-w-2xl rounded-lg border border-red-200 bg-red-50 px-6 py-8 text-center shadow-sm dark:border-red-900/70 dark:bg-red-950/30"
	>
		<h2 class="text-lg font-semibold text-gray-900 dark:text-gray-100">
			Local database sync failed
		</h2>
		<p class="mt-2 text-sm leading-6 text-gray-600 dark:text-gray-300">
			Orders and vaults are unavailable until the local database finishes syncing.
		</p>
		{#if $localDbSyncGate.error}
			<p class="mt-4 text-sm font-medium text-red-700 dark:text-red-300">
				{$localDbSyncGate.error}
			</p>
		{/if}
	</div>
{:else}
	<slot {emptyMessage} />
{/if}
