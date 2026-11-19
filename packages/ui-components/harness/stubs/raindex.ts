/* eslint-disable @typescript-eslint/no-explicit-any */
// Stub for the `@rainlanguage/raindex` wasm package.
//
// The harness renders components in a real browser WITHOUT initialising wasm.
// Every runtime export is replaced with an inert, infinitely-chainable no-op
// (callable, constructible, and swallows any static/instance member access) so
// that components resolve and mount. Scenes that need a specific return value
// from one of these should provide it via the scene's own fixture rather than
// relying on this stub.
//
// Type-only imports (`import type { ... } from '@rainlanguage/raindex'`) are
// erased at compile time and never reach this module.

function makeStub(name: string): any {
	const fn: any = function () {};
	return new Proxy(fn, {
		get(target, prop) {
			if (prop === Symbol.toPrimitive || prop === 'toString' || prop === 'valueOf') {
				return () => `[raindex-stub ${name}]`;
			}
			// `then`/`Symbol.iterator` return undefined so a stub is never mistaken
			// for a thenable (would hang `await`/`Promise.resolve`) or an iterable.
			if (prop === Symbol.iterator || prop === 'then') return undefined;
			if (prop in target) return (target as any)[prop];
			return makeStub(`${name}.${String(prop)}`);
		},
		apply() {
			return makeStub(`${name}()`);
		},
		construct() {
			return makeStub(`new ${name}`);
		}
	});
}

// Runtime classes / values imported by ui-components. Add to this list when a
// new scene imports another runtime export from `@rainlanguage/raindex`.
export const RaindexClient = makeStub('RaindexClient');
export const RaindexOrder = makeStub('RaindexOrder');
export const RaindexOrderBuilder = makeStub('RaindexOrderBuilder');
export const RaindexOrderQuote = makeStub('RaindexOrderQuote');
export const RaindexTrade = makeStub('RaindexTrade');
export const RaindexTransaction = makeStub('RaindexTransaction');
export const RaindexVault = makeStub('RaindexVault');
export const RaindexVaultBalanceChange = makeStub('RaindexVaultBalanceChange');
export const RaindexVaultToken = makeStub('RaindexVaultToken');
export const RaindexVaultVolume = makeStub('RaindexVaultVolume');
export const RaindexSyncStatus = makeStub('RaindexSyncStatus');
export const DotrainRegistry = makeStub('DotrainRegistry');
export const OrderPerformance = makeStub('OrderPerformance');
export const Float = makeStub('Float');
export const AccountBalance = makeStub('AccountBalance');
export const NetworkSyncStatus = makeStub('NetworkSyncStatus');
export const LocalDbStatus = makeStub('LocalDbStatus');

export default makeStub('@rainlanguage/raindex');
