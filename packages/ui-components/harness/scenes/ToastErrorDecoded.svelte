<script lang="ts">
	// Scene for raindex#544 "error decoding for toasts".
	//
	// Renders the REAL toast stack (ToastProvider -> ToastDetail -> flowbite
	// Toast) and pushes a decoded WasmEncodedError through the real errToast /
	// toast-store path. The screenshot must visibly show the decoded,
	// human-readable message (not a raw hex/selector).
	import ToastProvider from '$lib/providers/toasts/ToastProvider.svelte';
	import EmitDecodedError from './EmitDecodedError.svelte';
</script>

<!-- A little breathing room so the fixed top-right toast is not flush to the
     viewport edge in the captured PNG. -->
<div class="min-h-[220px] w-full p-4">
	<ToastProvider>
		<EmitDecodedError />
	</ToastProvider>
</div>
