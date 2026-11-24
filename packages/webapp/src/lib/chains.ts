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

// Not in viem 2.24.3; required so wagmi switchChain(16661) succeeds.
export const zerog = defineChain({
	id: 16661,
	name: '0G Mainnet',
	nativeCurrency: { name: '0G', symbol: '0G', decimals: 18 },
	rpcUrls: {
		default: {
			http: ['https://evmrpc.0g.ai', 'https://0g.drpc.org', 'https://0g-rpc.publicnode.com']
		}
	},
	blockExplorers: {
		default: {
			name: '0G Chainscan',
			url: 'https://chainscan.0g.ai'
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
	robinhood,
	zerog
} as const;
export const supportedChainsList = [
	mainnet,
	polygon,
	arbitrum,
	base,
	flare,
	linea,
	bsc,
	robinhood,
	zerog
] as const;
