<script lang="ts">
	import { toHex } from 'viem';
	import { useRaindexClient } from '$lib/hooks/useRaindexClient';
	import {
		Button,
		Checkbox,
		Dropdown,
		DropdownItem,
		TableBodyCell,
		TableHeadCell
	} from 'flowbite-svelte';
	import { goto } from '$app/navigation';
	import { ArrowUpFromBracketOutline, DotsVerticalOutline } from 'flowbite-svelte-icons';
	import { createInfiniteQuery, createQuery } from '@tanstack/svelte-query';
	import TanstackAppTable from '../TanstackAppTable.svelte';
	import ListViewRaindexFilters from '../ListViewRaindexFilters.svelte';
	import OrderOrVaultHash from '../OrderOrVaultHash.svelte';
	import Hash, { HashType } from '../Hash.svelte';
	import { DEFAULT_PAGE_SIZE, DEFAULT_REFRESH_INTERVAL } from '../../queries/constants';
	import {
		Float,
		RaindexClient,
		RaindexVault,
		RaindexVaultsList,
		type Address,
		type NetworkSyncStatus,
		type RaindexVaultsListResult,
		type RaindexCfg
	} from '@rainlanguage/raindex';
	import { QKEY_TOKENS, QKEY_VAULTS } from '../../queries/keys';
	import type { AppStoresInterface } from '$lib/types/appStores.ts';
	import { useAccount } from '$lib/providers/wallet/useAccount';
	import { getNetworkName } from '$lib/utils/getNetworkName';
	import { getAllContexts } from 'svelte';
	import Tooltip from '../Tooltip.svelte';
	import { useToasts } from '$lib/providers/toasts/useToasts';

	const context = getAllContexts();
	const { errToast } = useToasts();

	export let orderHash: AppStoresInterface['orderHash'];
	export let showInactiveOrders: AppStoresInterface['showInactiveOrders'];
	export let hideZeroBalanceVaults: AppStoresInterface['hideZeroBalanceVaults'];
	export let hideInactiveOrdersVaults: AppStoresInterface['hideInactiveOrdersVaults'];
	export let activeTokens: AppStoresInterface['activeTokens'];
	export let selectedChainIds: AppStoresInterface['selectedChainIds'];
	export let activeRaindexAddresses: AppStoresInterface['activeRaindexAddresses'];
	export let ownerFilter: AppStoresInterface['ownerFilter'];
	export let emptyMessage = 'No Vaults Found';
	export let refreshVersion = 0;
	export let localDbStatuses: Map<number, NetworkSyncStatus> | undefined = undefined;
	export let handleDepositModal:
		| ((
				vault: RaindexVault,
				refetch: () => void,
				context: ReturnType<typeof getAllContexts>
		  ) => void)
		| undefined = undefined;
	export let handleWithdrawModal:
		| ((
				vault: RaindexVault,
				refetch: () => void,
				context: ReturnType<typeof getAllContexts>
		  ) => void)
		| undefined = undefined;

	export let onWithdrawAll:
		| ((raindexClient: RaindexClient, vaultsList: RaindexVaultsList) => void | Promise<void>)
		| undefined = undefined;

	const { account } = useAccount();
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
			QKEY_VAULTS,
			$hideZeroBalanceVaults,
			$hideInactiveOrdersVaults,
			$selectedChainIds,
			ownerAddress,
			selectedTokens,
			selectedRaindexAddresses
		],
		queryFn: async ({ pageParam }) => {
			const result = await raindexClient.getVaults(
				$selectedChainIds,
				{
					owners,
					hideZeroBalance: $hideZeroBalanceVaults,
					tokens: selectedTokens,
					raindexAddresses:
						selectedRaindexAddresses.length > 0 ? selectedRaindexAddresses : undefined,
					onlyActiveOrders: $hideInactiveOrdersVaults
				},
				pageParam + 1,
				DEFAULT_PAGE_SIZE
			);
			if (result.error) throw new Error(result.error.readableMsg);
			return result.value;
		},
		initialPageParam: 0,
		getNextPageParam(lastPage, _allPages, lastPageParam) {
			return lastPage.hasMore ? lastPageParam + 1 : undefined;
		},
		refetchInterval: DEFAULT_REFRESH_INTERVAL,
		enabled: true
	});

	let appliedRefreshVersion = refreshVersion;
	$: if (refreshVersion > appliedRefreshVersion) {
		appliedRefreshVersion = refreshVersion;
		$query.refetch();
	}

	$: if (selectedVaults.size > 0 && !$account) {
		// If User disconnected — clear selected vaults
		selectedVaults = new Set<string>();
		selectedVaultsOnChainId = null;
	}

	let selectedVaults = new Set<string>();
	let selectedVaultsOnChainId: number | null = null;
	const getToggleSelectVaultHandler = (vaultId: string, chainId: number) => (e: Event) => {
		e.stopPropagation();

		if (selectedVaults.has(vaultId)) {
			selectedVaults.delete(vaultId);
			if (selectedVaults.size === 0) {
				selectedVaultsOnChainId = null;
			}
		} else {
			selectedVaults.add(vaultId);
			if (selectedVaultsOnChainId === null) {
				selectedVaultsOnChainId = chainId;
			}
		}
		// To trigger Svelte update
		selectedVaults = new Set(selectedVaults);
	};
	const stopPropagation = (e: Event) => e.stopPropagation();
	const handleWithdrawAll = () => {
		const pages = $query.data?.pages ?? [];
		if (!onWithdrawAll || pages.length === 0) {
			return;
		}
		// Combine across all loaded pages so selections beyond the first page are respected
		const selectedIds = Array.from(selectedVaults);
		try {
			// We need to pick by ids from all vaults first to get filtered copies,
			// otherwise it may break wasm reference
			const filteredVaultListResults = pages.reduce(
				(prev, cur) => {
					const result = cur.vaults.pickByIds(selectedIds);
					if (result.error) {
						throw new Error(result.error.readableMsg);
					}
					return [...prev, result.value];
				},
				<RaindexVaultsList[]>[]
			);
			// Now we can combine filtered VaultLists into one
			if (filteredVaultListResults.length === 0) {
				errToast('No selected vaults found in the loaded pages. Please refresh and try again.');
				return;
			}
			const [first, ...rest] = filteredVaultListResults;
			const combinedVaultsList = rest.reduce((prev, cur) => {
				const result = prev.concat(cur);
				if (result.error) {
					throw new Error(result.error.readableMsg);
				}
				return result.value;
			}, first);
			return onWithdrawAll(raindexClient, combinedVaultsList);
		} catch (err) {
			if (err instanceof Error) {
				errToast(err.message);
			}
		}
	};

	const ZERO_FLOAT = Float.parse('0').value;
	const isZeroBalance = (item: RaindexVault) => {
		if (!ZERO_FLOAT) return true;
		return item.balance.eq(ZERO_FLOAT).value;
	};
	const isSameChainId = (item: RaindexVault, chainId: number | null) => {
		return chainId === null || chainId === item.chainId;
	};
	const isDisabled = (item: RaindexVault, chainId: number | null) => {
		return !isSameChainId(item, chainId) || isZeroBalance(item);
	};
	const AppTable = TanstackAppTable<RaindexVault, RaindexVaultsListResult>;
</script>

{#if $query}
	<ListViewRaindexFilters
		{selectedChainIds}
		{showInactiveOrders}
		{orderHash}
		{hideZeroBalanceVaults}
		{hideInactiveOrdersVaults}
		{activeTokens}
		{tokensQuery}
		{selectedTokens}
		{activeRaindexAddresses}
		{selectedRaindexAddresses}
		{ownerFilter}
		{localDbStatuses}
	/>
	<AppTable
		{query}
		dataSelector={(page) => page.items}
		queryKey={QKEY_VAULTS}
		{emptyMessage}
		on:clickRow={(e) => {
			goto(`/vaults/${e.detail.item.chainId}-${e.detail.item.raindex}-${e.detail.item.id}`);
		}}
	>
		<svelte:fragment slot="title">
			<div class="mt-2 flex w-full justify-between">
				<div class="flex items-center gap-x-6">
					<div class="text-3xl font-medium dark:text-white">Vaults</div>
					<Button
						size="xs"
						on:click={handleWithdrawAll}
						disabled={!onWithdrawAll || selectedVaults.size === 0}
						data-testid="withdraw-all-button"
					>
						<ArrowUpFromBracketOutline size="xs" class="mr-2" />
						{selectedVaults.size > 0
							? `Withdraw selected (${selectedVaults.size})`
							: 'Withdraw vaults'}
					</Button>
				</div>
			</div>
		</svelte:fragment>
		<svelte:fragment slot="head">
			<TableHeadCell padding="p-0" class="w-[4%]"><span class="sr-only">Select</span></TableHeadCell
			>
			<TableHeadCell padding="pl-0 py-4" class="w-[8%]">Network</TableHeadCell>
			<TableHeadCell padding="px-4 py-4" class="w-[22%]">Addresses</TableHeadCell>
			<TableHeadCell padding="px-2 py-4" class="w-[22%]">Token</TableHeadCell>
			<TableHeadCell padding="px-3 py-4" class="w-[20%]">Input For</TableHeadCell>
			<TableHeadCell padding="px-3 py-4" class="w-[20%]">Output For</TableHeadCell>
			<TableHeadCell padding="p-0" class="w-[4%]"
				><span class="sr-only">Actions</span></TableHeadCell
			>
		</svelte:fragment>

		<svelte:fragment slot="bodyRow" let:item>
			<TableBodyCell tdClass="px-0" on:click={stopPropagation}>
				<Checkbox
					data-testid="vault-checkbox"
					class={`block px-2 py-4 ${$account?.toLowerCase() !== item.owner.toLowerCase() ? 'invisible' : ''}`}
					checked={selectedVaults.has(item.id)}
					disabled={isDisabled(item, selectedVaultsOnChainId)}
					on:change={getToggleSelectVaultHandler(item.id, item.chainId)}
					on:click={stopPropagation}
					aria-label={`Select vault ${item.id}`}
				/>
				{#if $account?.toLowerCase() === item.owner.toLowerCase() && isDisabled(item, selectedVaultsOnChainId)}
					<Tooltip>
						{isZeroBalance(item)
							? 'This vault has a zero balance'
							: 'This vault is on a different network'}
					</Tooltip>
				{/if}
			</TableBodyCell>

			<TableBodyCell tdClass="px-4 py-2" data-testid="vault-network">
				{getNetworkName(Number(item.chainId), configuredNetworks) ?? `Chain ${item.chainId}`}
			</TableBodyCell>

			<TableBodyCell data-testid="vaultAddresses" tdClass="px-4 py-2">
				<div class="flex flex-col gap-1 text-sm">
					<div class="flex items-center gap-1">
						<span class="text-gray-500 dark:text-gray-400">Vault:</span>
						<Hash type={HashType.Identifier} value={toHex(item.vaultId)} />
					</div>
					<div class="flex items-center gap-1">
						<span class="text-gray-500 dark:text-gray-400">Raindex:</span>
						<Hash type={HashType.Identifier} value={item.raindex} />
					</div>
					<div class="flex items-center gap-1">
						<span class="text-gray-500 dark:text-gray-400">Owner:</span>
						<Hash type={HashType.Wallet} value={item.owner} />
					</div>
				</div>
			</TableBodyCell>
			<TableBodyCell tdClass="p-2" data-testid="vault-token">
				<div class="flex flex-col overflow-hidden">
					<span class="truncate font-medium">{item.token.name}</span>
					<span class="max-w-[200px] truncate text-sm text-gray-500 dark:text-gray-400"
						>{item.formattedBalance} {item.token.symbol}</span
					>
				</div>
			</TableBodyCell>
			<TableBodyCell tdClass="break-all p-2">
				{#if item.ordersAsInput.length > 0}
					<div data-testid="vault-order-inputs" class="flex flex-wrap items-end justify-start">
						{#each item.ordersAsInput.slice(0, 3) as order}
							<OrderOrVaultHash
								type="orders"
								orderOrVault={order}
								chainId={item.chainId}
								raindexAddress={item.raindex}
							/>
						{/each}
						{#if item.ordersAsInput.length > 3}...{/if}
					</div>
				{/if}
			</TableBodyCell>
			<TableBodyCell tdClass="break-all p-2">
				{#if item.ordersAsOutput.length > 0}
					<div data-testid="vault-order-outputs" class="flex flex-wrap items-end justify-start">
						{#each item.ordersAsOutput.slice(0, 3) as order}
							<OrderOrVaultHash
								type="orders"
								orderOrVault={order}
								chainId={item.chainId}
								raindexAddress={item.raindex}
							/>
						{/each}
						{#if item.ordersAsOutput.length > 3}...{/if}
					</div>
				{/if}
			</TableBodyCell>
			<TableBodyCell tdClass="px-0 text-right">
				{#if handleDepositModal && handleWithdrawModal && item.owner.toLowerCase() === $account?.toLowerCase()}
					<Button
						color="alternative"
						outline={false}
						data-testid="vault-menu"
						id={`vault-menu-${item.id}`}
						class="mr-2 border-none px-2"
						title="Vault actions"
						aria-label="Vault actions"
						on:click={(e) => {
							e.stopPropagation();
						}}
					>
						<DotsVerticalOutline class="dark:text-white" />
					</Button>
				{/if}
			</TableBodyCell>
			{#if handleDepositModal && handleWithdrawModal && item.owner.toLowerCase() === $account?.toLowerCase()}
				<Dropdown
					data-testid="dropdown"
					placement="bottom-end"
					triggeredBy={`#vault-menu-${item.id}`}
				>
					<DropdownItem
						data-testid="deposit-button"
						on:click={(e) => {
							e.stopPropagation();
							handleDepositModal(item, $query.refetch, context);
						}}
						>Deposit
					</DropdownItem>
					<DropdownItem
						data-testid="withdraw-button"
						on:click={(e) => {
							e.stopPropagation();
							handleWithdrawModal(item, $query.refetch, context);
						}}
						>Withdraw
					</DropdownItem>
				</Dropdown>
			{/if}
		</svelte:fragment>
	</AppTable>
{/if}
