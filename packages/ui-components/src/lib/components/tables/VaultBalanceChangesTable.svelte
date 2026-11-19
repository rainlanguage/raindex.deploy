<script lang="ts">
	import { Badge, Heading, TableHeadCell, TableBodyCell } from 'flowbite-svelte';
	import { createInfiniteQuery } from '@tanstack/svelte-query';
	import {
		RaindexVault,
		type RaindexVaultBalanceChange,
		type VaultBalanceChangeFilter
	} from '@rainlanguage/raindex';
	import { formatTimestampSecondsAsLocal } from '../../services/time';
	import { useLocalTime } from '../../storesGeneric/useLocalTime';
	import Hash, { HashType } from '../Hash.svelte';
	import { QKEY_VAULT_CHANGES } from '../../queries/keys';
	import { DEFAULT_PAGE_SIZE } from '../../queries/constants';
	import TanstackAppTable from '../TanstackAppTable.svelte';
	import Tooltip from '../Tooltip.svelte';
	import VaultBalanceChangeTypeFilter from '../VaultBalanceChangeTypeFilter.svelte';

	type BadgeColor = 'green' | 'yellow' | 'blue' | 'red' | 'purple' | 'pink' | 'dark';

	function getTypeBadgeColor(type: string): BadgeColor {
		const lowerType = type.toLowerCase();
		if (lowerType.includes('deposit')) return 'green';
		if (lowerType.includes('withdrawal')) return 'yellow';
		if (lowerType.includes('bounty')) return 'purple';
		if (lowerType.includes('trade') || lowerType.includes('takeorder')) return 'blue';
		if (lowerType.includes('clear')) return 'pink';
		return 'dark';
	}

	export let vault: RaindexVault;

	let filterTypes: VaultBalanceChangeFilter[] | undefined = undefined;

	$: balanceChangesQuery = createInfiniteQuery({
		queryKey: [vault.id, QKEY_VAULT_CHANGES + vault.id, filterTypes],
		queryFn: async ({ pageParam }) => {
			const result = await vault.getBalanceChanges(pageParam + 1, filterTypes);
			if (result.error) throw new Error(result.error.msg);
			return result.value;
		},
		initialPageParam: 0,
		getNextPageParam(lastPage, _allPages, lastPageParam) {
			return lastPage.length === DEFAULT_PAGE_SIZE ? lastPageParam + 1 : undefined;
		}
	});

	const AppTable = TanstackAppTable<RaindexVaultBalanceChange>;
</script>

<div class="flex w-full justify-end">
	<VaultBalanceChangeTypeFilter bind:value={filterTypes} />
</div>

<AppTable
	query={balanceChangesQuery}
	queryKey={vault.id}
	emptyMessage="No balance changes found"
	rowHoverable={false}
>
	<svelte:fragment slot="title">
		<Heading tag="h5" class="mb-4 mt-6 font-normal">Vault balance changes</Heading>
	</svelte:fragment>
	<svelte:fragment slot="head">
		<TableHeadCell padding="p-4" class="w-[18%]">Info</TableHeadCell>
		<TableHeadCell padding="p-4" class="w-[22%]">Transaction</TableHeadCell>
		<TableHeadCell padding="p-2" class="w-[30%]">Balance Change</TableHeadCell>
		<TableHeadCell padding="p-2" class="w-[30%]">New Balance</TableHeadCell>
	</svelte:fragment>

	<svelte:fragment slot="bodyRow" let:item>
		<TableBodyCell tdClass="px-4 py-2" data-testid="vaultBalanceChangesTableInfo">
			<div class="flex flex-col gap-1 overflow-hidden">
				<div class="flex">
					<Badge
						id={`type-${item.transaction.id}`}
						class="truncate"
						color={getTypeBadgeColor(item.type)}>{item.typeDisplayName}</Badge
					>
					<Tooltip triggeredBy={`#type-${item.transaction.id}`}>
						{item.typeDisplayName}
					</Tooltip>
				</div>
				<span class="text-xs text-gray-500 dark:text-gray-400">
					{formatTimestampSecondsAsLocal(BigInt(item.timestamp), $useLocalTime)}
				</span>
			</div>
		</TableBodyCell>
		<TableBodyCell tdClass="px-4 py-2" data-testid="vaultBalanceChangesTableTx">
			<div class="flex flex-col gap-1 text-sm">
				<div class="flex items-center gap-1">
					<span class="text-gray-500 dark:text-gray-400">Sender:</span>
					<Hash type={HashType.Wallet} value={item.transaction.from} />
				</div>
				<div class="flex items-center gap-1">
					<span class="text-gray-500 dark:text-gray-400">Tx:</span>
					<Hash type={HashType.Transaction} value={item.transaction.id} />
				</div>
			</div>
		</TableBodyCell>
		<TableBodyCell tdClass="p-2" data-testid="vaultBalanceChangesTableBalanceChange">
			<div class="flex flex-col overflow-hidden">
				<span class="truncate font-medium">{item.token.symbol}</span>
				<span
					id={`change-${item.transaction.id}`}
					class="truncate text-sm text-gray-500 dark:text-gray-400">{item.formattedAmount}</span
				>
				<Tooltip triggeredBy={`#change-${item.transaction.id}`}>
					{`${item.formattedAmount} ${item.token.symbol}`}
				</Tooltip>
			</div>
		</TableBodyCell>
		<TableBodyCell tdClass="p-2" data-testid="vaultBalanceChangesTableBalance">
			<div class="flex flex-col overflow-hidden">
				<span class="truncate font-medium">{item.token.symbol}</span>
				<span
					id={`balance-${item.transaction.id}`}
					class="truncate text-sm text-gray-500 dark:text-gray-400">{item.formattedNewBalance}</span
				>
				<Tooltip triggeredBy={`#balance-${item.transaction.id}`}>
					{`${item.formattedNewBalance} ${item.token.symbol}`}
				</Tooltip>
			</div>
		</TableBodyCell>
	</svelte:fragment>
</AppTable>
