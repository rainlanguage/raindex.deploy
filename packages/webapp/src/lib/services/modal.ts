import DepositModal from '$lib/components/DepositModal.svelte';
import WithdrawModal from '$lib/components/WithdrawModal.svelte';
import TransactionConfirmationModal from '$lib/components/TransactionConfirmationModal.svelte';
import TakeOrderModal from '$lib/components/TakeOrderModal.svelte';
import {
	DisclaimerModal,
	type TransactionConfirmationProps,
	type DisclaimerModalProps,
	type VaultActionModalProps
} from '@rainlanguage/ui-components';
import WithdrawAllModal from '../components/WithdrawAllModal.svelte';
import type { WithdrawAllModalProps } from './handleVaultsWithdrawAll';
import type { RaindexOrder, RaindexOrderQuote, TakeOrdersMode } from '@rainlanguage/raindex';

export const handleDepositModal = (props: VaultActionModalProps) => {
	new DepositModal({ target: document.body, props });
};

export const handleWithdrawModal = (props: VaultActionModalProps) => {
	new WithdrawModal({ target: document.body, props });
};

export const handleWithdrawAllModal = (props: WithdrawAllModalProps) => {
	new WithdrawAllModal({ target: document.body, props });
};

export type TransactionConfirmationModalResult = {
	success: boolean;
	hash?: string;
};

export const handleTransactionConfirmationModal = (
	props: TransactionConfirmationProps,
	options?: { timeout?: number }
): Promise<TransactionConfirmationModalResult> => {
	return new Promise((resolve) => {
		const originalOnConfirm = props.args.onConfirm;
		let modalResolved = false;

		const finish = (result: TransactionConfirmationModalResult) => {
			if (modalResolved) return;
			modalResolved = true;
			resolve(result);
		};

		// Wrap the onConfirm to resolve our promise
		props.args.onConfirm = (hash) => {
			originalOnConfirm(hash);
			finish({ success: true, hash });
		};

		// Honor closeOnConfirm from the caller. Take-order and withdraw pass false so
		// the "Transaction submitted" state is visible; the old override to true hid
		// confirmation even when the tx succeeded on-chain.
		const modal = new TransactionConfirmationModal({
			target: document.body,
			props: {
				...props,
				onClosed: () => {
					finish({ success: false });
					cleanup();
				}
			}
		});

		const cleanup = () => {
			clearInterval(checkDismissal);
			if (timeoutId !== undefined) clearTimeout(timeoutId);
			if (!modal.$$.destroyed) {
				modal.$destroy();
			}
		};

		// Check periodically if modal was dismissed without an onClosed callback
		const checkDismissal = setInterval(() => {
			if (!modal.$$.ctx || modal.$$.destroyed) {
				finish({ success: false });
				clearInterval(checkDismissal);
			}
		}, 500);

		const timeoutId =
			options?.timeout !== undefined
				? setTimeout(() => {
						finish({ success: false });
						cleanup();
					}, options.timeout)
				: undefined;
	});
};

export const handleDisclaimerModal = (props: DisclaimerModalProps) => {
	new DisclaimerModal({ target: document.body, props });
};

export interface TakeOrderSubmitParams {
	quote: RaindexOrderQuote;
	mode: TakeOrdersMode;
	amount: string;
	priceCap: string;
}

export interface TakeOrderModalProps {
	open: boolean;
	order: RaindexOrder;
	onSubmit: (params: TakeOrderSubmitParams) => Promise<boolean>;
}

export type HandleTakeOrderModal = (props: TakeOrderModalProps) => void;

export const handleTakeOrderModal: HandleTakeOrderModal = (props: TakeOrderModalProps) => {
	new TakeOrderModal({ target: document.body, props });
};
