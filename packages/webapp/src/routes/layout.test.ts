import { render, waitFor, screen } from '@testing-library/svelte';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import Layout from './+layout.svelte';

const { mockPageStore, initialPageState, mockSignerAddressStore } = await vi.hoisted(
	() => import('$lib/__mocks__/stores')
);

const mockEnv = vi.hoisted(() => ({ browser: true }));
const mockInitWallet = vi.hoisted(() => vi.fn());

vi.mock('$lib/services/handleWalletInitialization', () => ({
	initWallet: mockInitWallet
}));

vi.mock('$app/stores', async (importOriginal) => {
	return {
		...((await importOriginal()) as object),
		page: mockPageStore
	};
});

vi.mock('$app/environment', () => mockEnv);

vi.mock('$lib/components/TransactionProviderWrapper.svelte', async () => {
	const MockComponent = (await import('$lib/__mocks__/MockComponent.svelte')).default;
	return {
		default: MockComponent
	};
});

vi.mock('$lib/components/Sidebar.svelte', async () => {
	const MockComponent = (await import('$lib/__mocks__/MockComponent.svelte')).default;
	return { default: MockComponent };
});

vi.mock('@rainlanguage/ui-components', async (importOriginal) => {
	const MockComponent = (await import('$lib/__mocks__/MockComponent.svelte')).default;
	return {
		...(await importOriginal()),
		cachedWritableStore: vi.fn(),
		WalletProvider: MockComponent,
		ToastProvider: MockComponent,
		FixedBottomTransaction: MockComponent
	};
});

vi.mock('$lib/stores/wagmi', () => ({
	signerAddress: mockSignerAddressStore
}));

vi.mock('$env/static/public', () => ({
	PUBLIC_WALLETCONNECT_PROJECT_ID: 'test-project-id'
}));

vi.mock('@wagmi/connectors', async (importOriginal) => {
	return {
		...(await importOriginal()),
		injected: vi.fn().mockReturnValue('injected-connector'),
		walletConnect: vi.fn().mockReturnValue('wallet-connect-connector')
	};
});

vi.mock('@tanstack/svelte-query', async (importOriginal) => {
	const MockComponent = (await import('$lib/__mocks__/MockComponent.svelte')).default;
	return {
		...(await importOriginal()),
		QueryClientProvider: MockComponent,
		QueryClient: vi.fn().mockImplementation(() => ({
			name: 'test'
		}))
	};
});

describe('Layout component', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.resetAllMocks();
		mockEnv.browser = true;
		mockInitWallet.mockResolvedValue(null);
	});

	it('displays an error message if wallet initialization fails', async () => {
		mockInitWallet.mockResolvedValue(
			'Failed to initialize wallet connection: Test error. Please try again or check console.'
		);
		mockPageStore.mockSetSubscribeValue(initialPageState);

		render(Layout);

		const errorMessage = await screen.findByText(
			'Failed to initialize wallet connection: Test error. Please try again or check console.'
		);
		expect(errorMessage).toBeInTheDocument();
	});

	it('renders Homepage when on root path', async () => {
		mockPageStore.mockSetSubscribeValue({
			...initialPageState,
			url: new URL('http://localhost/')
		});

		const { container } = render(Layout);

		await waitFor(() => {
			expect(container.querySelector('main')).not.toBeInTheDocument();
			expect(screen.getByTestId('homepage')).toBeInTheDocument();
		});
	});

	it('renders main content when not on root path', async () => {
		mockPageStore.mockSetSubscribeValue({
			...initialPageState,
			url: new URL('http://localhost/some-page')
		});

		render(Layout);

		await waitFor(() => {
			expect(screen.getByTestId('layout-container')).toBeInTheDocument();
		});
	});

	it('does not initialize wallet when not in browser environment', async () => {
		const originalNavigator = global.navigator;
		mockEnv.browser = false;
		render(Layout);
		expect(mockInitWallet).not.toHaveBeenCalled();
		Object.defineProperty(global, 'navigator', {
			value: originalNavigator,
			writable: true
		});
	});

	it('displays an error page if page.error is set', async () => {
		mockPageStore.mockSetSubscribeValue({
			...initialPageState,
			data: {
				...initialPageState.data,
				errorMessage: 'Test error'
			}
		});
		render(Layout);

		await waitFor(() => {
			expect(screen.getByText('Test error')).toBeInTheDocument();
			expect(screen.getByTestId('error-page')).toBeInTheDocument();
		});
	});

	it('shows a non-fatal registry warning banner and still renders the app', async () => {
		mockPageStore.mockSetSubscribeValue({
			...initialPageState,
			url: new URL('http://localhost/some-page'),
			data: {
				...initialPageState.data,
				registryWarning: 'The custom registry failed to load and has been reset to the default registry.'
			}
		});

		render(Layout);

		await waitFor(() => {
			expect(screen.getByTestId('registry-warning')).toBeInTheDocument();
			expect(
				screen.getByText(
					'The custom registry failed to load and has been reset to the default registry.'
				)
			).toBeInTheDocument();
			// The warning is non-fatal: the app still mounts and the error page is absent.
			expect(screen.getByTestId('layout-container')).toBeInTheDocument();
			expect(screen.queryByTestId('error-page')).not.toBeInTheDocument();
		});
	});
});
