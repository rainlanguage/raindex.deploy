import { defineConfig } from 'vite';
import { svelte, vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';

const here = fileURLToPath(new URL('.', import.meta.url));
const resolve = (p: string) => fileURLToPath(new URL(p, import.meta.url));

// Standalone vite config for the component render harness.
//
// It runs the plain Svelte plugin (NOT sveltekit()) with `harness/` as the
// root, and alias-stubs the SvelteKit `$app/*` virtual modules plus the heavy
// `@rainlanguage/raindex` wasm package so ui-components components resolve and
// mount in a real browser without wasm/router. `$lib` maps to the real
// component source in ../src/lib.
//
// Order matters: the more specific stub aliases must come BEFORE the `$lib`
// prefix alias.
export default defineConfig({
	root: here,
	plugins: [svelte({ preprocess: vitePreprocess() })],
	resolve: {
		alias: [
			{ find: '$app/stores', replacement: resolve('./stubs/app-stores.ts') },
			{ find: '$app/navigation', replacement: resolve('./stubs/app-navigation.ts') },
			{ find: '$app/environment', replacement: resolve('./stubs/app-environment.ts') },
			{ find: /^@rainlanguage\/raindex$/, replacement: resolve('./stubs/raindex.ts') },
			{ find: '$lib', replacement: resolve('../src/lib') }
		]
	},
	define: {
		'process.env': {}
	},
	server: {
		fs: {
			// Allow serving the ui-components source and the repo-root node_modules.
			allow: [resolve('..'), resolve('../../..')]
		}
	}
});
