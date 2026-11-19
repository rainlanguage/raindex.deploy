<script lang="ts">
	import { onMount } from 'svelte';
	import { useToasts } from '$lib/providers/toasts/useToasts';

	// A `WasmEncodedError` as produced by the `@rainlanguage/raindex` wasm
	// bindings: `{ msg, readableMsg }` (see crates/js_api/ARCHITECTURE.md and
	// packages/raindex/scripts/buildPackage.js). `msg` is the raw contract-level
	// failure; `readableMsg` is the DECODED, human-readable explanation.
	//
	// Issue #544 ("error decoding for toasts") is about surfacing `readableMsg`
	// in the toast rather than a raw selector/hex. This scene proves the decoded
	// text is what actually renders.
	interface WasmEncodedError {
		msg: string;
		readableMsg: string;
	}

	const withdrawError: WasmEncodedError = {
		// What an UN-decoded toast would show: a raw revert selector.
		msg: 'execution reverted, data: 0x963b34a5',
		// The decoded, human-readable reason the toast SHOULD show.
		readableMsg:
			'Withdrawal failed: the requested amount of 50.0 WFLR exceeds the vault balance of 12.5 WFLR available to withdraw.'
	};

	const { errToast } = useToasts();

	onMount(() => {
		// Mirrors the real error path: components throw
		// `new Error(result.error.readableMsg)` and the catch surfaces the decoded
		// message via errToast (see OrderDetail.svelte / VaultDetail.svelte).
		errToast(withdrawError.readableMsg);
	});
</script>
