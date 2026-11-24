import { describe, it, expect } from 'vitest';
import { supportedChainsList, SupportedChains } from './chains';

const ROBINHOOD_CHAIN_ID = 4663;
const ZEROG_CHAIN_ID = 16661;

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

	it('includes 0G Mainnet so wagmi can switch to chain 16661', () => {
		const zerog = supportedChainsList.find((chain) => chain.id === ZEROG_CHAIN_ID);

		expect(zerog).toBeDefined();
		expect(zerog?.name).toBe('0G Mainnet');
		expect(zerog?.nativeCurrency.symbol).toBe('0G');
		expect(zerog?.rpcUrls.default.http).toContain('https://evmrpc.0g.ai');
		expect(zerog?.blockExplorers?.default.url).toBe('https://chainscan.0g.ai');
	});

	it('exposes 0G on SupportedChains', () => {
		expect(SupportedChains.zerog.id).toBe(ZEROG_CHAIN_ID);
	});
});
