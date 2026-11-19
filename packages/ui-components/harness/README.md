# Component render + screenshot harness

Mounts a single `ui-components` Svelte component in a **real headless Chromium**
and captures a **real-pixel PNG**. The component unit tests run in jsdom and are
text-only; this harness lets you verify what a component actually _looks like_ —
useful for confirming UI bug fixes / close-candidate claims against pixels.

It is modelled on the "vite alias-stub" render harness used in the
`cyclofinance/cyclo.site` repo: a standalone vite config rooted in `harness/`
that runs the plain Svelte plugin (not `sveltekit()`) and alias-stubs the heavy
runtime dependencies so a component resolves and mounts without wasm, a wallet,
or a router.

## Quick start

From the repo root:

```sh
packages/ui-components/harness/screenshot.sh toast-error-decoded /tmp/toast.png
```

That resolves Chromium + fonts + Node via nix, starts a vite dev server, renders
the scene, and writes the PNG. Optional width/height:

```sh
packages/ui-components/harness/screenshot.sh toast-error-decoded /tmp/toast.png 760 240
```

Prerequisite: dependencies installed for `ui-components`
(`cd packages/ui-components && npm i`, exactly what the CI `ui-components` task
does). `node_modules` is hoisted to the repo root.

`screenshot.sh` prefers a chromium already on `$PATH` or one named in
`$CHROMIUM`, and only falls back to `nix build nixpkgs#chromium` (a large
first-run download) when neither is present. Point it at an existing browser to
skip that:

```sh
CHROMIUM=/path/to/chromium \
packages/ui-components/harness/screenshot.sh toast-error-decoded /tmp/toast.png
```

## Rendering an arbitrary component (add a scene)

A **scene** is a small Svelte component that composes a real component from
`$lib/...` with a fixture (props / store state / context providers). Scenes live
in `scenes/` and are registered by a short, url-safe key in `scenes/index.ts`.

1. Create `scenes/MyScene.svelte`:

   ```svelte
   <script lang="ts">
     import SomeComponent from '$lib/components/.../SomeComponent.svelte';
     // build fixture props here
   </script>

   <SomeComponent foo={...} bar={...} />
   ```

   Wrap it in whatever providers the component needs (e.g. `ToastProvider`, a
   `@tanstack/svelte-query` `QueryClientProvider`, a wallet-store stub).

2. Register it in `scenes/index.ts`:

   ```ts
   import MyScene from "./MyScene.svelte";
   export const scenes = {
     "toast-error-decoded": ToastErrorDecoded,
     "my-scene": MyScene,
   };
   ```

3. Render it:

   ```sh
   packages/ui-components/harness/screenshot.sh my-scene /tmp/my-scene.png
   ```

Rendering an unknown scene prints the list of registered scenes.

## How it stubs heavy dependencies

`vite.config.ts` uses an alias **array** (order matters — specific stubs before
the `$lib` prefix):

| import                  | replaced with              | why                                                  |
| ----------------------- | -------------------------- | ---------------------------------------------------- |
| `$app/stores`           | `stubs/app-stores.ts`      | SvelteKit page/navigating/updated stores (no router) |
| `$app/navigation`       | `stubs/app-navigation.ts`  | `goto`/`invalidate` no-ops                           |
| `$app/environment`      | `stubs/app-environment.ts` | `browser = true`                                     |
| `@rainlanguage/raindex` | `stubs/raindex.ts`         | inert no-op for the wasm bindings (no wasm init)     |
| `$lib`                  | `../src/lib`               | the real component source                            |

When a new scene imports a runtime value from `@rainlanguage/raindex` that isn't
already exported by `stubs/raindex.ts`, add it there (type-only imports are
erased and need nothing). Add new `$app/*` or third-party stubs the same way.

## Layout

```text
harness/
  index.html        vite entry, #app mount point
  main.ts           reads ?scene=<name>, mounts the scene
  vite.config.ts    standalone config: svelte plugin + alias stubs
  tailwind.config.cjs / postcss.config.cjs   tailwind (incl. flowbite classes)
  tsconfig.json     harness-local tsconfig
  scenes/           scene registry + scene components (fixtures)
  stubs/            alias-stub modules for heavy deps
  screenshot.mjs    driver: start vite -> drive Chromium -> write PNG
  screenshot.sh     nix wrapper around the driver (chromium + fonts + node)
  examples/         reference PNGs produced by the harness
```

## Other packages (webapp)

The same `harness/` pattern ports to `packages/webapp`: copy this directory to
`packages/webapp/harness`, point the `$lib` alias at `webapp/src/lib`, and
alias-stub webapp's own stores (`$lib/stores/wagmi`,
`$lib/stores/localDbStatus`, ...) the same way `stubs/` does here. That is what
the webapp-only close-candidate components need — e.g. `DepositModal` (#550),
`WithdrawModal` (#570), `Sidebar`. Components that live in `ui-components` (e.g.
`OrderDetail` for the order-page IO ratio, #588) are added as scenes here
directly.

## Notes

- `screenshot.sh` uses `--headless=new --virtual-time-budget` so the component's
  `onMount` work and Svelte transitions settle before capture, and
  `--force-device-scale-factor=2` for crisp text.
- The driver writes a minimal `.svelte-kit/tsconfig.json` if one is absent, so
  the harness works from a fresh clone without a full `svelte-kit sync` (which
  needs the SvelteKit adapter that a standalone `ui-components` install omits).
  `.svelte-kit/` is gitignored.
- Run the driver directly (skipping nix) if you already have a browser:
  `CHROMIUM=/path/to/chromium node harness/screenshot.mjs --scene <name> --out /abs/out.png`.
