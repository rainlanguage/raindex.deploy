import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { RaindexMarketSnapshot } from '@rainlanguage/raindex';
import MarketDetailPanel from './MarketDetailPanel.svelte';

const queryState = vi.hoisted(() => ({ value: {} as Record<string, unknown> }));

vi.mock('$env/dynamic/public', () => ({ env: {} }));

vi.mock('@tanstack/svelte-query', () => ({
	createQuery: () => ({
		subscribe(run: (value: Record<string, unknown>) => void) {
			run(queryState.value);
			return () => undefined;
		}
	})
}));

const snapshot: RaindexMarketSnapshot = {
	market: {
		id: '8453:base:quote',
		tickerId: '0xbase_0xquote',
		chainId: 8453,
		base: {
			chainId: 8453,
			address: '0xbase',
			name: 'Base',
			symbol: 'BASE',
			decimals: 18,
			logoUri: 'https://assets.example/base.png',
			variants: []
		},
		quote: {
			chainId: 8453,
			address: '0xquote',
			name: 'Quote',
			symbol: 'QUOTE',
			decimals: 6,
			logoUri: 'https://assets.example/quote.png',
			variants: []
		},
		raindexAddresses: ['0xraindex']
	},
	orderbook: {
		bestBid: '99',
		bestAsk: '101',
		bids: [
			{
				price: '99',
				baseQuantity: '1',
				targetQuantity: '99',
				chainId: 8453,
				raindex: '0xraindex',
				orderHash: '0xshared',
				sourceToken: '0xinput-a',
				blockNumber: 1
			},
			{
				price: '98',
				baseQuantity: '2',
				targetQuantity: '196',
				chainId: 8453,
				raindex: '0xraindex',
				orderHash: '0xshared',
				sourceToken: '0xinput-b',
				blockNumber: 1
			}
		],
		asks: []
	},
	stats: {
		lastPrice: '100.123456789',
		high24h: '102',
		low24h: '97',
		baseVolume24h: '12.5',
		targetVolume24h: '1250',
		tradeCount24h: 3
	},
	recentTrades: [],
	observedAt: 100,
	errors: []
};

describe('MarketDetailPanel', () => {
	it('keeps cached detail visible after refresh failure and renders repeated order hashes', () => {
		queryState.value = {
			data: snapshot,
			isPending: false,
			isError: true,
			isRefetchError: true,
			refetch: vi.fn()
		};

		render(MarketDetailPanel, {
			props: { tickerId: snapshot.market.tickerId }
		});

		expect(screen.getByText('Cached quote · refresh failed')).toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'Orderbook' })).toBeInTheDocument();
		expect(
			document.querySelector('img[src="https://assets.example/base.png"]')
		).toBeInTheDocument();
		expect(
			document.querySelector('img[src="https://assets.example/quote.png"]')
		).toBeInTheDocument();
		expect(screen.getByText('100.12346')).toBeInTheDocument();
		expect(screen.getByText('Exact API values')).toBeInTheDocument();
		expect(screen.getByText('100.123456789')).toBeInTheDocument();
		expect(screen.getByTitle('Price 99, size 1')).toBeInTheDocument();
		expect(screen.getByTitle('Price 98, size 2')).toBeInTheDocument();
	});
});
