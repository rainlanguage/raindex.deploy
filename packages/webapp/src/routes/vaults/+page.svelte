<script lang="ts">
	import {
		PageHeader,
		useAccount,
		useToasts,
		useTransactions,
		VaultsListTable
	} from '@rainlanguage/ui-components';
	import { page } from '$app/stores';
	import {
		hideZeroBalanceVaults,
		hideInactiveOrdersVaults,
		orderHash,
		activeTokens,
		activeRaindexAddresses,
		selectedChainIds,
		ownerFilter
	} from '$lib/stores/settings';
	import { handleTransactionConfirmationModal, handleWithdrawAllModal } from '$lib/services/modal';
	import type { RaindexClient, RaindexVaultsList } from '@rainlanguage/raindex';
	import { handleVaultsWithdrawAll } from '$lib/services/handleVaultsWithdrawAll';
	import type { Hex } from 'viem';
	import LocalDbSyncGate from '$lib/components/LocalDbSyncGate.svelte';
	import { networkStatuses } from '$lib/stores/localDbStatus';

	const { showInactiveOrders } = $page.data.stores;

	const { account } = useAccount();
	const { errToast } = useToasts();
	const { manager } = useTransactions();
	let localDbRefreshVersion = 0;

	async function onWithdrawAll(raindexClient: RaindexClient, vaultsList: RaindexVaultsList) {
		if (!$account) {
			errToast('Please connect your wallet to withdraw');
			return;
		}
		await handleVaultsWithdrawAll({
			raindexClient,
			vaultsList,
			handleWithdrawAllModal,
			handleTransactionConfirmationModal,
			errToast,
			manager,
			account: $account as Hex
		});
	}
</script>

<PageHeader title="Vaults" pathname={$page.url.pathname} />

<LocalDbSyncGate
	nonBlocking
	{selectedChainIds}
	emptyMessage="No Vaults Found"
	on:synccomplete={() => (localDbRefreshVersion += 1)}
	let:emptyMessage
>
	<VaultsListTable
		{orderHash}
		{showInactiveOrders}
		{hideZeroBalanceVaults}
		{hideInactiveOrdersVaults}
		{activeTokens}
		{selectedChainIds}
		{activeRaindexAddresses}
		{ownerFilter}
		{emptyMessage}
		refreshVersion={localDbRefreshVersion}
		localDbStatuses={$networkStatuses}
		{onWithdrawAll}
	/>
</LocalDbSyncGate>
