// Stub for SvelteKit's `$app/stores`. The harness runs the plain Svelte plugin
// (not sveltekit()), so the `$app/*` virtual modules are alias-stubbed here.
// Mirrors the mock in ../../test-setup.ts.
import { readable, writable } from 'svelte/store';

export const page = readable({
	url: new URL('http://localhost/'),
	params: {},
	route: { id: null },
	status: 200,
	error: null,
	data: {},
	form: undefined,
	searchParams: new URLSearchParams()
});

export const navigating = readable(null);
export const updated = { ...readable(false), check: async () => false };
export const session = writable(null);

export function getStores() {
	return { page, navigating, updated, session };
}
