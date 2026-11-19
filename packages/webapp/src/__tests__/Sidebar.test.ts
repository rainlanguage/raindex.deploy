import { render, cleanup, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { vi, describe, it, expect, afterEach } from 'vitest';
import { writable } from 'svelte/store';
import Sidebar from '../lib/components/Sidebar.svelte';
import { networkStatuses } from '../lib/stores/localDbStatus';

vi.mock('@rainlanguage/ui-components', async (importOriginal) => {
	const actual = await importOriginal<typeof import('@rainlanguage/ui-components')>();
	const MockComponent = (await import('../lib/__mocks__/MockComponent.svelte')).default;
	return {
		...actual,
		ButtonDarkMode: MockComponent,
		logoLight: 'mock-logo-light.svg',
		logoDark: 'mock-logo-dark.svg',
		IconTelegram: MockComponent,
		IconExternalLink: MockComponent,
		LocalDbStatusCard: (await import('../lib/__mocks__/LocalDbStatusCardMock.svelte')).default,
		WalletConnect: MockComponent,
		TransactionList: MockComponent
	};
});

vi.mock('svelte/store', async (importOriginal) => {
	return {
		...((await importOriginal()) as object),
		writable: (value: unknown) => {
			let current = value;
			const subscribers = new Set<(val: unknown) => void>();
			return {
				subscribe: (run: (val: unknown) => void) => {
					subscribers.add(run);
					run(current);
					return () => {
						subscribers.delete(run);
					};
				},
				set: vi.fn((next: unknown) => {
					current = next;
					subscribers.forEach((run) => run(current));
				}),
				update: vi.fn((updater: (cur: unknown) => unknown) => {
					current = updater(current);
					subscribers.forEach((run) => run(current));
				})
			};
		}
	};
});

const mockWindowSize = (width: number) => {
	Object.defineProperty(window, 'innerWidth', {
		writable: true,
		configurable: true,
		value: width
	});
	window.dispatchEvent(new Event('resize'));
};

describe('Sidebar', () => {
	afterEach(() => {
		cleanup();
		networkStatuses.set(new Map());
	});

	it('renders correctly with colorTheme store', async () => {
		const mockColorTheme = writable('light');
		const mockPage = {
			url: { pathname: '/' }
		};
		const { container } = render(Sidebar, {
			props: {
				colorTheme: mockColorTheme,
				page: mockPage
			}
		});

		expect(container).toBeTruthy();
	});
	it('shows Markets immediately after Vaults', () => {
		mockWindowSize(1025);
		const mockColorTheme = writable('light');
		const mockPage = {
			url: { pathname: '/markets' }
		};
		render(Sidebar, { colorTheme: mockColorTheme, page: mockPage });

		const vaults = screen.getByTestId('sidebar-vaults').closest('a');
		const markets = screen.getByTestId('sidebar-markets').closest('a');
		expect(vaults).not.toBeNull();
		expect(markets).not.toBeNull();
		expect(markets).toHaveAttribute('href', '/markets');
		expect(vaults!.compareDocumentPosition(markets!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		expect(vaults!.parentElement?.nextElementSibling).toContainElement(markets);
	});
	it('renders menu bars button when screen width is small', () => {
		// Mock small screen width
		mockWindowSize(500);
		const mockColorTheme = writable('light');
		const mockPage = {
			url: { pathname: '/' }
		};
		render(Sidebar, { colorTheme: mockColorTheme, page: mockPage });

		const barsButton = screen.getByTestId('sidebar-bars');
		expect(barsButton).toBeInTheDocument();
	});
	it('shows hover title and aria-label on the open and close menu buttons', async () => {
		mockWindowSize(500);
		const mockColorTheme = writable('light');
		const mockPage = {
			url: { pathname: '/' }
		};
		render(Sidebar, { colorTheme: mockColorTheme, page: mockPage });

		const barsButton = screen.getByTestId('sidebar-bars');
		expect(barsButton).toHaveAttribute('title', 'Open menu');
		expect(barsButton).toHaveAttribute('aria-label', 'Open menu');

		await fireEvent.click(barsButton);

		const closeButton = await screen.findByTestId('close-button');
		expect(closeButton).toHaveAttribute('title', 'Close menu');
		expect(closeButton).toHaveAttribute('aria-label', 'Close menu');
	});
	it('shows sidebar on wide screen', () => {
		mockWindowSize(1025);
		const mockColorTheme = writable('light');
		const mockPage = {
			url: { pathname: '/' }
		};
		render(Sidebar, { colorTheme: mockColorTheme, page: mockPage });

		expect(screen.getByTestId('sidebar')).toBeInTheDocument();
	});
	it('renders sidebar when bars button is clicked', async () => {
		// Mock small screen width
		mockWindowSize(500);
		const mockColorTheme = writable('light');
		const mockPage = {
			url: { pathname: '/' }
		};
		render(Sidebar, { colorTheme: mockColorTheme, page: mockPage });

		const barsButton = screen.getByTestId('sidebar-bars');
		fireEvent.click(barsButton);

		await waitFor(() => {
			const sidebar = screen.getByTestId('sidebar');
			expect(sidebar.hidden).toBe(false);
		});
	});
	it('renders the scrub toggle next to the dark mode button', () => {
		mockWindowSize(1025);
		const mockColorTheme = writable('light');
		const mockPage = {
			url: { pathname: '/' }
		};
		render(Sidebar, { colorTheme: mockColorTheme, page: mockPage });

		expect(screen.getByTestId('scrub-toggle')).toBeInTheDocument();
	});
	it('hides sidebar when close button is clicked', async () => {
		mockWindowSize(500);
		const mockColorTheme = writable('light');
		const mockPage = {
			url: { pathname: '/' }
		};
		render(Sidebar, { colorTheme: mockColorTheme, page: mockPage });

		const barsButton = screen.getByTestId('sidebar-bars');
		await fireEvent.click(barsButton);
		await waitFor(() => {
			expect(screen.getByTestId('sidebar')).toBeInTheDocument();
		});
		const closeButton = screen.getByTestId('close-button');
		await fireEvent.click(closeButton);
		await waitFor(() => {
			const sidebar = screen.getByTestId('sidebar');
			expect(sidebar.hidden).toBe(true);
		});
	});
});
