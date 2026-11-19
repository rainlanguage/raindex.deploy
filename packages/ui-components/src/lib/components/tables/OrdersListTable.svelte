<script lang="ts">
	import { getNetworkName } from '$lib/utils/getNetworkName';
	import { goto } from '$app/navigation';
	import { DotsVerticalOutline } from 'flowbite-svelte-icons';
	import { createInfiniteQuery, createQuery } from '@tanstack/svelte-query';
	import {
		RaindexOrder,
		type RaindexCfg,
		type Address,
		type NetworkSyncStatus
	} from '@rainlanguage/raindex';
	import TanstackAppTable from '../TanstackAppTable.svelte';
	import { formatTimestampSecondsAsLocal } from '../../services/time';
	import { useLocalTime } from '../../storesGeneric/useLocalTime';
	import ListViewRaindexFilters from '../ListViewRaindexFilters.svelte';
	import Hash, { HashType } from '../Hash.svelte';
	import VaultCard from '../VaultCard.svelte';
	import { DEFAULT_PAGE_SIZE, DEFAULT_REFRESH_INTERVAL } from '../../queries/constants';
	import { QKEY_ORDERS, QKEY_TOKENS } from '../../queries/keys';
	import type { AppStoresInterface } from '../../types/appStores';
	import {
		Badge,
		Button,
		Dropdown,
		DropdownItem,
		TableBodyCell,
		TableHeadCell
	} from 'flowbite-svelte';
	import { useAccount } from '$lib/providers/wallet/useAccount';
	import { useRaindexClient } from '$lib/hooks/useRaindexClient';
	import { getAllContexts } from 'svelte';

	const context = getAllContexts();

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	export let handleOrderRemoveModal: any = undefined;
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	export let handleTakeOrderModal: any = undefined;

	export let selectedChainIds: AppStoresInterface['selectedChainIds'];
	export let showInactiveOrders: AppStoresInterface['showInactiveOrders'];
	export let orderHash: AppStoresInterface['orderHash'];
	export let hideZeroBalanceVaults: AppStoresInterface['hideZeroBalanceVaults'];
	export let hideInactiveOrdersVaults: AppStoresInterface['hideInactiveOrdersVaults'];
	export let activeTokens: AppStoresInterface['activeTokens'];
	export let activeRaindexAddresses: AppStoresInterface['activeRaindexAddresses'];
	export let ownerFilter: AppStoresInterface['ownerFilter'];
	export let emptyMessage = 'No Orders Found';
	export let refreshVersion = 0;
	export let localDbStatuses: Map<number, NetworkSyncStatus> | undefined = undefined;

	const { matchesAccount, account } = useAccount();
	const raindexClient = useRaindexClient();

	$: ownerAddress = $ownerFilter?.trim() || '';
	$: owners = ownerAddress ? [ownerAddress as Address] : ([] as Address[]);

	$: tokensQuery = createQuery({
		queryKey: [QKEY_TOKENS, $selectedChainIds],
		queryFn: async () => {
			const result = await raindexClient.getAllVaultTokens($selectedChainIds);
			if (result.error) throw new Error(result.error.readableMsg);
			return result.value;
		},
		enabled: true
	});

	$: selectedTokens =
		$activeTokens?.filter(
			(address) => !$tokensQuery.data || $tokensQuery.data.some((t) => t.address === address)
		) ?? [];

	$: raindexesMap = raindexClient.getAllRaindexes()?.value ?? new Map<string, RaindexCfg>();
	$: configuredNetworks = Array.from(raindexesMap.values(), (cfg) => cfg.network);
	$: availableRaindexAddresses = (() => {
		const addrs: string[] = [];
		raindexesMap.forEach((cfg: RaindexCfg) => {
			if ($selectedChainIds.length === 0 || $selectedChainIds.includes(cfg.network.chainId)) {
				addrs.push(cfg.address.toLowerCase());
			}
		});
		return addrs;
	})();
	$: selectedRaindexAddresses =
		$activeRaindexAddresses?.filter((address) =>
			availableRaindexAddresses.includes(address.toLowerCase())
		) ?? [];

	$: query = createInfiniteQuery({
		queryKey: [
			QKEY_ORDERS,
			$selectedChainIds,
			ownerAddress,
			$showInactiveOrders,
			$orderHash,
			selectedTokens,
			selectedRaindexAddresses
		],
		queryFn: async ({ pageParam }) => {
			const result = await raindexClient.getOrders(
				$selectedChainIds,
				{
					owners,
					active: $showInactiveOrders ? undefined : true,
					orderHash: $orderHash || undefined,
					tokens:
						selectedTokens.length > 0
							? { inputs: selectedTokens, outputs: selectedTokens }
							: undefined,
					raindexAddresses:
						selectedRaindexAddresses.length > 0 ? selectedRaindexAddresses : undefined
				},
				pageParam + 1
			);
			if (result.error) throw new Error(result.error.readableMsg);
			return result.value;
		},
		initialPageParam: 0,
		getNextPageParam(lastPage, _allPages, lastPageParam) {
			return lastPage.orders.length === DEFAULT_PAGE_SIZE ? lastPageParam + 1 : undefined;
		},
		refetchInterval: DEFAULT_REFRESH_INTERVAL,
		enabled: true
	});

	let appliedRefreshVersion = refreshVersion;
	$: if (refreshVersion > appliedRefreshVersion) {
		appliedRefreshVersion = refreshVersion;
		$query.refetch();
	}

	type OrdersListResult = { orders: RaindexOrder[]; totalCount: number };
	const AppTable = TanstackAppTable<RaindexOrder, OrdersListResult>;
</script>

<ListViewRaindexFilters
	{selectedChainIds}
	{showInactiveOrders}
	{orderHash}
	{hideZeroBalanceVaults}
	{hideInactiveOrdersVaults}
	{tokensQuery}
	{activeTokens}
	{selectedTokens}
	{activeRaindexAddresses}
	{selectedRaindexAddresses}
	{ownerFilter}
	{localDbStatuses}
/>

<AppTable
	{query}
	queryKey={QKEY_ORDERS}
	{emptyMessage}
	dataSelector={(page) => page.orders}
	on:clickRow={(e) => {
		goto(`/orders/${e.detail.item.chainId}-${e.detail.item.raindex}-${e.detail.item.orderHash}`);
	}}
>
	<svelte:fragment slot="title">
		<div class="mt-2 flex w-full justify-between">
			<div class="text-3xl font-medium dark:text-white" data-testid="title">Orders</div>
			<slot name="filters" />
		</div>
	</svelte:fragment>

	<svelte:fragment slot="head">
		{#if $account && (handleTakeOrderModal || handleOrderRemoveModal)}
			<TableHeadCell data-testid="orderListHeadingOrderInfo" padding="p-4" class="w-[15%]"
				>Order Info</TableHeadCell
			>
			<TableHeadCell data-testid="orderListHeadingAddresses" padding="p-4" class="w-[18%]"
				>Addresses</TableHeadCell
			>
			<TableHeadCell data-testid="orderListHeadingInputs" padding="px-2 py-4" class="w-[23.5%]"
				>Input Token(s)</TableHeadCell
			>
			<TableHeadCell data-testid="orderListHeadingOutputs" padding="px-2 py-4" class="w-[23.5%]"
				>Output Token(s)</TableHeadCell
			>
			<TableHeadCell data-testid="orderListHeadingTrades" padding="px-2 py-4" class="w-[10%]"
				>Trades</TableHeadCell
			>
			<TableHeadCell data-testid="orderListHeadingActions" padding="px-2 py-4" class="w-[10%]"
				>Actions</TableHeadCell
			>
		{:else}
			<TableHeadCell data-testid="orderListHeadingOrderInfo" padding="p-4" class="w-[15%]"
				>Order Info</TableHeadCell
			>
			<TableHeadCell data-testid="orderListHeadingAddresses" padding="p-4" class="w-[20%]"
				>Addresses</TableHeadCell
			>
			<TableHeadCell data-testid="orderListHeadingInputs" padding="px-2 py-4" class="w-[27.5%]"
				>Input Token(s)</TableHeadCell
			>
			<TableHeadCell data-testid="orderListHeadingOutputs" padding="px-2 py-4" class="w-[27.5%]"
				>Output Token(s)</TableHeadCell
			>
			<TableHeadCell data-testid="orderListHeadingTrades" padding="px-2 py-4" class="w-[10%]"
				>Trades</TableHeadCell
			>
		{/if}
	</svelte:fragment>

	<svelte:fragment slot="bodyRow" let:item>
		<TableBodyCell data-testid="orderListRowOrderInfo" tdClass="px-4 py-2">
			<div class="flex flex-col gap-1">
				<div class="flex items-center gap-2">
					<span class="text-sm font-medium"
						>{getNetworkName(Number(item.chainId), configuredNetworks) ??
							`Chain ${item.chainId}`}</span
					>
					{#if item.active}
						<Badge color="green">Active</Badge>
					{:else}
						<Badge color="yellow">Inactive</Badge>
					{/if}
				</div>
				<span class="text-xs text-gray-500 dark:text-gray-400">
					Added: {formatTimestampSecondsAsLocal(item.timestampAdded, $useLocalTime)}
				</span>
			</div>
		</TableBodyCell>
		<TableBodyCell data-testid="orderListRowAddresses" tdClass="px-4 py-2">
			<div class="flex flex-col gap-1 text-sm">
				<div class="flex items-center gap-1">
					<span class="text-gray-500 dark:text-gray-400">Order:</span>
					<Hash type={HashType.Identifier} value={item.orderHash} />
				</div>
				<div class="flex items-center gap-1">
					<span class="text-gray-500 dark:text-gray-400">Owner:</span>
					<Hash type={HashType.Wallet} value={item.owner} />
				</div>
				<div class="flex items-center gap-1">
					<span class="text-gray-500 dark:text-gray-400">Raindex:</span>
					<Hash type={HashType.Identifier} value={item.raindex} />
				</div>
			</div>
		</TableBodyCell>

		<TableBodyCell data-testid="orderListRowInputs" tdClass="p-2 whitespace-normal">
			<div class="grid w-full grid-cols-1 gap-2 sm:grid-cols-2">
				{#each item.inputsList.items as vault}
					<VaultCard {vault} />
				{/each}
				{#each item.inputsOutputsList.items as vault}
					{#if !item.inputsList.items.find((v) => v.id === vault.id)}
						<VaultCard {vault} />
					{/if}
				{/each}
			</div>
		</TableBodyCell>

		<TableBodyCell data-testid="orderListRowOutputs" tdClass="p-2 whitespace-normal">
			<div class="grid w-full grid-cols-1 gap-2 sm:grid-cols-2">
				{#each item.outputsList.items as vault}
					<VaultCard {vault} />
				{/each}
				{#each item.inputsOutputsList.items as vault}
					{#if !item.outputsList.items.find((v) => v.id === vault.id)}
						<VaultCard {vault} />
					{/if}
				{/each}
			</div>
		</TableBodyCell>

		<TableBodyCell data-testid="orderListRowTrades" tdClass="break-word p-2">
			{item.tradesCount}
		</TableBodyCell>
		{#if $account && (handleTakeOrderModal || handleOrderRemoveModal)}
			<TableBodyCell data-testid="orderListRowActions" tdClass="px-2 py-2">
				{#if item.active}
					{#if handleTakeOrderModal}
						<Button
							size="xs"
							data-testid={`order-take-${item.id}`}
							on:click={(e) => {
								e.stopPropagation();
								handleTakeOrderModal(item, $query.refetch, context);
							}}
						>
							Take Order
						</Button>
					{/if}
					{#if matchesAccount(item.owner) && handleOrderRemoveModal}
						<div data-testid="wallet-actions">
							<Button
								color="alternative"
								outline={false}
								data-testid={`order-menu-${item.id}`}
								id={`order-menu-${item.id}`}
								class="border-none px-2"
								title="Order actions"
								aria-label="Order actions"
								on:click={(e) => {
									e.stopPropagation();
								}}
							>
								<DotsVerticalOutline class="dark:text-white" />
							</Button>
							<Dropdown placement="bottom-end" triggeredBy={`#order-menu-${item.id}`}>
								<DropdownItem
									on:click={(e) => {
										e.stopPropagation();
										handleOrderRemoveModal(item, $query.refetch, context);
									}}>Remove</DropdownItem
								>
							</Dropdown>
						</div>
					{/if}
				{/if}
			</TableBodyCell>
		{/if}
	</svelte:fragment>
</AppTable>
