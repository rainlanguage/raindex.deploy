<script lang="ts">
	import { page } from '$app/stores';
	import {
		OrdersListTable,
		PageHeader,
		useTransactions,
		useAccount,
		useToasts,
		useRaindexClient
	} from '@rainlanguage/ui-components';
	import type { AppStoresInterface } from '@rainlanguage/ui-components';
	import {
		orderHash,
		showInactiveOrders,
		activeTokens,
		selectedChainIds,
		activeRaindexAddresses,
		ownerFilter
	} from '$lib/stores/settings';
	import { handleTransactionConfirmationModal, handleTakeOrderModal } from '$lib/services/modal';
	import { handleTakeOrder } from '$lib/services/handleTakeOrder';
	import type { RaindexOrder } from '@rainlanguage/raindex';
	import type { Hex } from 'viem';
	import LocalDbSyncGate from '$lib/components/LocalDbSyncGate.svelte';
	import { networkStatuses } from '$lib/stores/localDbStatus';

	const { hideZeroBalanceVaults, hideInactiveOrdersVaults }: AppStoresInterface = $page.data.stores;

	const { manager } = useTransactions();
	const { account } = useAccount();
	const { errToast, addToast } = useToasts();
	const raindexClient = useRaindexClient();
	let localDbRefreshVersion = 0;

	const onTakeOrderCallback = (item: RaindexOrder) => {
		handleTakeOrder({
			raindexClient,
			order: item,
			handleTakeOrderModal,
			handleTransactionConfirmationModal,
			errToast,
			addToast,
			manager,
			account: $account as Hex
		});
	};
</script>

<PageHeader title={'Orders'} pathname={$page.url.pathname} />

<LocalDbSyncGate
	nonBlocking
	{selectedChainIds}
	emptyMessage="No Orders Found"
	on:synccomplete={() => (localDbRefreshVersion += 1)}
	let:emptyMessage
>
	<OrdersListTable
		{selectedChainIds}
		{showInactiveOrders}
		{orderHash}
		{hideZeroBalanceVaults}
		{hideInactiveOrdersVaults}
		{activeTokens}
		{activeRaindexAddresses}
		{ownerFilter}
		{emptyMessage}
		refreshVersion={localDbRefreshVersion}
		localDbStatuses={$networkStatuses}
		handleTakeOrderModal={onTakeOrderCallback}
	/>
</LocalDbSyncGate>
