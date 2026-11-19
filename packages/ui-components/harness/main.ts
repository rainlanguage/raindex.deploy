// Entry point for the component render harness.
//
// The harness mounts a single "scene" (a Svelte component that composes a real
// ui-components component with a fixture) into the page, selected by the
// `?scene=<name>` query parameter. The screenshot script drives a real headless
// Chromium against this page and captures a PNG, so UI claims can be verified
// against actual rendered pixels (jsdom component tests are text-only).
//
// See scenes/index.ts for the scene registry and harness/README.md for usage.

// Tailwind base/components/utilities, exactly as the app loads them.
import '../src/lib/app.css';
import { scenes } from './scenes';

const params = new URLSearchParams(location.search);
const requested = params.get('scene');
const name = requested ?? Object.keys(scenes)[0];
const Component = scenes[name];

const target = document.getElementById('app');
if (!target) {
	throw new Error('harness: #app mount point missing');
}

if (!Component) {
	// Build the diagnostic with DOM APIs and textContent so the query-derived
	// `name` is never interpolated into markup (no innerHTML sink).
	const error = document.createElement('pre');
	error.style.cssText = 'color:#b91c1c;padding:1rem;font:14px monospace';
	error.textContent =
		`Unknown scene: ${name}\n\nAvailable scenes:\n  ` +
		Object.keys(scenes).join('\n  ');
	target.replaceChildren(error);
	// Mark ready so the screenshot script fails fast with a visible error frame
	// instead of hanging on a wait selector that will never appear.
	document.body.setAttribute('data-harness-error', name ?? '');
} else {
	new Component({ target });
	// The screenshot script waits on a scene-specific selector, not this flag,
	// but it is handy when debugging in a normal browser.
	document.body.setAttribute('data-scene', name);
}
