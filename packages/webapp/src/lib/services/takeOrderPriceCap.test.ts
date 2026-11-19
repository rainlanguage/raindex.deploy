import { describe, it, expect } from 'vitest';
import { Float } from '@rainlanguage/raindex';
import {
	TAKE_ORDER_PRICE_CAP_BUFFER_LABEL,
	MAX_PRICE_CAP_DECIMALS,
	bufferedPriceCap,
	formatBufferedPriceCap,
	limitDecimalPlaces
} from './takeOrderPriceCap';

function f(value: string): Float {
	return Float.parse(value).value as Float;
}

function decimalCount(value: string): number {
	const dot = value.indexOf('.');
	return dot === -1 ? 0 : value.length - dot - 1;
}

describe('bufferedPriceCap', () => {
	it('exposes a 0.5% buffer label', () => {
		expect(TAKE_ORDER_PRICE_CAP_BUFFER_LABEL).toBe('0.5%');
	});

	it('raises the current quote by 0.5%', () => {
		const result = bufferedPriceCap(f('100'));
		expect(result?.format().value).toBe('100.5');
	});

	it('raises a non-round quote by 0.5%', () => {
		const result = bufferedPriceCap(f('200'));
		expect(result?.format().value).toBe('201');
	});
});

describe('limitDecimalPlaces', () => {
	it('leaves values within the parser limit unchanged', () => {
		expect(limitDecimalPlaces('100.5', MAX_PRICE_CAP_DECIMALS)).toBe('100.5');
	});

	it('truncates a CURRENT +0.5% snapshot that exceeds 67 decimals', () => {
		const tooLong =
			'0.011121324352043270230379406670855011893399508723917687482222187121207';
		expect(decimalCount(tooLong)).toBeGreaterThan(MAX_PRICE_CAP_DECIMALS);

		const limited = limitDecimalPlaces(tooLong, MAX_PRICE_CAP_DECIMALS);
		expect(decimalCount(limited)).toBe(MAX_PRICE_CAP_DECIMALS);
		expect(limited).toBe(
			'0.0111213243520432702303794066708550118933995087239176874822221871212'
		);
	});
});

describe('formatBufferedPriceCap', () => {
	it('formats a round quote plus 0.5% without extra decimals', () => {
		expect(formatBufferedPriceCap(f('100'))).toBe('100.5');
	});

	it('always returns a string with at most 67 decimal places for any finite quote', () => {
		const quote = f('0.011068446678066134741211935');
		const formatted = formatBufferedPriceCap(quote);
		expect(formatted).toBeDefined();
		expect(decimalCount(formatted as string)).toBeLessThanOrEqual(MAX_PRICE_CAP_DECIMALS);
	});
});
