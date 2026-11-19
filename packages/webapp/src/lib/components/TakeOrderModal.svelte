<script lang="ts">
	import { Modal, Button, Select, Label, Radio, Alert, Spinner, Toggle } from 'flowbite-svelte';
	import {
		InputTokenAmount,
		WalletConnect,
		Refresh,
		DEFAULT_REFRESH_INTERVAL,
		validateAmount
	} from '@rainlanguage/ui-components';
	import { onDestroy } from 'svelte';
	import { appKitModal, connected, signerAddress } from '$lib/stores/wagmi';
	import { takeOrderMaxAmount } from '$lib/services/takeOrderMaxAmount';
	import {
		TAKE_ORDER_PRICE_CAP_BUFFER_LABEL,
		formatBufferedPriceCap
	} from '$lib/services/takeOrderPriceCap';
	import { fade } from 'svelte/transition';
	import {
		Float,
		type Hex,
		type RaindexOrder,
		type RaindexOrderQuote,
		type TakeOrdersMode,
		type TakeOrderEstimate
	} from '@rainlanguage/raindex';

	export let open: boolean;
	export let order: RaindexOrder;
	export let onSubmit: (params: {
		quote: RaindexOrderQuote;
		mode: TakeOrdersMode;
		amount: string;
		priceCap: string;
	}) => Promise<boolean>;

	type Direction = 'buy' | 'sell';

	let selectedPairIndex: number = 0;
	let amount: Float = Float.parse('0').value as Float;
	let direction: Direction = 'buy';
	let exact: boolean = false;
	let priceCap: string = '';
	let priceCapFromCurrentQuote = false;
	let errorMessage = '';
	let isLoadingQuotes = false;
	let isFetchingQuotes = false;
	let quotes: RaindexOrderQuote[] = [];
	let refreshInterval: ReturnType<typeof setInterval> | null = null;

	const loadQuotes = async (isRefresh = false) => {
		if (isRefresh) {
			isFetchingQuotes = true;
		} else {
			isLoadingQuotes = true;
		}

		try {
			const result = await order.getQuotes();
			if (result.error) {
				if (!isRefresh) {
					errorMessage = result.error.readableMsg;
				}
				return;
			}
			quotes = result.value.filter((q: RaindexOrderQuote) => q.success && q.data);
			if (quotes.length > 0) {
				errorMessage = '';
			}
			if (selectedPairIndex >= quotes.length) {
				selectedPairIndex = 0;
			}
			if (quotes.length === 0 && !isRefresh) {
				errorMessage = 'No valid quotes available for this order';
			}
		} catch (e) {
			if (!isRefresh) {
				errorMessage = e instanceof Error ? e.message : 'Failed to load quotes';
			}
		} finally {
			isLoadingQuotes = false;
			isFetchingQuotes = false;
		}
	};

	const refreshQuotes = () => {
		loadQuotes(true);
	};

	const startRefreshInterval = () => {
		if (refreshInterval) clearInterval(refreshInterval);
		refreshInterval = setInterval(() => {
			loadQuotes(true);
		}, DEFAULT_REFRESH_INTERVAL);
	};

	const stopRefreshInterval = () => {
		if (refreshInterval) {
			clearInterval(refreshInterval);
			refreshInterval = null;
		}
	};

	$: if (open && order && $signerAddress) {
		loadQuotes();
		startRefreshInterval();
	} else {
		stopRefreshInterval();
	}

	onDestroy(() => {
		stopRefreshInterval();
	});

	$: mode = ((): TakeOrdersMode => {
		if (direction === 'buy') {
			return exact ? 'buyExact' : 'buyUpTo';
		} else {
			return exact ? 'spendExact' : 'spendUpTo';
		}
	})();

	function handleClose() {
		open = false;
		amount = Float.parse('0').value as Float;
		direction = 'buy';
		exact = false;
		priceCap = '';
		priceCapFromCurrentQuote = false;
		errorMessage = '';
		estimateResult = null;
		selectedPairIndex = 0;
	}

	$: selectedQuote = quotes[selectedPairIndex];
	$: maxOutputHex = selectedQuote?.data?.maxOutput as unknown as Hex | undefined;
	$: maxInputHex = selectedQuote?.data?.maxInput as unknown as Hex | undefined;
	$: ratioHex = selectedQuote?.data?.ratio as unknown as Hex | undefined;
	$: maxOutput = (() => {
		if (!maxOutputHex) return undefined;
		const parsed = Float.fromHex(maxOutputHex);
		return parsed.error ? undefined : (parsed.value as Float);
	})();
	$: maxInput = (() => {
		if (!maxInputHex) return undefined;
		const parsed = Float.fromHex(maxInputHex);
		return parsed.error ? undefined : (parsed.value as Float);
	})();
	$: ratio = (() => {
		if (!ratioHex) return undefined;
		const parsed = Float.fromHex(ratioHex);
		return parsed.error ? undefined : (parsed.value as Float);
	})();
	$: formattedRatio = selectedQuote?.data?.formattedRatio ?? '-';
	$: formattedMaxOutput = selectedQuote?.data?.formattedMaxOutput ?? '-';
	$: formattedMaxInput = selectedQuote?.data?.formattedMaxInput ?? '-';
	$: outputSymbol = selectedQuote?.pair?.pairName?.split('/')[1] ?? 'tokens';
	$: inputSymbol = selectedQuote?.pair?.pairName?.split('/')[0] ?? 'tokens';

	type VaultAccountBalance = {
		getAccountBalance: (account: string) => Promise<{
			value?: { balance: Float; formattedBalance: string };
			error?: { readableMsg: string };
		}>;
	};

	let walletInputBalance: Float | undefined = undefined;
	let isLoadingWalletBalance = false;
	let walletBalanceRequestId = 0;

	async function loadWalletInputBalance(account: string, inputIndex: number) {
		const requestId = ++walletBalanceRequestId;
		isLoadingWalletBalance = true;
		walletInputBalance = undefined;
		try {
			const vaults = (order.inputsList?.items ?? []) as VaultAccountBalance[];
			const vault = vaults[inputIndex];
			if (!vault?.getAccountBalance) return;
			const result = await vault.getAccountBalance(account);
			if (requestId !== walletBalanceRequestId) return;
			if (result.error || !result.value) return;
			walletInputBalance = result.value.balance;
		} finally {
			if (requestId === walletBalanceRequestId) {
				isLoadingWalletBalance = false;
			}
		}
	}

	$: if (open && $signerAddress && selectedQuote) {
		loadWalletInputBalance($signerAddress, selectedQuote.pair.inputIndex);
	}

	$: maxAmount = (() => {
		const quoteCap = direction === 'buy' ? maxOutput : maxInput;
		if (!quoteCap || isLoadingWalletBalance) return undefined;
		if (!walletInputBalance || !maxOutput || !maxInput || !ratio) return quoteCap;
		return takeOrderMaxAmount({
			direction,
			maxOutput,
			maxInput,
			ratio,
			walletInputBalance
		});
	})();
	$: formattedMaxAmount = direction === 'buy' ? formattedMaxOutput : formattedMaxInput;
	$: amountSymbol = direction === 'buy' ? outputSymbol : inputSymbol;

	let estimateResult: TakeOrderEstimate | null = null;

	function formatFloat(float: Float): string {
		const formatted = float.format().value;
		return typeof formatted === 'string' ? formatted : '-';
	}

	function updateEstimate() {
		if (!selectedQuote || !amount) {
			estimateResult = null;
			return;
		}

		try {
			const zero = Float.parse('0').value as Float;
			if (amount.lte(zero).value) {
				estimateResult = null;
				return;
			}

			const amountStr = amount.format();
			if (amountStr.error) {
				estimateResult = null;
				return;
			}

			const isBuy = direction === 'buy';
			const result = order.estimateTakeOrder(selectedQuote, isBuy, amountStr.value as string);

			if (result.error) {
				estimateResult = null;
				return;
			}

			estimateResult = result.value as TakeOrderEstimate;
		} catch {
			estimateResult = null;
		}
	}

	$: if (selectedQuote && amount && direction !== undefined) {
		updateEstimate();
	}

	$: priceCapDecimalsError = validateAmount(priceCap).errorMessage;

	function fillBufferedCurrentPrice() {
		if (!ratio) return;
		const formatted = formatBufferedPriceCap(ratio);
		if (!formatted) return;
		priceCap = formatted;
		priceCapFromCurrentQuote = true;
	}

	function handlePriceCapInput() {
		priceCapFromCurrentQuote = false;
	}

	$: effectivePriceCap = (() => {
		if (priceCap && priceCap.trim() !== '') {
			const parsed = Float.parse(priceCap);
			if (!parsed.error) return parsed.value as Float;
		}
		return null;
	})();

	$: isAmountValid = (() => {
		try {
			const zero = Float.parse('0').value as Float;
			return amount.gt(zero).value;
		} catch {
			return false;
		}
	})();

	$: exceedsMax = (() => {
		if (!maxAmount || !isAmountValid) return false;
		if (exact) {
			try {
				return amount.gt(maxAmount).value;
			} catch {
				return false;
			}
		}
		return false;
	})();

	$: canSubmit =
		isAmountValid &&
		!exceedsMax &&
		!priceCapDecimalsError &&
		selectedQuote &&
		effectivePriceCap &&
		$signerAddress;

	async function handleSubmit() {
		if (!selectedQuote || !effectivePriceCap) return;

		const priceCapStr = effectivePriceCap.format();
		if (priceCapStr.error) {
			errorMessage = 'Failed to format price cap';
			return;
		}

		const amountStr = amount.format();
		if (amountStr.error) {
			errorMessage = 'Failed to format amount';
			return;
		}

		const shouldClose = await onSubmit({
			quote: selectedQuote,
			mode: mode as TakeOrdersMode,
			amount: amountStr.value as string,
			priceCap: priceCapStr.value as string
		});

		if (shouldClose) {
			handleClose();
		}
	}

	$: pairOptions = quotes.map((q, i) => ({
		value: i,
		name: q.pair.pairName
	}));
</script>

<Modal bind:open autoclose={false} size="lg" data-testid="take-order-modal">
	<div class="space-y-4">
		<h3 class="text-xl font-medium" data-testid="modal-title">Take Order</h3>
		<p class="text-sm text-gray-500 dark:text-gray-400">
			Trade directly against this order at its current price. Specify how much you want to buy or
			sell and set your maximum acceptable price.
		</p>

		{#if !$signerAddress}
			<p>Connect your wallet to continue.</p>
			<div class="flex justify-end gap-2 pt-2">
				<Button color="alternative" on:click={handleClose}>Cancel</Button>
				<WalletConnect {appKitModal} {connected} />
			</div>
		{:else if isLoadingQuotes}
			<div class="flex items-center justify-center py-8">
				<Spinner size="8" />
				<span class="ml-2">Loading quotes...</span>
			</div>
		{:else if quotes.length === 0}
			<Alert color="red">
				{errorMessage || 'No valid quotes available for this order'}
			</Alert>
			<div class="flex justify-end">
				<Button color="alternative" on:click={handleClose}>Close</Button>
			</div>
		{:else}
			<div in:fade class="space-y-4">
				{#if quotes.length > 1}
					<div>
						<Label for="pair-select" class="mb-2 block">Token Pair</Label>
						<Select
							id="pair-select"
							items={pairOptions}
							bind:value={selectedPairIndex}
							data-testid="pair-selector"
						/>
					</div>
				{:else}
					<div class="rounded-lg bg-gray-50 p-3 dark:bg-gray-700">
						<span class="text-sm text-gray-500 dark:text-gray-400">Pair:</span>
						<span class="ml-2 font-medium">{selectedQuote?.pair?.pairName}</span>
					</div>
				{/if}

				<div class="rounded-lg bg-gray-50 p-4 dark:bg-gray-700">
					<div class="mb-2 flex items-center justify-between">
						<h4 class="text-sm font-medium text-gray-700 dark:text-gray-300">Current Quote</h4>
						<Refresh
							class="h-5 w-5 cursor-pointer text-gray-400 hover:text-gray-600 dark:hover:text-gray-300"
							title="Refresh quote"
							ariaLabel="Refresh quote"
							on:click={refreshQuotes}
							spin={isFetchingQuotes}
						/>
					</div>
					<div class="grid grid-cols-3 gap-4">
						<div class="min-w-0">
							<p class="text-xs text-gray-500 dark:text-gray-400">Max Output</p>
							<p
								class="truncate font-medium"
								data-testid="max-output"
								title="{formattedMaxOutput} {outputSymbol}"
							>
								{formattedMaxOutput}
								{outputSymbol}
							</p>
						</div>
						<div class="min-w-0">
							<p class="text-xs text-gray-500 dark:text-gray-400">Max Input</p>
							<p
								class="truncate font-medium"
								data-testid="max-input"
								title="{formattedMaxInput} {inputSymbol}"
							>
								{formattedMaxInput}
								{inputSymbol}
							</p>
						</div>
						<div class="min-w-0">
							<p class="text-xs text-gray-500 dark:text-gray-400">Current Price</p>
							<p
								class="truncate font-medium"
								data-testid="current-price"
								title="{formattedRatio} {inputSymbol}/{outputSymbol}"
							>
								{formattedRatio}
								{inputSymbol}/{outputSymbol}
							</p>
						</div>
					</div>
				</div>

				<div>
					<Label class="mb-2 block">Direction</Label>
					<div class="flex gap-4">
						<Radio bind:group={direction} value="buy" data-testid="direction-buy">Buy</Radio>
						<Radio bind:group={direction} value="sell" data-testid="direction-sell">Sell</Radio>
					</div>
					<p class="mt-1 text-xs text-gray-500 dark:text-gray-400">
						{#if direction === 'buy'}
							Specify how much {outputSymbol} you want to receive
						{:else}
							Specify how much {inputSymbol} you want to spend
						{/if}
					</p>
				</div>

				<div>
					<div class="mb-2 flex items-center justify-between">
						<Label>
							{direction === 'buy' ? 'Amount to Buy' : 'Amount to Sell'} ({amountSymbol})
						</Label>
						<div class="flex items-center gap-2">
							<p class="text-xs text-gray-500 dark:text-gray-400">
								{#if exact}
									Transaction fails if exact amount cannot be filled
								{:else}
									Allow partial fills up to your target amount
								{/if}
							</p>
							<Toggle bind:checked={exact} data-testid="exact-toggle" size="small" class="-mr-2" />
						</div>
					</div>
					<InputTokenAmount bind:value={amount} symbol={amountSymbol} maxValue={maxAmount} />
					{#if exceedsMax}
						<p class="mt-1 text-sm text-red-500" data-testid="amount-error">
							Amount exceeds maximum available ({formattedMaxAmount}
							{amountSymbol})
						</p>
					{/if}
				</div>

				<div>
					<Label for="price-cap" class="mb-2 block">
						Max Price
						<span class="text-xs text-gray-500">
							- {inputSymbol} per {outputSymbol}
						</span>
					</Label>
					<div class="relative">
						<input
							id="price-cap"
							type="text"
							class="block w-full rounded-lg border-gray-300 bg-gray-50 p-2.5 pr-32 text-sm text-gray-900 focus:border-primary-500 focus:ring-primary-500 dark:border-gray-500 dark:bg-gray-600 dark:text-white dark:placeholder-gray-400 dark:focus:border-primary-500 dark:focus:ring-primary-500"
							placeholder={`Enter max price in ${inputSymbol} per ${outputSymbol}`}
							bind:value={priceCap}
							on:input={handlePriceCapInput}
							data-testid="price-cap-input"
						/>
						{#if ratio}
							<div class="absolute right-2 top-0 flex h-10 items-center">
								<Button
									color="blue"
									class="px-2 py-1"
									size="xs"
									pill
									on:click={fillBufferedCurrentPrice}
									data-testid="price-cap-current-button"
								>
									CURRENT +{TAKE_ORDER_PRICE_CAP_BUFFER_LABEL}
								</Button>
							</div>
						{/if}
					</div>
					{#if priceCapFromCurrentQuote}
						<p class="mt-1 text-xs text-gray-500 dark:text-gray-400" data-testid="price-cap-buffer-hint">
							Includes a {TAKE_ORDER_PRICE_CAP_BUFFER_LABEL} buffer above the current quote so the next
							block can still fill.
						</p>
					{/if}
					{#if priceCapDecimalsError}
						<p class="mt-1 text-sm text-red-500" data-testid="price-cap-error">
							{priceCapDecimalsError}
						</p>
					{/if}
				</div>

				{#if isAmountValid && estimateResult}
					<div
						class="rounded-lg border border-blue-200 bg-blue-50 p-4 dark:border-blue-800 dark:bg-blue-900/30"
					>
						<h4 class="mb-2 text-sm font-medium text-blue-800 dark:text-blue-300">
							Expected Transaction
						</h4>
						<div class="grid grid-cols-2 gap-4 text-sm">
							<div class="min-w-0">
								<p class="text-blue-600 dark:text-blue-400">You Spend</p>
								<p
									class="truncate font-medium text-blue-900 dark:text-white"
									data-testid="expected-spend"
									title="{direction === 'sell' && exact ? '' : '~'}{formatFloat(
										estimateResult.expectedSpend
									)} {inputSymbol}"
								>
									{direction === 'sell' && exact ? '' : '~'}
									{formatFloat(estimateResult.expectedSpend)}
									{inputSymbol}
								</p>
							</div>
							<div class="min-w-0">
								<p class="text-blue-600 dark:text-blue-400">You Receive</p>
								<p
									class="truncate font-medium text-blue-900 dark:text-white"
									data-testid="expected-receive"
									title="{direction === 'buy' && exact ? '' : '~'}{formatFloat(
										estimateResult.expectedReceive
									)} {outputSymbol}"
								>
									{direction === 'buy' && exact ? '' : '~'}
									{formatFloat(estimateResult.expectedReceive)}
									{outputSymbol}
								</p>
							</div>
						</div>

						{#if estimateResult.isPartial}
							<div class="mt-3 text-sm text-amber-600 dark:text-amber-400">
								{#if exact}
									Order only has {formatFloat(estimateResult.expectedReceive)}
									{outputSymbol} available, exact fill might not be possible
								{:else}
									Order will be partially filled ({formatFloat(estimateResult.expectedReceive)} of {amount.format()
										.value ?? '-'}
									{direction === 'buy' ? outputSymbol : inputSymbol})
								{/if}
							</div>
						{:else if !exact}
							<p class="mt-2 text-xs text-blue-600 dark:text-blue-400">
								Partial fills allowed - you may receive less than shown
							</p>
						{/if}
					</div>
				{/if}

				{#if errorMessage}
					<Alert color="red" data-testid="error-message">{errorMessage}</Alert>
				{/if}

				<div class="flex justify-end gap-2 pt-2">
					<Button color="alternative" on:click={handleClose}>Cancel</Button>
					<Button
						color="blue"
						disabled={!canSubmit}
						on:click={handleSubmit}
						data-testid="submit-button"
					>
						Take Order
					</Button>
				</div>
			</div>
		{/if}
	</div>
</Modal>
