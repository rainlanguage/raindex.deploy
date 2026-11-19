<script lang="ts">
	import {
		OrderDetail,
		PageHeader,
		useAccount,
		useToasts,
		useTransactions
	} from '@rainlanguage/ui-components';
	import { page } from '$app/stores';
	import { codeMirrorTheme, lightweightChartsTheme } from '$lib/darkMode';
	import {
		handleDepositModal,
		handleTransactionConfirmationModal,
		handleWithdrawModal,
		handleWithdrawAllModal,
		handleTakeOrderModal
	} from '$lib/services/modal';
	import type {
		Address,
		RaindexClient,
		RaindexOrder,
		RaindexVault,
		RaindexVaultsList
	} from '@rainlanguage/raindex';
	import type { Hex } from 'viem';
	import { handleRemoveOrder } from '$lib/services/handleRemoveOrder';
	import { handleVaultWithdraw } from '$lib/services/handleVaultWithdraw';
	import { handleVaultDeposit } from '$lib/services/handleVaultDeposit';
	import { handleVaultsWithdrawAll } from '$lib/services/handleVaultsWithdrawAll';
	import { handleTakeOrder } from '$lib/services/handleTakeOrder';
	import LocalDbSyncGate from '$lib/components/LocalDbSyncGate.svelte';
	import { readable } from 'svelte/store';

	const { orderHash, chainId, raindex } = $page.params;
	const parsedOrderHash = orderHash as Hex;
	const parsedChainId = Number(chainId);
	const raindexAddress = raindex as Address;
	const detailChainIds = readable([parsedChainId]);

	const { account } = useAccount();
	const { manager } = useTransactions();
	const { errToast, addToast } = useToasts();

	async function onRemove(raindexClient: RaindexClient, order: RaindexOrder) {
		await handleRemoveOrder({
			raindexClient,
			order,
			handleTransactionConfirmationModal,
			errToast,
			manager
		});
	}

	async function onDeposit(raindexClient: RaindexClient, vault: RaindexVault) {
		await handleVaultDeposit({
			raindexClient,
			vault,
			handleDepositModal,
			handleTransactionConfirmationModal,
			errToast,
			manager,
			account: $account as Hex
		});
	}

	async function onWithdraw(raindexClient: RaindexClient, vault: RaindexVault) {
		await handleVaultWithdraw({
			raindexClient,
			vault,
			handleWithdrawModal,
			handleTransactionConfirmationModal,
			errToast,
			manager,
			account: $account as Hex
		});
	}

	async function onWithdrawAll(raindexClient: RaindexClient, vaultsList: RaindexVaultsList) {
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

	function onTakeOrder(raindexClient: RaindexClient, order: RaindexOrder) {
		handleTakeOrder({
			raindexClient,
			order,
			handleTakeOrderModal,
			handleTransactionConfirmationModal,
			errToast,
			addToast,
			manager,
			account: $account as Hex
		});
	}
</script>

<PageHeader title="Order" pathname={$page.url.pathname} />

<LocalDbSyncGate nonBlocking selectedChainIds={detailChainIds}>
	<OrderDetail
		chainId={parsedChainId}
		{raindexAddress}
		orderHash={parsedOrderHash}
		{lightweightChartsTheme}
		{codeMirrorTheme}
		{onRemove}
		{onDeposit}
		{onWithdraw}
		{onWithdrawAll}
		{onTakeOrder}
	/>
</LocalDbSyncGate>
