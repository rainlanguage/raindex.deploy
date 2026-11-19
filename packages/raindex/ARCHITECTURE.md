# @rainlanguage/raindex — Architecture

This package is the JavaScript/TypeScript SDK that exposes Raindex functionality
to web and Node.js consumers. It packages the Rust WASM crate surface (primarily
`raindex_js_api`, plus re-exports from sibling crates) into a single,
installable NPM module with CJS and ESM entry points.

The SDK is designed to work in browsers, Node.js, and hybrid runtimes. It embeds
the compiled `.wasm` bytes directly in the published bundle so consumers do not
need network fetches or filesystem access at runtime.

## Overview

- Purpose
  - Provide a WASM-backed API for: YAML parsing/validation, raindex queries
    (subgraph), quoting, vault management, transaction calldata generation
    (add/remove, deposit/withdraw), builder helpers to deploy orders from
    dotrain, and low-level hashing/ABI helpers.
- Targets
  - ESM (browser) and CJS (Node.js) builds are both published.
  - The WASM is base64-embedded to avoid runtime `fetch`/`fs` requirements.
- Upstream crates
  - Backed by `raindex_js_api` (WASM cdylib) which re-exports
    `raindex_app_settings`, `raindex_common`, and `raindex_subgraph_client` for
    a unified JS surface.

Typical import

```ts
import {
  getOrderHash,
  parseYaml,
  RaindexClient,
  RaindexOrderBuilder,
} from "@rainlanguage/raindex";
```

## Build & Packaging

All builds should be run inside a Nix shell to ensure toolchain parity
(`nix develop -c <cmd>`).

- Entry generation (`scripts/build.js`)
  - Writes thin top-level entry files: `cjs.js` (CommonJS re-export), `esm.js`
    (ESM re-export), plus `.d.ts` stubs.
  - Creates `dist/cjs` and `dist/esm` directories.
  - Invokes `npm run build-wasm` to compile the Rust workspace to
    `wasm32-unknown-unknown` in release mode (excludes CLI and integration
    tests).
  - For each package (currently just `js_api`), calls `scripts/buildPackage.js`
    to produce JS bindings and package artifacts.
- WASM binding & embedding (`scripts/buildPackage.js`)
  - Runs `wasm-bindgen` twice to generate Node (CJS) and Web (ESM) wrappers from
    the compiled `.wasm`. The `wasm-bindgen` binary comes from the Nix
    environment.
  - Reads the generated `.wasm` files and writes `dist/cjs/raindex_wbg.json` and
    `dist/esm/raindex_wbg.json` containing base64-encoded bytes.
  - Rewrites the generated JS to:
    - CJS: read bytes from the embedded JSON via `Buffer.from(base64)` and
      initialize the module without touching the filesystem.
    - ESM: import the embedded JSON and use top-level `await __wbg_init(bytes)`
      to initialize the WASM before exporting symbols.
  - Copies type declarations to `dist/*/index.d.ts` and prefixes generated files
    with a note that they are auto-generated.
- Prepublish bootstrap (`scripts/setup.js`)
  - If `./dist` exists, exits early (supporting installs from already-built
    tarballs).
  - Otherwise, cleans temp/outputs and runs the full build inside Nix:
    `nix develop -c node scripts/build`.
- Type checking & tests
  - `npm run check` runs `tsc` over the built JS to validate the emitted types.
  - `npm run test` executes Vitest suites under `test/` against the built
    artifacts.

Key commands

- Build: `nix develop -c npm run build`
- Test: `nix develop -c npm run test`
- Docs: `nix develop -c npm run docs`

## Directory Layout

- `cjs.js`, `esm.js` — Top-level re-exports pointing at `dist/` (published
  files).
- `cjs.d.ts`, `esm.d.ts` — Type re-export stubs for consumers.
- `dist/` — Build output (published)
  - `cjs/`
    - `index.js` — Auto-generated CommonJS glue that initializes WASM from
      `raindex_wbg.json`.
    - `index.d.ts` — Type declarations.
    - `raindex_wbg.json` — Base64-encoded WASM bytes.
  - `esm/`
    - `index.js` — Auto-generated ESM glue with top-level `await` for WASM init.
    - `index.d.ts` — Type declarations.
    - `raindex_wbg.json` — Base64-encoded WASM bytes.
- `scripts/`
  - `build.js` — Orchestrates the full build.
  - `buildPackage.js` — Runs `wasm-bindgen`, embeds WASM, writes JS/TS outputs.
  - `setup.js` — Prepublish bootstrap inside Nix.
- `test/` — Vitest suites exercising bindings (bindings/common/js_api).
- `typedoc.json`, `tsconfig.json` — Documentation and TS settings for the
  published surface.
- `README.md` — End-user SDK guide with examples.

## Exports & API Surface

The package re-exports the WASM-bound API from the Rust crates. Representative
items:

- Functions
  - `parseYaml`, `getOrderHash`, `getTakeOrders5Calldata`, `keccak256`,
    `keccak256HexString`.
- High-level classes (selected)
  - `RaindexClient` — raindex queries (orders, trades, vaults, quotes,
    transactions) across configured networks/subgraphs. Constructor is async
    (`await RaindexClient.new(...)`) and accepts an optional
    `{ localDb, statusCallback }` object for local DB sync when the YAML has
    `local-db-sync` sections. The sync scheduler starts automatically when
    configured and shuts down via Drop.
  - `RaindexOrder`, `RaindexVault`, `RaindexTrade`, `RaindexTransaction`,
    `RaindexVaultsList`, etc.
  - `DotrainOrder`, `RaindexOrderBuilder`, `DotrainRegistry` — dotrain parsing,
    builder orchestration, registry fetching (including `getRaindexYaml()` for
    token queries), and deployment calldata.
  - `RaindexYaml` — typed access to networks, tokens (via `getTokens()`),
    raindexes, subgraphs, deployers, accounts, metaboards.
  - `Float` — arbitrary-precision float utilities used across the API.
- Errors & results
  - Most methods return `WasmEncodedResult<T>` with either `{ value }` or
    `{ error: { msg, readableMsg } }` for ergonomic, user-readable error
    handling in JS.

Notes on runtime behavior

- ESM builds use top-level `await` to initialize the WASM module before exports
  are used. Ensure your bundler/runtime supports top-level await.
- No network fetches are performed to load the WASM bytes; they are embedded via
  JSON.

## How It Fits The Workspace

- Rust crates under `crates/*` implement the core logic. `raindex_js_api`
  compiles to WASM and re-exports pieces of `common`, `settings`, `subgraph`,
  and others for a cohesive JS surface.
- This package is the NPM wrapper that compiles those crates for WASM, generates
  JS glue, and publishes the resulting SDK.
- Consumers use only `@rainlanguage/raindex`; no direct interaction with the
  Rust build is required.

## Testing & Documentation

- Tests: Vitest suites under `test/` validate representative flows:
  orders/vaults/trades queries, quoting, calldata generation, builder flows, and
  error surfaces.
- Docs: `npm run docs` builds TypeDoc from the emitted `.d.ts` for hosted API
  documentation.

## Publishing & Versioning

- The `prepublish` script ensures the package is fully rebuilt within a Nix
  shell and includes the embedded WASM. Tarballs contain `dist/` and thin
  top-level entry points.
- Node.js >= 22 is required (see `package.json#engines`). A small `buffer`
  dependency is bundled for ESM environments that lack a native `Buffer`.

## Caveats & Tips

- Always run build/test inside `nix develop` so `wasm-bindgen`, Rust toolchains,
  and targets are available.
- If you add new WASM crates/exports in the workspace, extend the `packages`
  array in `scripts/build.js` and mirror any binding tweaks in
  `scripts/buildPackage.js`.
- If you see initialization issues in the browser, confirm your bundler supports
  top-level await and that `raindex_wbg.json` is included in the bundle.

This document explains what the `packages/raindex` directory is for, how the
WASM artifacts are produced and embedded, what gets exported to consumers, and
how it connects to the rest of the Raindex workspace.
