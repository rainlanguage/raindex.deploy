import { describe, it, expect } from 'vitest';
import { supportedChainsList, SupportedChains } from './chains';

const ROBINHOOD_CHAIN_ID = 4663;

describe('supportedChainsList', () => {
	it('includes Robinhood Chain so wagmi can switch to chain 4663', () => {
		const robinhood = supportedChainsList.find((chain) => chain.id === ROBINHOOD_CHAIN_ID);

		expect(robinhood).toBeDefined();
		expect(robinhood?.name).toBe('Robinhood Chain');
		expect(robinhood?.nativeCurrency.symbol).toBe('ETH');
		expect(robinhood?.rpcUrls.default.http).toContain('https://rpc.mainnet.chain.robinhood.com');
		expect(robinhood?.blockExplorers?.default.url).toBe('https://robinhoodchain.blockscout.com');
	});

	it('exposes Robinhood on SupportedChains', () => {
		expect(SupportedChains.robinhood.id).toBe(ROBINHOOD_CHAIN_ID);
	});
});
