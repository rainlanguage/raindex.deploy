import { describe, it, expect } from 'vitest';
import { Float } from '@rainlanguage/raindex';
import { takeOrderMaxAmount } from './takeOrderMaxAmount';

function f(value: string): Float {
	return Float.parse(value).value as Float;
}

describe('takeOrderMaxAmount', () => {
	it('caps sell MAX at the wallet input-token balance when it is below quote maxInput', () => {
		const result = takeOrderMaxAmount({
			direction: 'sell',
			maxOutput: f('100'),
			maxInput: f('50'),
			ratio: f('0.5'),
			walletInputBalance: f('10')
		});
		expect(result.format().value).toBe('10');
	});

	it('caps sell MAX at quote maxInput when the wallet can spend more than the order', () => {
		const result = takeOrderMaxAmount({
			direction: 'sell',
			maxOutput: f('100'),
			maxInput: f('50'),
			ratio: f('0.5'),
			walletInputBalance: f('80')
		});
		expect(result.format().value).toBe('50');
	});

	it('caps buy MAX at walletInput / ratio when that is below quote maxOutput', () => {
		const result = takeOrderMaxAmount({
			direction: 'buy',
			maxOutput: f('100'),
			maxInput: f('50'),
			ratio: f('0.5'),
			walletInputBalance: f('10')
		});
		expect(result.format().value).toBe('20');
	});

	it('caps buy MAX at quote maxOutput when the wallet can afford the full order', () => {
		const result = takeOrderMaxAmount({
			direction: 'buy',
			maxOutput: f('100'),
			maxInput: f('50'),
			ratio: f('0.5'),
			walletInputBalance: f('80')
		});
		expect(result.format().value).toBe('100');
	});
});
