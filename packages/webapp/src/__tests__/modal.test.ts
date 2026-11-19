import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { TransactionConfirmationProps } from '@rainlanguage/ui-components';

const destroy = vi.fn();
let lastModalProps: Record<string, unknown> | undefined;
let lastModalInstance: {
	$$: { ctx: boolean; destroyed: boolean };
	$destroy: typeof destroy;
};

vi.mock('$lib/components/TransactionConfirmationModal.svelte', () => ({
	default: vi.fn().mockImplementation((options: { props: Record<string, unknown> }) => {
		lastModalProps = options.props;
		lastModalInstance = {
			$$: { ctx: true, destroyed: false },
			$destroy: destroy.mockImplementation(() => {
				lastModalInstance.$$.destroyed = true;
			})
		};
		return lastModalInstance;
	})
}));

vi.mock('$lib/components/DepositModal.svelte', () => ({ default: vi.fn() }));
vi.mock('$lib/components/WithdrawModal.svelte', () => ({ default: vi.fn() }));
vi.mock('$lib/components/TakeOrderModal.svelte', () => ({ default: vi.fn() }));
vi.mock('$lib/components/WithdrawAllModal.svelte', () => ({ default: vi.fn() }));
vi.mock('@rainlanguage/ui-components', () => ({
	DisclaimerModal: vi.fn()
}));

const { handleTransactionConfirmationModal } = await import('$lib/services/modal');

const baseProps = (): TransactionConfirmationProps => ({
	open: true,
	modalTitle: 'Taking order for TEST',
	closeOnConfirm: false,
	args: {
		chainId: 8453,
		toAddress: '0x1234567890123456789012345678901234567890',
		calldata: '0xabc',
		onConfirm: vi.fn()
	}
});

describe('handleTransactionConfirmationModal', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.clearAllMocks();
		lastModalProps = undefined;
	});

	afterEach(() => {
		vi.clearAllTimers();
		vi.useRealTimers();
	});

	it('honors closeOnConfirm from the caller so take-order can show submission confirmation', () => {
		void handleTransactionConfirmationModal(baseProps());

		expect(lastModalProps?.closeOnConfirm).toBe(false);
	});

	it('does not destroy the modal after 30s while the wallet confirmation is still pending', async () => {
		const pending = handleTransactionConfirmationModal(baseProps());

		await vi.advanceTimersByTimeAsync(30_000);

		expect(destroy).not.toHaveBeenCalled();

		const onConfirm = lastModalProps?.args as TransactionConfirmationProps['args'] | undefined;
		onConfirm?.onConfirm(
			'0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890'
		);

		await expect(pending).resolves.toEqual({
			success: true,
			hash: '0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890'
		});
	});

	it('still times out when an explicit timeout is provided', async () => {
		const pending = handleTransactionConfirmationModal(baseProps(), { timeout: 5_000 });

		await vi.advanceTimersByTimeAsync(5_000);

		await expect(pending).resolves.toEqual({ success: false });
		expect(destroy).toHaveBeenCalled();
	});
});
