import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import TakeOrderModal from '$lib/components/TakeOrderModal.svelte';
import type { ComponentProps } from 'svelte';
import { Float, type RaindexOrder } from '@rainlanguage/raindex';

type ModalProps = ComponentProps<TakeOrderModal>;

function floatToHex(f: Float): string {
	const bigint = f.toBigint();
	return '0x' + bigint.toString(16).padStart(64, '0');
}

const { mockAppKitModalStore, mockConnectedStore, mockSignerAddressStore } = await vi.hoisted(
	() => import('../lib/__mocks__/stores')
);

vi.mock('../lib/stores/wagmi', () => ({
	appKitModal: mockAppKitModalStore,
	connected: mockConnectedStore,
	signerAddress: mockSignerAddressStore
}));

vi.mock('@rainlanguage/ui-components', async (importOriginal) => {
	const actual = await importOriginal<typeof import('@rainlanguage/ui-components')>();
	const MockComponent = (await import('../lib/__mocks__/MockComponent.svelte')).default;
	return {
		...actual,
		WalletConnect: MockComponent
	};
});

const MOCK_RAINDEX_ADDRESS = '0x1234567890123456789012345678901234567890';
const MOCK_SIGNER_ADDRESS = '0x9876543210987654321098765432109876543210';

describe('TakeOrderModal', () => {
	const mockQuotes = [
		{
			pair: {
				pairName: 'USDC/ETH',
				inputIndex: 0,
				outputIndex: 0
			},
			blockNumber: 12345678,
			success: true,
			data: {
				maxOutput: floatToHex(Float.parse('100').value as Float),
				maxInput: floatToHex(Float.parse('50').value as Float),
				ratio: floatToHex(Float.parse('0.0005').value as Float),
				inverseRatio: floatToHex(Float.parse('2000').value as Float),
				formattedMaxOutput: '100',
				formattedMaxInput: '50',
				formattedRatio: '0.0005',
				formattedInverseRatio: '2000'
			}
		}
	];

	const mockGetAccountBalance = vi.fn();

	const mockOrder = {
		id: '0xorderid',
		orderHash: '0xorderhash',
		chainId: 1,
		raindex: MOCK_RAINDEX_ADDRESS,
		inputsList: {
			items: [{ getAccountBalance: mockGetAccountBalance }]
		},
		getQuotes: vi.fn().mockResolvedValue({
			value: mockQuotes,
			error: undefined
		})
	} as unknown as RaindexOrder;

	const mockOnSubmit = vi.fn();

	const defaultProps: ModalProps = {
		open: true,
		order: mockOrder,
		onSubmit: mockOnSubmit
	};

	beforeEach(() => {
		vi.clearAllMocks();
		mockOnSubmit.mockClear();
		mockSignerAddressStore.mockSetSubscribeValue(MOCK_SIGNER_ADDRESS);
		mockGetAccountBalance.mockResolvedValue({
			value: {
				balance: Float.parse('1000').value as Float,
				formattedBalance: '1000'
			},
			error: undefined
		});
	});

	it('renders take order modal correctly', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('modal-title')).toHaveTextContent('Take Order');
		});
	});

	it('shows loading spinner while fetching quotes', async () => {
		vi.mocked(mockOrder.getQuotes).mockImplementation(
			() =>
				new Promise((resolve) =>
					setTimeout(() => resolve({ value: mockQuotes, error: undefined } as never), 100)
				)
		);

		render(TakeOrderModal, defaultProps);

		expect(screen.getByText('Loading quotes...')).toBeInTheDocument();
	});

	it('shows error message when quotes fail to load', async () => {
		vi.mocked(mockOrder.getQuotes).mockResolvedValue({
			value: undefined,
			error: { msg: 'Quote error', readableMsg: 'Failed to fetch quotes' }
		});

		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByText('Failed to fetch quotes')).toBeInTheDocument();
		});
	});

	it('shows error when no valid quotes are available', async () => {
		vi.mocked(mockOrder.getQuotes).mockResolvedValue({
			value: [],
			error: undefined
		});

		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByText('No valid quotes available for this order')).toBeInTheDocument();
		});
	});

	it('displays quote information after loading', async () => {
		vi.mocked(mockOrder.getQuotes).mockResolvedValue({
			value: mockQuotes as never,
			error: undefined
		});

		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('max-output')).toBeInTheDocument();
			expect(screen.getByTestId('max-input')).toBeInTheDocument();
			expect(screen.getByTestId('current-price')).toBeInTheDocument();
		});
	});

	it('shows hover title and aria-label on the refresh quote icon', async () => {
		vi.mocked(mockOrder.getQuotes).mockResolvedValue({
			value: mockQuotes as never,
			error: undefined
		});

		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			const refreshButton = screen.getByTestId('refresh-button');
			expect(refreshButton).toHaveAttribute('title', 'Refresh quote');
			expect(refreshButton).toHaveAttribute('aria-label', 'Refresh quote');
		});
	});

	it('shows pair selector when multiple quotes are available', async () => {
		const multiQuotes = [
			...mockQuotes,
			{
				pair: {
					pairName: 'DAI/ETH',
					inputIndex: 1,
					outputIndex: 0
				},
				blockNumber: 12345678,
				success: true,
				data: {
					maxOutput: floatToHex(Float.parse('500').value as Float),
					maxInput: floatToHex(Float.parse('250').value as Float),
					ratio: floatToHex(Float.parse('0.0006').value as Float),
					inverseRatio: floatToHex(Float.parse('1666').value as Float),
					formattedMaxOutput: '500',
					formattedMaxInput: '250',
					formattedRatio: '0.0006',
					formattedInverseRatio: '1666'
				}
			}
		];

		vi.mocked(mockOrder.getQuotes).mockResolvedValue({
			value: multiQuotes as never,
			error: undefined
		});

		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('pair-selector')).toBeInTheDocument();
		});
	});

	it('does not show pair selector for single quote', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.queryByTestId('pair-selector')).not.toBeInTheDocument();
		});
	});

	it('shows direction selection and exact toggle', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('direction-buy')).toBeInTheDocument();
			expect(screen.getByTestId('direction-sell')).toBeInTheDocument();
			expect(screen.getByTestId('exact-toggle')).toBeInTheDocument();
		});
	});

	it('shows price cap input', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('price-cap-input')).toBeInTheDocument();
		});
	});

	it('rejects a price cap with more than 67 decimals and keeps submit disabled', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('submit-button')).toBeInTheDocument();
		});

		const inputs = screen.getAllByRole('textbox');
		const amountInput = inputs[0];
		const priceCapInput = screen.getByTestId('price-cap-input');
		await fireEvent.input(amountInput, { target: { value: '10' } });
		// 68 decimal places, one more than the maximum of 67.
		await fireEvent.input(priceCapInput, {
			target: { value: '0.00151160732710359471234567890123456789012345678901234567890123456789' }
		});

		await waitFor(() => {
			expect(screen.getByTestId('price-cap-error')).toHaveTextContent(
				'Too many decimal places. A maximum of 67 decimal places is allowed.'
			);
		});
		expect(screen.getByTestId('submit-button')).toBeDisabled();
	});

	it('accepts a price cap with exactly 18 decimals and enables submit', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('submit-button')).toBeInTheDocument();
		});

		const inputs = screen.getAllByRole('textbox');
		const amountInput = inputs[0];
		const priceCapInput = screen.getByTestId('price-cap-input');
		await fireEvent.input(amountInput, { target: { value: '10' } });
		// 18 decimal places, exactly the maximum.
		await fireEvent.input(priceCapInput, { target: { value: '0.001511607327103594' } });

		await waitFor(() => {
			expect(screen.queryByTestId('price-cap-error')).not.toBeInTheDocument();
			expect(screen.getByTestId('submit-button')).not.toBeDisabled();
		});
	});

	it('shows submit button when connected and amount is valid', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('submit-button')).toBeInTheDocument();
		});

		const inputs = screen.getAllByRole('textbox');
		const amountInput = inputs[0];
		const priceCapInput = screen.getByTestId('price-cap-input');
		await fireEvent.input(amountInput, { target: { value: '10' } });
		await fireEvent.input(priceCapInput, { target: { value: '0.001' } });

		await waitFor(() => {
			expect(screen.getByTestId('submit-button')).not.toBeDisabled();
		});
	});

	it('disables submit button when amount is 0', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('submit-button')).toBeInTheDocument();
		});

		const inputs = screen.getAllByRole('textbox');
		const amountInput = inputs[0];
		await fireEvent.input(amountInput, { target: { value: '0' } });

		expect(screen.getByTestId('submit-button')).toBeDisabled();
	});

	it('shows expected transaction info when amount is entered', async () => {
		const mockOrderWithEstimate = {
			...mockOrder,
			estimateTakeOrder: vi.fn().mockReturnValue({
				value: {
					expectedSpend: Float.parse('0.005').value as Float,
					expectedReceive: Float.parse('10').value as Float,
					isPartial: false
				},
				error: undefined
			})
		} as unknown as RaindexOrder;

		render(TakeOrderModal, { ...defaultProps, order: mockOrderWithEstimate });

		await waitFor(() => {
			expect(screen.getByTestId('submit-button')).toBeInTheDocument();
		});

		const inputs = screen.getAllByRole('textbox');
		const amountInput = inputs[0];
		await fireEvent.input(amountInput, { target: { value: '10' } });

		await waitFor(() => {
			expect(screen.getByTestId('expected-spend')).toBeInTheDocument();
			expect(screen.getByTestId('expected-receive')).toBeInTheDocument();
		});
	});

	it('shows error when amount exceeds max available with exact toggle enabled', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('exact-toggle')).toBeInTheDocument();
		});

		const exactToggle = screen.getByTestId('exact-toggle');
		await fireEvent.click(exactToggle);

		const inputs = screen.getAllByRole('textbox');
		const amountInput = inputs[0];
		await fireEvent.input(amountInput, { target: { value: '200' } });

		await waitFor(() => {
			expect(screen.getByTestId('amount-error')).toBeInTheDocument();
		});
	});

	it('does not show error when amount exceeds max with exact toggle disabled', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('submit-button')).toBeInTheDocument();
		});

		const inputs = screen.getAllByRole('textbox');
		const amountInput = inputs[0];
		await fireEvent.input(amountInput, { target: { value: '200' } });

		await waitFor(() => {
			expect(screen.queryByTestId('amount-error')).not.toBeInTheDocument();
		});
	});

	it('calls onSubmit when submit button is clicked', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('submit-button')).toBeInTheDocument();
		});

		const inputs = screen.getAllByRole('textbox');
		const amountInput = inputs[0];
		const priceCapInput = screen.getByTestId('price-cap-input');
		await fireEvent.input(amountInput, { target: { value: '10' } });
		await fireEvent.input(priceCapInput, { target: { value: '0.001' } });

		const submitButton = screen.getByTestId('submit-button');
		await fireEvent.click(submitButton);

		await waitFor(() => {
			expect(mockOnSubmit).toHaveBeenCalledWith({
				quote: expect.objectContaining({
					pair: expect.objectContaining({
						pairName: 'USDC/ETH'
					})
				}),
				mode: 'buyUpTo',
				amount: '10',
				priceCap: '0.001'
			});
		});
	});

	it('closes modal on cancel button click', async () => {
		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByText('Cancel')).toBeInTheDocument();
		});

		const cancelButton = screen.getByText('Cancel');
		await fireEvent.click(cancelButton);
	});

	it('prompts to connect and does not fetch quotes when the wallet is disconnected', async () => {
		mockSignerAddressStore.mockSetSubscribeValue('');

		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByText('Connect your wallet to continue.')).toBeInTheDocument();
		});
		expect(screen.getByTestId('mock-component')).toBeInTheDocument();
		expect(screen.queryByTestId('submit-button')).not.toBeInTheDocument();
		expect(screen.queryByTestId('direction-buy')).not.toBeInTheDocument();
		expect(screen.queryByText('Loading quotes...')).not.toBeInTheDocument();
		expect(mockOrder.getQuotes).not.toHaveBeenCalled();
	});

	it('fetches quotes and shows the take form after the wallet connects', async () => {
		mockSignerAddressStore.mockSetSubscribeValue('');
		vi.mocked(mockOrder.getQuotes).mockResolvedValue({
			value: mockQuotes as never,
			error: undefined
		});

		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByText('Connect your wallet to continue.')).toBeInTheDocument();
		});
		expect(mockOrder.getQuotes).not.toHaveBeenCalled();

		mockSignerAddressStore.mockSetSubscribeValue(MOCK_SIGNER_ADDRESS);

		await waitFor(() => {
			expect(screen.getByTestId('submit-button')).toBeInTheDocument();
		});
		expect(screen.queryByText('Connect your wallet to continue.')).not.toBeInTheDocument();
		expect(mockOrder.getQuotes).toHaveBeenCalled();
	});

	it('shows submit button disabled initially', async () => {
		vi.mocked(mockOrder.getQuotes).mockResolvedValue({
			value: mockQuotes as never,
			error: undefined
		});

		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.getByTestId('submit-button')).toBeInTheDocument();
		});

		expect(screen.getByTestId('submit-button')).toBeDisabled();
	});

	it('filters out failed quotes', async () => {
		const mixedQuotes = [
			...mockQuotes,
			{
				pair: {
					pairName: 'FAILED/ETH',
					inputIndex: 1,
					outputIndex: 0
				},
				blockNumber: 12345678,
				success: false,
				data: undefined,
				error: { msg: 'Quote failed' }
			}
		];

		vi.mocked(mockOrder.getQuotes).mockResolvedValue({
			value: mixedQuotes as never,
			error: undefined
		});

		render(TakeOrderModal, defaultProps);

		await waitFor(() => {
			expect(screen.queryByText('FAILED/ETH')).not.toBeInTheDocument();
		});
	});

	describe('direction toggle and mode mapping', () => {
		it('updates labels when direction is toggled from buy to sell', async () => {
			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: mockQuotes as never,
				error: undefined
			});

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('direction-buy')).toBeInTheDocument();
			});

			expect(screen.getByText('Amount to Buy (ETH)')).toBeInTheDocument();

			const sellRadio = screen.getByTestId('direction-sell');
			await fireEvent.click(sellRadio);

			await waitFor(() => {
				expect(screen.getByText('Amount to Sell (USDC)')).toBeInTheDocument();
			});
		});

		it('shows partial fill warning when estimateResult.isPartial is true', async () => {
			const mockOrderWithPartialEstimate = {
				...mockOrder,
				estimateTakeOrder: vi.fn().mockReturnValue({
					value: {
						expectedSpend: Float.parse('0.005').value as Float,
						expectedReceive: Float.parse('8').value as Float,
						isPartial: true
					},
					error: undefined
				})
			} as unknown as RaindexOrder;

			render(TakeOrderModal, { ...defaultProps, order: mockOrderWithPartialEstimate });

			await waitFor(() => {
				expect(screen.getByTestId('submit-button')).toBeInTheDocument();
			});

			const inputs = screen.getAllByRole('textbox');
			const amountInput = inputs[0];
			await fireEvent.input(amountInput, { target: { value: '10' } });

			await waitFor(() => {
				expect(screen.getByText(/Order will be partially filled/)).toBeInTheDocument();
			});
		});

		it('maps mode correctly - buyExact when buy + exact toggle', async () => {
			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: mockQuotes as never,
				error: undefined
			});

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('submit-button')).toBeInTheDocument();
			});

			const buyRadio = screen.getByTestId('direction-buy');
			await fireEvent.click(buyRadio);

			const exactToggle = screen.getByTestId('exact-toggle');
			await fireEvent.click(exactToggle);

			const inputs = screen.getAllByRole('textbox');
			const amountInput = inputs[0];
			const priceCapInput = screen.getByTestId('price-cap-input');
			await fireEvent.input(amountInput, { target: { value: '10' } });
			await fireEvent.input(priceCapInput, { target: { value: '0.001' } });

			const submitButton = screen.getByTestId('submit-button');
			await fireEvent.click(submitButton);

			await waitFor(() => {
				expect(mockOnSubmit).toHaveBeenCalledWith(
					expect.objectContaining({
						mode: 'buyExact'
					})
				);
			});
		});

		it('maps mode correctly - spendUpTo when sell + no exact toggle', async () => {
			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: mockQuotes as never,
				error: undefined
			});

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('submit-button')).toBeInTheDocument();
			});

			const sellRadio = screen.getByTestId('direction-sell');
			await fireEvent.click(sellRadio);

			const inputs = screen.getAllByRole('textbox');
			const amountInput = inputs[0];
			const priceCapInput = screen.getByTestId('price-cap-input');
			await fireEvent.input(amountInput, { target: { value: '10' } });
			await fireEvent.input(priceCapInput, { target: { value: '0.001' } });

			const submitButton = screen.getByTestId('submit-button');
			await fireEvent.click(submitButton);

			await waitFor(() => {
				expect(mockOnSubmit).toHaveBeenCalledWith(
					expect.objectContaining({
						mode: 'spendUpTo'
					})
				);
			});
		});
	});

	describe('refresh logic', () => {
		it('calls getQuotes again when refresh icon is clicked', async () => {
			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: mockQuotes as never,
				error: undefined
			});

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('max-output')).toBeInTheDocument();
			});

			expect(mockOrder.getQuotes).toHaveBeenCalledTimes(1);

			const refreshButton = screen.getByTestId('refresh-button');
			await fireEvent.click(refreshButton);

			await waitFor(() => {
				expect(mockOrder.getQuotes).toHaveBeenCalledTimes(2);
			});
		});

		it('auto-refreshes quotes at the default interval (10s)', async () => {
			vi.useFakeTimers();

			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: mockQuotes as never,
				error: undefined
			});

			render(TakeOrderModal, defaultProps);

			await vi.advanceTimersByTimeAsync(0);

			expect(mockOrder.getQuotes).toHaveBeenCalledTimes(1);

			await vi.advanceTimersByTimeAsync(10000);

			expect(mockOrder.getQuotes).toHaveBeenCalledTimes(2);

			await vi.advanceTimersByTimeAsync(10000);

			expect(mockOrder.getQuotes).toHaveBeenCalledTimes(3);

			vi.useRealTimers();
		});

		it('shows spinner during refresh without clearing existing quotes', async () => {
			let resolveSecondCall: (value: unknown) => void;
			const secondCallPromise = new Promise((resolve) => {
				resolveSecondCall = resolve;
			});

			vi.mocked(mockOrder.getQuotes)
				.mockResolvedValueOnce({
					value: mockQuotes as never,
					error: undefined
				})
				.mockImplementationOnce(() => secondCallPromise as never);

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('max-output')).toBeInTheDocument();
			});

			const refreshButton = screen.getByTestId('refresh-button');
			await fireEvent.click(refreshButton);

			expect(screen.getByTestId('max-output')).toBeInTheDocument();
			expect(screen.queryByText('Loading quotes...')).not.toBeInTheDocument();

			resolveSecondCall!({
				value: mockQuotes as never,
				error: undefined
			});
		});

		it('does not show error message when refresh fails', async () => {
			vi.mocked(mockOrder.getQuotes)
				.mockResolvedValueOnce({
					value: mockQuotes as never,
					error: undefined
				})
				.mockResolvedValueOnce({
					value: undefined,
					error: { msg: 'Refresh error', readableMsg: 'Failed to refresh quotes' }
				});

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('max-output')).toBeInTheDocument();
			});

			const refreshButton = screen.getByTestId('refresh-button');
			await fireEvent.click(refreshButton);

			await waitFor(() => {
				expect(mockOrder.getQuotes).toHaveBeenCalledTimes(2);
			});

			expect(screen.queryByText('Failed to refresh quotes')).not.toBeInTheDocument();
			expect(screen.getByTestId('max-output')).toBeInTheDocument();
		});

		it('stops auto-refresh interval when modal closes', async () => {
			vi.useFakeTimers();

			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: mockQuotes as never,
				error: undefined
			});

			const { component } = render(TakeOrderModal, defaultProps);

			await vi.advanceTimersByTimeAsync(0);

			expect(mockOrder.getQuotes).toHaveBeenCalledTimes(1);

			await component.$set({ open: false });
			await vi.advanceTimersByTimeAsync(0);

			const callCountAfterClose = vi.mocked(mockOrder.getQuotes).mock.calls.length;

			await vi.advanceTimersByTimeAsync(30000);

			expect(mockOrder.getQuotes).toHaveBeenCalledTimes(callCountAfterClose);

			vi.useRealTimers();
		});

		it('cleans up interval on component destroy', async () => {
			vi.useFakeTimers();

			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: mockQuotes as never,
				error: undefined
			});

			const { unmount } = render(TakeOrderModal, defaultProps);

			await vi.advanceTimersByTimeAsync(0);

			expect(mockOrder.getQuotes).toHaveBeenCalledTimes(1);

			unmount();

			await vi.advanceTimersByTimeAsync(30000);

			expect(mockOrder.getQuotes).toHaveBeenCalledTimes(1);

			vi.useRealTimers();
		});

		it('clears error message when auto-refresh succeeds after initial load error', async () => {
			vi.useFakeTimers();

			vi.mocked(mockOrder.getQuotes)
				.mockResolvedValueOnce({
					value: undefined,
					error: { msg: 'Initial error', readableMsg: 'Failed to fetch quotes' }
				})
				.mockResolvedValueOnce({
					value: mockQuotes as never,
					error: undefined
				});

			render(TakeOrderModal, defaultProps);

			await vi.advanceTimersByTimeAsync(0);

			await waitFor(() => {
				expect(screen.getByText('Failed to fetch quotes')).toBeInTheDocument();
			});

			await vi.advanceTimersByTimeAsync(10000);

			await waitFor(() => {
				expect(screen.queryByText('Failed to fetch quotes')).not.toBeInTheDocument();
				expect(screen.getByTestId('max-output')).toBeInTheDocument();
			});

			vi.useRealTimers();
		});

		it('clamps selectedPairIndex when quotes shrink during refresh', async () => {
			const threeQuotes = [
				mockQuotes[0],
				{
					pair: { pairName: 'DAI/ETH', inputIndex: 1, outputIndex: 0 },
					blockNumber: 12345678,
					success: true,
					data: mockQuotes[0].data
				},
				{
					pair: { pairName: 'WBTC/ETH', inputIndex: 2, outputIndex: 0 },
					blockNumber: 12345678,
					success: true,
					data: mockQuotes[0].data
				}
			];

			vi.mocked(mockOrder.getQuotes)
				.mockResolvedValueOnce({ value: threeQuotes as never, error: undefined })
				.mockResolvedValueOnce({ value: [mockQuotes[0]] as never, error: undefined });

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('pair-selector')).toBeInTheDocument();
			});

			const pairSelector = screen.getByTestId('pair-selector');
			await fireEvent.change(pairSelector, { target: { value: '2' } });

			const refreshButton = screen.getByTestId('refresh-button');
			await fireEvent.click(refreshButton);

			await waitFor(() => {
				expect(screen.queryByTestId('pair-selector')).not.toBeInTheDocument();
				expect(screen.getByTestId('max-output')).toBeInTheDocument();
				expect(screen.getByTestId('max-output')).toHaveTextContent('100');
			});
		});
	});

	describe('MAX amount', () => {
		const maxQuotes = [
			{
				pair: {
					pairName: 'USDC/ETH',
					inputIndex: 0,
					outputIndex: 0
				},
				blockNumber: 12345678,
				success: true,
				data: {
					maxOutput: floatToHex(Float.parse('100').value as Float),
					maxInput: floatToHex(Float.parse('50').value as Float),
					ratio: floatToHex(Float.parse('0.5').value as Float),
					inverseRatio: floatToHex(Float.parse('2').value as Float),
					formattedMaxOutput: '100',
					formattedMaxInput: '50',
					formattedRatio: '0.5',
					formattedInverseRatio: '2'
				}
			}
		];

		it('fills buy MAX with walletInput / ratio when that is below quote maxOutput', async () => {
			mockGetAccountBalance.mockResolvedValue({
				value: {
					balance: Float.parse('10').value as Float,
					formattedBalance: '10'
				},
				error: undefined
			});
			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: maxQuotes as never,
				error: undefined
			});

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByText('MAX')).toBeInTheDocument();
			});
			await fireEvent.click(screen.getByText('MAX'));

			const amountInput = screen.getAllByRole('textbox')[0] as HTMLInputElement;
			expect(amountInput.value).toBe('20');
			expect(mockGetAccountBalance).toHaveBeenCalledWith(MOCK_SIGNER_ADDRESS);
		});

		it('fills sell MAX with the wallet input-token balance when it is below quote maxInput', async () => {
			mockGetAccountBalance.mockResolvedValue({
				value: {
					balance: Float.parse('10').value as Float,
					formattedBalance: '10'
				},
				error: undefined
			});
			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: maxQuotes as never,
				error: undefined
			});

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('direction-sell')).toBeInTheDocument();
			});
			await fireEvent.click(screen.getByTestId('direction-sell'));

			await waitFor(() => {
				expect(screen.getByText('MAX')).toBeInTheDocument();
			});
			await fireEvent.click(screen.getByText('MAX'));

			const amountInput = screen.getAllByRole('textbox')[0] as HTMLInputElement;
			expect(amountInput.value).toBe('10');
		});
	});

	describe('max price current-quote shortcut', () => {
		const priceQuotes = [
			{
				pair: {
					pairName: 'USDC/ETH',
					inputIndex: 0,
					outputIndex: 0
				},
				blockNumber: 12345678,
				success: true,
				data: {
					maxOutput: floatToHex(Float.parse('100').value as Float),
					maxInput: floatToHex(Float.parse('50').value as Float),
					ratio: floatToHex(Float.parse('100').value as Float),
					inverseRatio: floatToHex(Float.parse('0.01').value as Float),
					formattedMaxOutput: '100',
					formattedMaxInput: '50',
					formattedRatio: '100',
					formattedInverseRatio: '0.01'
				}
			}
		];

		it('fills Max Price with the current quote plus a 0.5% buffer', async () => {
			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: priceQuotes as never,
				error: undefined
			});

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('price-cap-current-button')).toBeInTheDocument();
			});
			expect(screen.getByTestId('price-cap-current-button')).toHaveTextContent('CURRENT +0.5%');

			await fireEvent.click(screen.getByTestId('price-cap-current-button'));

			const priceCapInput = screen.getByTestId('price-cap-input') as HTMLInputElement;
			expect(priceCapInput.value).toBe('100.5');
			expect(screen.getByTestId('price-cap-buffer-hint')).toHaveTextContent(
				'Includes a 0.5% buffer above the current quote so the next block can still fill.'
			);
		});

		it('does not reject CURRENT +0.5% when the buffered quote has more than 67 decimals', async () => {
			const highPrecisionQuotes = [
				{
					...priceQuotes[0],
					data: {
						...priceQuotes[0].data,
						ratio: floatToHex(
							Float.parse('0.011068446678066134741211935').value as Float
						),
						formattedRatio: '0.011068446678066134741211935'
					}
				}
			];
			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: highPrecisionQuotes as never,
				error: undefined
			});

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('price-cap-current-button')).toBeInTheDocument();
			});
			await fireEvent.click(screen.getByTestId('price-cap-current-button'));

			const priceCapInput = screen.getByTestId('price-cap-input') as HTMLInputElement;
			expect(priceCapInput.value).not.toBe('');
			expect(screen.queryByTestId('price-cap-error')).not.toBeInTheDocument();
		});

		it('keeps the filled cap when the quote refreshes to a new price', async () => {
			const laterQuotes = [
				{
					...priceQuotes[0],
					data: {
						...priceQuotes[0].data,
						ratio: floatToHex(Float.parse('200').value as Float),
						formattedRatio: '200'
					}
				}
			];

			vi.mocked(mockOrder.getQuotes)
				.mockResolvedValueOnce({
					value: priceQuotes as never,
					error: undefined
				})
				.mockResolvedValueOnce({
					value: laterQuotes as never,
					error: undefined
				});

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('price-cap-current-button')).toBeInTheDocument();
			});
			await fireEvent.click(screen.getByTestId('price-cap-current-button'));

			const priceCapInput = screen.getByTestId('price-cap-input') as HTMLInputElement;
			expect(priceCapInput.value).toBe('100.5');

			await fireEvent.click(screen.getByTestId('refresh-button'));

			await waitFor(() => {
				expect(screen.getByTestId('current-price')).toHaveTextContent('200');
			});
			expect(priceCapInput.value).toBe('100.5');
		});

		it('hides the buffer hint when the user edits Max Price', async () => {
			vi.mocked(mockOrder.getQuotes).mockResolvedValue({
				value: priceQuotes as never,
				error: undefined
			});

			render(TakeOrderModal, defaultProps);

			await waitFor(() => {
				expect(screen.getByTestId('price-cap-current-button')).toBeInTheDocument();
			});
			await fireEvent.click(screen.getByTestId('price-cap-current-button'));
			expect(screen.getByTestId('price-cap-buffer-hint')).toBeInTheDocument();

			const priceCapInput = screen.getByTestId('price-cap-input');
			await fireEvent.input(priceCapInput, { target: { value: '101' } });

			expect(screen.queryByTestId('price-cap-buffer-hint')).not.toBeInTheDocument();
		});
	});
});
