import { defineChain } from 'viem';
import { mainnet, polygon, arbitrum, base, flare, linea, bsc } from 'wagmi/chains';

// Not in viem 2.24.3; required so wagmi switchChain(4663) succeeds.
// https://docs.robinhood.com/chain/connecting/
export const robinhood = defineChain({
	id: 4663,
	name: 'Robinhood Chain',
	nativeCurrency: { name: 'Ether', symbol: 'ETH', decimals: 18 },
	rpcUrls: {
		default: {
			http: [
				'https://rpc.mainnet.chain.robinhood.com',
				'https://robinhood-rpc.publicnode.com',
				'https://robinhood.drpc.org'
			]
		}
	},
	blockExplorers: {
		default: {
			name: 'Blockscout',
			url: 'https://robinhoodchain.blockscout.com'
		}
	}
});

export const SupportedChains = {
	mainnet,
	polygon,
	arbitrum,
	base,
	flare,
	linea,
	bsc,
	robinhood
} as const;
export const supportedChainsList = [
	mainnet,
	polygon,
	arbitrum,
	base,
	flare,
	linea,
	bsc,
	robinhood
] as const;
