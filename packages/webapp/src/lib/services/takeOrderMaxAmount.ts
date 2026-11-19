import { Float } from '@rainlanguage/raindex';

export type TakeOrderDirection = 'buy' | 'sell';

export function minFloat(a: Float, b: Float): Float {
	const cmp = a.lte(b);
	if (cmp.error) return a;
	return cmp.value ? a : b;
}

/**
 * MAX amount for the take-order field, in the field's units.
 *
 * Buy amounts are output tokens, so the wallet cap is `walletInput / ratio`.
 * Sell amounts are input tokens, so the wallet cap is the input-token balance.
 */
export function takeOrderMaxAmount(args: {
	direction: TakeOrderDirection;
	maxOutput: Float;
	maxInput: Float;
	ratio: Float;
	walletInputBalance: Float;
}): Float {
	if (args.direction === 'sell') {
		return minFloat(args.maxInput, args.walletInputBalance);
	}

	const affordableOutput = args.walletInputBalance.div(args.ratio);
	if (affordableOutput.error) return args.maxOutput;
	return minFloat(args.maxOutput, affordableOutput.value as Float);
}
