<script lang="ts">
	import { Modal, Spinner, Button } from 'flowbite-svelte';
	import type { TransactionConfirmationProps } from '@rainlanguage/ui-components';
	import { match, P } from 'ts-pattern';
	import {
		handleWalletConfirmation,
		type WalletConfirmationState
	} from '../services/handleWalletConfirmation';

	export let open: boolean = false;
	export let modalTitle: string;
	export let closeOnConfirm: boolean = false;
	export let args: TransactionConfirmationProps['args'];
	export let onClosed: (() => void) | undefined = undefined;

	let confirmationState: WalletConfirmationState = { status: 'awaiting_confirmation' };
	let autoCloseTimeout: ReturnType<typeof setTimeout> | undefined;
	let closed = false;

	async function init() {
		confirmationState = { status: 'awaiting_confirmation' };
		clearAutoCloseTimeout();
		const result = await handleWalletConfirmation(args);
		confirmationState = result.state;
		if (closeOnConfirm && confirmationState.status === 'confirmed') {
			closeModal();
		}
	}

	function clearAutoCloseTimeout() {
		if (autoCloseTimeout) {
			clearTimeout(autoCloseTimeout);
			autoCloseTimeout = undefined;
		}
	}

	$: if (open && args) {
		init();
	}

	// Auto-close modal after 2 seconds when transaction is confirmed so the user
	// can see the success state. Approvals that set closeOnConfirm skip this.
	$: if (confirmationState.status === 'confirmed' && open && !autoCloseTimeout && !closeOnConfirm) {
		autoCloseTimeout = setTimeout(() => {
			closeModal();
		}, 2000);
	}

	function closeModal() {
		if (closed) return;
		closed = true;
		clearAutoCloseTimeout();
		open = false;
		onClosed?.();
	}

	function handleDismiss() {
		closeModal();
	}
</script>

<Modal size="sm" class="bg-opacity-90 backdrop-blur-sm" {open} data-testid="transaction-modal">
	{@const ui = match(confirmationState)
		.with({ status: 'awaiting_confirmation' }, () => ({
			iconType: 'spinner',
			title: modalTitle,
			description: 'Please confirm this transaction in your wallet.',
			showDismiss: true
		}))
		.with({ status: 'confirmed' }, () => ({
			iconType: 'success',
			title: 'Transaction submitted',
			description: 'Transaction has been submitted to the network.',
			showDismiss: true
		}))
		.with({ status: 'rejected' }, (state) => ({
			iconType: 'error',
			title: 'Confirmation failed',
			description: state.reason,
			showDismiss: true
		}))
		.with({ status: 'error' }, (state) => ({
			iconType: 'error',
			title: 'Confirmation failed',
			description: state.reason,
			showDismiss: true
		}))
		.otherwise(() => null)}

	{#if ui}
		<div class="flex flex-col items-center justify-center gap-2 p-4">
			<div
				class={match(confirmationState)
					.with(
						{ status: P.union('rejected', 'error') },
						() =>
							'mb-4 flex h-16 w-16 items-center justify-center rounded-full border-2 border-red-400 bg-red-100 dark:bg-red-900'
					)
					.with(
						{ status: 'confirmed' },
						() =>
							'mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-green-100 dark:bg-green-900'
					)
					.otherwise(
						() =>
							'mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-blue-100 dark:bg-blue-900'
					)}
			>
				{#if ui.iconType === 'spinner'}
					<Spinner color="blue" aria-label="Pending confirmation" size={10} />
				{:else if ui.iconType === 'success'}
					<h1 class="text-lg md:text-2xl" aria-label="Success">✅</h1>
				{:else if ui.iconType === 'error'}
					<h1 class="text-lg md:text-2xl" aria-label="Error">❌</h1>
				{/if}
			</div>

			<div class="flex flex-col gap-4 text-center">
				<div class="flex flex-col">
					<p
						class="w-full whitespace-pre-wrap break-words text-center text-lg font-semibold text-gray-900 dark:text-white"
					>
						{ui.title}
					</p>
					<p
						class="w-full whitespace-pre-wrap break-words text-center text-sm font-normal text-gray-900 dark:text-white"
					>
						{ui.description}
					</p>
				</div>
			</div>

			{#if ui.showDismiss}
				<Button on:click={handleDismiss} aria-label="Close transaction modal">Dismiss</Button>
			{/if}
		</div>
	{/if}
</Modal>
