<script lang="ts">
	import Hash, { HashType } from '../Hash.svelte';
	import BadgeActive from '../BadgeActive.svelte';
	import OrderTradesChart from '../charts/OrderTradesChart.svelte';
	import OrderTradesListTable from '../tables/OrderTradesListTable.svelte';
	import TanstackOrderQuote from './TanstackOrderQuote.svelte';
	import TanstackPageContentDetail from './TanstackPageContentDetail.svelte';
	import CardProperty from '../CardProperty.svelte';
	import { formatTimestampSecondsAsLocal } from '../../services/time';
	import { useLocalTime } from '../../storesGeneric/useLocalTime';
	import ButtonVaultLink from '../ButtonVaultLink.svelte';
	import OrderVaultsVolTable from '../tables/OrderVaultsVolTable.svelte';
	import { QKEY_ORDER } from '../../queries/keys';
	import CodeMirrorRainlang from '../CodeMirrorRainlang.svelte';
	import { createQuery, useQueryClient } from '@tanstack/svelte-query';
	import { Button, Dropdown, DropdownItem, TabItem, Tabs, Tooltip } from 'flowbite-svelte';
	import { onDestroy } from 'svelte';
	// import OrderApy from '../tables/OrderAPY.svelte';
	import type { DebugTradeModalHandler, QuoteDebugModalHandler } from '../../types/modal';
	import Refresh from '../icon/Refresh.svelte';
	import { invalidateTanstackQueries } from '$lib/queries/queryClient';
	import {
		ArrowDownToBracketOutline,
		ArrowUpFromBracketOutline,
		ChevronDownOutline,
		InfoCircleOutline,
		WalletOutline
	} from 'flowbite-svelte-icons';
	import { getExplorerLink } from '$lib/services/getExplorerLink';
	import { useAccount } from '$lib/providers/wallet/useAccount';
	import {
		RaindexClient,
		RaindexOrder,
		RaindexVault,
		RaindexVaultsList,
		type Address,
		type Hex
	} from '@rainlanguage/raindex';
	import { useToasts } from '$lib/providers/toasts/useToasts';
	import { useRaindexClient } from '$lib/hooks/useRaindexClient';

	export let handleQuoteDebugModal: QuoteDebugModalHandler | undefined = undefined;
	export let handleDebugTradeModal: DebugTradeModalHandler | undefined = undefined;
	export let codeMirrorTheme;
	export let lightweightChartsTheme;
	export let raindexAddress: Address;
	export let orderHash: Hex;
	export let chainId: number;
	export let rpcs: string[] | undefined = undefined;

	/** Callback function when remove action is triggered for an order
	 * @param order The order to remove
	 */
	export let onRemove: (raindexClient: RaindexClient, order: RaindexOrder) => void;

	/** Callback function when remove-and-withdraw-all action is triggered for an order.
	 * Removes the order and withdraws all of its vault balances in a single transaction.
	 * @param order The order to remove
	 * @param vaultsList The VaultsList struct containing the order's vaults to withdraw from
	 */
	export let onRemoveAndWithdrawAll:
		| ((
				raindexClient: RaindexClient,
				order: RaindexOrder,
				vaultsList: RaindexVaultsList
		  ) => void)
		| undefined = undefined;

	/** Callback function when deposit action is triggered for a vault
	 * @param vault The vault to deposit into
	 */
	export let onDeposit: (raindexClient: RaindexClient, vault: RaindexVault) => void;

	/** Callback function when withdraw action is triggered for a vault
	 * @param vault The vault to withdraw from
	 */
	export let onWithdraw: (raindexClient: RaindexClient, vault: RaindexVault) => void;

	/** Callback function when withdraw all action is triggered for a vault
	 * @param vaultsList The VaultsList struct containing the vaults to withdraw from
	 */
	export let onWithdrawAll:
		| ((raindexClient: RaindexClient, vaultsList: RaindexVaultsList) => void)
		| undefined = undefined;

	/** Callback function when take order action is triggered
	 * @param order The order to take
	 */
	export let onTakeOrder:
		| ((raindexClient: RaindexClient, order: RaindexOrder) => void)
		| undefined = undefined;

	let codeMirrorDisabled = true;
	let codeMirrorStyles = {};

	const queryClient = useQueryClient();
	const { matchesAccount } = useAccount();
	const { errToast } = useToasts();
	const raindexClient = useRaindexClient();

	$: orderDetailQuery = createQuery<RaindexOrder>({
		queryKey: [QKEY_ORDER, orderHash],
		queryFn: async () => {
			const result = await raindexClient.getOrderByHash(chainId, raindexAddress, orderHash);
			if (result.error) throw new Error(result.error.readableMsg);
			return result.value;
		}
	});

	const interval = setInterval(async () => {
		await invalidateTanstackQueries(queryClient, [orderHash]);
	}, 10000);

	onDestroy(() => {
		clearInterval(interval);
	});

	const handleRefresh = async () => {
		try {
			await invalidateTanstackQueries(queryClient, [orderHash]);
		} catch {
			errToast('Failed to refresh');
		}
	};

	const vaultTypesMap = [
		{ key: 'Output vaults', type: 'output', getter: 'outputsList' },
		{ key: 'Input vaults', type: 'input', getter: 'inputsList' },
		{
			key: 'Input & output vaults',
			type: 'inputOutput',
			getter: 'inputsOutputsList'
		}
	] as const;

	const formatBuilderState = (builderState: string) => {
		try {
			return JSON.stringify(JSON.parse(builderState), null, 2);
		} catch {
			return 'Invalid builder state';
		}
	};
</script>

<TanstackPageContentDetail query={orderDetailQuery} emptyMessage="Order not found">
	<svelte:fragment slot="top" let:data>
		<div
			class="flex w-full flex-wrap items-center justify-between gap-4 text-3xl font-medium lg:justify-between dark:text-white"
		>
			<div class="flex items-center gap-x-2">
				<div class="flex gap-x-2">
					<span class="font-light">Order</span>
					<Hash shorten value={data.orderHash} />
				</div>
				<BadgeActive active={data.active} large />
			</div>

			<div class="flex items-center gap-2">
				{#if matchesAccount(data.owner) && data.active}
					{#if onRemoveAndWithdrawAll}
						<Button
							id="remove-order-menu"
							data-testid="remove-order-menu"
							aria-label="Remove order">
							Remove
							<ChevronDownOutline size="sm" class="ml-2" />
						</Button>
						<Dropdown placement="bottom-end" triggeredBy="#remove-order-menu">
							<DropdownItem
								on:click={() => onRemove(raindexClient, data)}
								data-testid="remove-button">Remove</DropdownItem
							>
							<DropdownItem
								on:click={() => onRemoveAndWithdrawAll(raindexClient, data, data.vaultsList)}
								data-testid="remove-and-withdraw-all-button"
								>Remove and withdraw all vaults</DropdownItem
							>
						</Dropdown>
					{:else}
						<Button
							on:click={() => onRemove(raindexClient, data)}
							data-testid="remove-button"
							aria-label="Remove order">Remove</Button
						>
					{/if}
				{/if}
				{#if data.active && onTakeOrder}
					<Button
						on:click={() => onTakeOrder(raindexClient, data)}
						data-testid="take-order-button"
						aria-label="Take order">Take Order</Button
					>
				{/if}

				<Refresh
					testId="top-refresh"
					title="Refresh order"
					ariaLabel="Refresh order"
					on:click={handleRefresh}
					spin={$orderDetailQuery.isLoading || $orderDetailQuery.isFetching}
				/>
			</div>
		</div>
	</svelte:fragment>
	<svelte:fragment slot="card" let:data>
		<div class="flex flex-col gap-y-6">
			<CardProperty>
				<svelte:fragment slot="key">Raindex</svelte:fragment>
				<svelte:fragment slot="value">
					<Hash type={HashType.Identifier} shorten={false} value={data.raindex} />
				</svelte:fragment>
			</CardProperty>

			<CardProperty>
				<svelte:fragment slot="key">Owner</svelte:fragment>
				<svelte:fragment slot="value">
					{@const explorerLink = getExplorerLink(data.owner, chainId, 'address')}
					{#if explorerLink}
						<a
							href={explorerLink}
							target="_blank"
							rel="noopener noreferrer"
							class="flex items-center justify-start space-x-2 text-left text-blue-500 hover:underline"
						>
							<WalletOutline size="sm" />
							<span>{data.owner}</span>
						</a>
					{:else}
						<Hash type={HashType.Wallet} shorten={false} value={data.owner} />
					{/if}
				</svelte:fragment>
			</CardProperty>

			<CardProperty>
				<svelte:fragment slot="key">Created</svelte:fragment>
				<svelte:fragment slot="value">
					{formatTimestampSecondsAsLocal(data.timestampAdded, $useLocalTime)}
				</svelte:fragment>
			</CardProperty>

			{#if data.oracleUrl}
				<CardProperty>
					<svelte:fragment slot="key">
						<div class="flex items-center gap-x-2">
							Oracle
							<InfoCircleOutline class="h-4 w-4" /><Tooltip
								>This order uses a signed context oracle for external data (e.g. price feeds).
								Quotes include oracle data automatically.</Tooltip
							>
						</div>
					</svelte:fragment>
					<svelte:fragment slot="value">
						{#if data.oracleUrl.startsWith('https://') || data.oracleUrl.startsWith('http://')}
							<a
								href={data.oracleUrl}
								target="_blank"
								rel="noopener noreferrer"
								class="break-all text-blue-500 hover:underline"
							>
								{data.oracleUrl}
							</a>
						{:else}
							<span class="break-all text-gray-500">{data.oracleUrl}</span>
						{/if}
					</svelte:fragment>
				</CardProperty>
			{/if}

			{#each vaultTypesMap as { key, type, getter }}
				{@const filteredVaults = data.vaultsList.items.filter((vault) => vault.vaultType === type)}
				{@const vaultsListByType = data[getter]}
				{#if filteredVaults.length !== 0}
					<CardProperty>
						<svelte:fragment slot="key">
							<div class="flex items-center justify-between">
								<div class="flex items-center gap-x-2">
									{key}
									{#if type === 'inputOutput'}
										<InfoCircleOutline class="h-4 w-4" /><Tooltip
											>{'These vaults can be an input or an output for this order'}</Tooltip
										>{/if}
								</div>
								{#if onWithdrawAll}
									<Button
										color="light"
										size="xs"
										on:click={() =>
											vaultsListByType && onWithdrawAll(raindexClient, vaultsListByType)}
										disabled={!vaultsListByType ||
											vaultsListByType.getWithdrawableVaults()?.value?.length === 0}
										data-testid="withdraw-all-button"
									>
										<ArrowUpFromBracketOutline size="xs" class="mr-2" />
										Withdraw all
									</Button>
								{/if}
							</div>
						</svelte:fragment>
						<svelte:fragment slot="value">
							<div class="mt-2 space-y-2">
								{#each filteredVaults as vault}
									<ButtonVaultLink tokenVault={vault} {chainId} {raindexAddress}>
										<svelte:fragment slot="buttons">
											{#if matchesAccount(vault.owner)}
												<div class="flex gap-1">
													<Button
														color="light"
														size="xs"
														data-testid="deposit-button"
														title="Deposit into vault"
														aria-label="Deposit into vault"
														on:click={() => onDeposit(raindexClient, vault)}
													>
														<ArrowDownToBracketOutline size="xs" />
													</Button>
													<Button
														color="light"
														size="xs"
														data-testid="withdraw-button"
														title="Withdraw from vault"
														aria-label="Withdraw from vault"
														on:click={() => onWithdraw(raindexClient, vault)}
													>
														<ArrowUpFromBracketOutline size="xs" />
													</Button>
												</div>
											{/if}
										</svelte:fragment>
									</ButtonVaultLink>
								{/each}
							</div>
						</svelte:fragment>
					</CardProperty>
				{/if}
			{/each}
			{#if onWithdrawAll}
				<Button
					size="xs"
					on:click={() =>
						$orderDetailQuery.data?.vaultsList &&
						onWithdrawAll(raindexClient, $orderDetailQuery.data.vaultsList)}
					disabled={!$orderDetailQuery.data?.vaultsList ||
						$orderDetailQuery.data?.vaultsList?.getWithdrawableVaults()?.value?.length === 0}
					data-testid="withdraw-all-button"
				>
					<ArrowUpFromBracketOutline size="xs" class="mr-2" />
					Withdraw all vaults
				</Button>
			{/if}
		</div>
	</svelte:fragment>
	<svelte:fragment slot="chart" let:data>
		<OrderTradesChart order={data} {lightweightChartsTheme} />
	</svelte:fragment>
	<svelte:fragment slot="below" let:data>
		<TanstackOrderQuote order={data} {handleQuoteDebugModal} />
		<Tabs
			style="underline"
			contentClass="mt-4"
			defaultClass="flex flex-wrap space-x-2 rtl:space-x-reverse mt-4 list-none"
		>
			{#if data.dotrainSource}
				<TabItem title="Dotrain">
					<div class="mb-8 overflow-hidden rounded-lg border dark:border-none">
						<CodeMirrorRainlang
							rainlangText={data.dotrainSource}
							codeMirrorTheme={$codeMirrorTheme}
							{codeMirrorDisabled}
							{codeMirrorStyles}
						></CodeMirrorRainlang>
					</div>
				</TabItem>
			{/if}
			<TabItem title="On-chain Rainlang">
				<div class="mb-8 overflow-hidden rounded-lg border dark:border-none">
					<CodeMirrorRainlang
						rainlangText={data.rainlang}
						codeMirrorTheme={$codeMirrorTheme}
						{codeMirrorDisabled}
						{codeMirrorStyles}
					></CodeMirrorRainlang>
				</div>
			</TabItem>
			{#if data.orderBuilderState}
				<TabItem title="Builder State">
					<div class="mb-4">
						<div class="overflow-auto rounded-lg border bg-gray-50 p-4 dark:bg-gray-800">
							<pre class="text-sm" data-testid="builder-state-json">
								{formatBuilderState(data.orderBuilderState)}
							</pre>
						</div>
					</div>
				</TabItem>
			{/if}
			<TabItem open title="Trades">
				<OrderTradesListTable order={data} {handleDebugTradeModal} {rpcs} />
			</TabItem>
			<TabItem title="Volume">
				<OrderVaultsVolTable order={data} />
			</TabItem>
			<TabItem title="APY">
				<div>TODO: Issue #1989</div>
				<!-- <OrderApy order={data} /> -->
			</TabItem>
		</Tabs>
	</svelte:fragment>
</TanstackPageContentDetail>
