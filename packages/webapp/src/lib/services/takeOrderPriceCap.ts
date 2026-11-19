import { Float } from '@rainlanguage/raindex';

/** Multiplier for a 0.5% ceiling above the current quote ratio. */
const TAKE_ORDER_PRICE_CAP_BUFFER_MULTIPLIER = '1.005';

/**
 * Must match `@rainlanguage/ui-components` `MAX_DECIMALS`. The Max Price
 * field rejects more fractional digits than this.
 */
export const MAX_PRICE_CAP_DECIMALS = 67;

export const TAKE_ORDER_PRICE_CAP_BUFFER_LABEL = '0.5%';

/**
 * Snapshot cap for Max Price: current quote ratio plus a 0.5% buffer so a
 * slightly worse next-block print can still land.
 */
export function bufferedPriceCap(currentRatio: Float): Float | undefined {
	const multiplier = Float.parse(TAKE_ORDER_PRICE_CAP_BUFFER_MULTIPLIER);
	if (multiplier.error) return undefined;
	const result = currentRatio.mul(multiplier.value as Float);
	if (result.error) return undefined;
	return result.value as Float;
}

/**
 * Formats {@link bufferedPriceCap} for the Max Price text field.
 *
 * `Float.format()` can emit more fractional digits than the client-side
 * parser accepts (67). Truncate so CURRENT +0.5% never fails decimal
 * validation. The dropped digits are far smaller than the 0.5% buffer.
 */
export function formatBufferedPriceCap(currentRatio: Float): string | undefined {
	const buffered = bufferedPriceCap(currentRatio);
	if (!buffered) return undefined;
	const formatted = buffered.format();
	if (formatted.error || typeof formatted.value !== 'string') return undefined;
	return limitDecimalPlaces(formatted.value, MAX_PRICE_CAP_DECIMALS);
}

export function limitDecimalPlaces(value: string, maxDecimals: number): string {
	const sign = value.startsWith('-') ? '-' : '';
	const unsigned = sign ? value.slice(1) : value;
	const dot = unsigned.indexOf('.');
	if (dot === -1) return value;

	const fractionalPart = unsigned.slice(dot + 1);
	if (fractionalPart.length <= maxDecimals) return value;
	return `${sign}${unsigned.slice(0, dot + 1 + maxDecimals)}`;
}
