import type { ComponentType } from 'svelte';
import ToastErrorDecoded from './ToastErrorDecoded.svelte';

// The scene registry: a scene name -> a Svelte component that composes a real
// ui-components component with a fixture (props / store state / context).
//
// To add a new scene (e.g. the deposit modal for #550, the order-page IO ratio
// for #588, withdraw for #570, the sidebar):
//   1. Create `scenes/MyScene.svelte` that imports the real component from
//      `$lib/...` and feeds it fixture data. Wrap it in whatever providers it
//      needs (ToastProvider, QueryClientProvider, a wallet-store stub, ...).
//   2. Register it below under a short, url-safe key.
//   3. Render it:  harness/screenshot.sh my-scene /abs/path/out.png
//
// Heavy runtime deps (the `@rainlanguage/raindex` wasm, `$app/*`) are
// alias-stubbed in vite.config.ts so components resolve without wasm/SvelteKit.
export const scenes: Record<string, ComponentType> = {
	// raindex#544 — decoded error message in an error toast.
	'toast-error-decoded': ToastErrorDecoded
};
