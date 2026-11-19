# @rainlanguage/webapp — Architecture

This package is the SvelteKit web application for exploring and interacting with
Rain Raindex. It composes the `@rainlanguage/ui-components` library with the
WASM‑backed SDK `@rainlanguage/raindex` to provide a browser UI for:

- Browsing orders, trades and vaults across networks
- Viewing order detail and performing actions (remove, deposit, withdraw)
- Deploying “algorithmic orders” from a dotrain registry via a guided builder
- Managing wallet connection and transaction flows

## Overview

- Purpose
  - End‑user web UI showcasing Raindex features and the reusable
    `@rainlanguage/ui-components` surface.
  - Loads workspace settings from YAML, initializes a `RaindexClient`, and wires
    providers for state, wallet and transactions.
- Targets
  - Client‑side SvelteKit app (SSR disabled) built with Vite and Tailwind.
  - Deployed using the Vercel adapter (Node.js 22 runtime).
- Upstream libraries
  - UI: `@rainlanguage/ui-components`, `flowbite-svelte`,
    `@tanstack/svelte-query`.
  - SDK: `@rainlanguage/raindex` (WASM), `@rainlanguage/float`.
  - Wallet: `viem`, `@wagmi/core`, `@reown/appkit` +
    `@reown/appkit-adapter-wagmi`.

Typical development

```bash
cd packages/webapp
nix develop -c npm run dev
```

## Runtime & State

- App bootstrap (`src/routes/+layout.ts`)
  - Loads the dotrain registry (`REGISTRY_URL` or `?registry=` override) via the
    WASM `DotrainRegistry` and constructs a `RaindexClient` from the registry’s
    shared settings. Construction is a single async call that accepts an
    optional `{ localDb, statusCallback }` object; the scheduler starts
    automatically when `local-db-sync` is configured in the YAML.
  - Exposes a set of Svelte stores (selected chains, active accounts, filters,
    etc.) to child routes.
  - `export const ssr = false;` — the app renders client‑side only.
- Layout shell (`src/routes/+layout.svelte`)
  - Wraps the app with providers:
    - `ToastProvider` for notifications
    - `WalletProvider` for account context
    - `QueryClientProvider` (TanStack Query) for data caching
    - `TransactionProviderWrapper` and `FixedBottomTransaction` for tx UX
    - `RaindexClientProvider` to pass the initialized client to UI components
  - Initializes wallet on mount and surfaces any initialization error to the
    user.
- Wallet integration (`src/lib/stores/wagmi.ts`,
  `src/lib/services/handleWalletInitialization.ts`)
  - Configures Wagmi + AppKit with injected and WalletConnect connectors.
  - Requires `PUBLIC_WALLETCONNECT_PROJECT_ID` (see Configuration below).
  - Tracks connection, chain, and signer via Svelte stores and reacts to account
    changes.
- Modal orchestration (`src/lib/services/modal.ts`)
  - Creates Svelte modals for deposit/withdraw/confirmation and a custom
    “withdraw all” flow by instantiating components onto `document.body`.

## Routes & Features

- `/` — Home. Static copy and getting‑started content via `Homepage.svelte`.
- `/orders`
  - Lists orders via `OrdersListTable` from `@rainlanguage/ui-components`.
  - Detail route: `/orders/[chainId]-[raindex]-[orderHash]` displays
    `OrderDetail` and wires actions:
    - Remove order
    - Deposit / Withdraw / Withdraw All to/from vaults
- `/vaults`
  - Lists vaults using `VaultsListTable`; supports filtering, active accounts,
    and bulk withdraw.
  - Detail route: `/vaults/[chainId]-[raindex]-[id]` (components handle the
    heavy lifting inside `ui-components`).
- `/markets`
  - Public Raindex market page for direct quote-token markets discovered from
    active Raindex orders. It renders the same SDK snapshot model served by the
    market-data REST API, including 24-hour statistics and executable depth.
- `/deploy`
  - Loads a dotrain registry (`?registry=` query param or default
    `REGISTRY_URL`), validates orders, and shows valid/invalid sections.
  - Nested routes:
    - `/deploy/[orderName]` — loads the dotrain and order details
    - `/deploy/[orderName]/[deploymentKey]` — fetches deployment detail with
      `RaindexOrderBuilder.getDeploymentDetail`, then renders a builder for
      composing calldata
- `/license` — Static license information.

## Directory Layout

- `src/routes/`
  - `+layout.ts` — Fetch settings, build `RaindexClient`, define app‑wide
    stores, disable SSR.
  - `+layout.svelte` — Provider composition and app shell (sidebar + content
    area).
  - `orders/` — Orders list and dynamic order detail.
  - `vaults/` — Vaults list and dynamic vault detail.
  - `deploy/` — Registry load/validation, order selection, and deployment
    builder routes.
- `src/lib/`
  - `components/` — App‑specific wrappers (Sidebar, modals, loaders, error page,
    etc.).
  - `services/` — Side‑effectful helpers (wallet init, transactions, modal
    helpers, deposit/withdraw flows).
  - `stores/` — Svelte stores for settings, wagmi state, loading flags, and
    toasts.
  - `types/` — Small app‑local type helpers.
- `src/app.*` — SvelteKit app template, global CSS, and global TS types.
- `static/` — Static assets.
- `tailwind.config.ts` — Tailwind setup (includes `ui-components` and Flowbite
  paths).
- `svelte.config.js` — Vercel adapter (Node.js 22 runtime) and preprocessing.
- `vite.config.ts` — Build and Vitest configuration (JS DOM, inline deps, env
  passthrough).

## Build, Test, and Dev

Run inside a Nix shell for tool parity (`nix develop -c <cmd>`):

- Dev server: `nix develop -c npm run dev`
- Build: `nix develop -c npm run build`
- Preview: `nix develop -c npm run preview`
- Lint/format/check: `npm run format`, `npm run lint`, `npm run check`
- Tests: `nix develop -c npm run test` (Vitest, jsdom)

## Configuration

- Copy `.env.example` to `.env` and set:

```bash
PUBLIC_WALLETCONNECT_PROJECT_ID=<your_walletconnect_project_id>
PUBLIC_MARKET_DATA_API_URL=https://api.raindex.finance
```

- Notes
  - Use the `PUBLIC_` prefix (SvelteKit convention) for variables that must be
    available in the browser.
  - Never commit secrets. Use Vercel/hosted environment variables for deploys.
  - Deploy registry: `REGISTRY_URL` in `src/lib/constants.ts` can be overridden
    via the `?registry=` query parameter (persisted by `RegistryManager`).

## How It Fits The Workspace

- Rust crates under `crates/*` implement the core logic; `@rainlanguage/raindex`
  packages a WASM surface to consume from JS.
- `@rainlanguage/ui-components` provides reusable Svelte components, transaction
  plumbing, and providers.
- This webapp stitches both together into a cohesive UI. It lives in the JS
  workspace and is not part of the Cargo workspace.

## Caveats & Tips

- SSR is disabled; avoid Node‑only APIs or server‑only assumptions in route/load
  code.
- Always run inside `nix develop` so the correct Node and build tools are
  available.
- If you modify or add routes, ensure Tailwind’s `content` globs include your
  files for proper styling.
- Wallet init issues usually stem from a missing/invalid
  `PUBLIC_WALLETCONNECT_PROJECT_ID` or blocked popups.
- When adding new flows that touch the blockchain, prefer composing
  `@rainlanguage/ui-components` helpers and passing the `RaindexClient` from the
  layout provider.

This document explains the purpose and structure of `packages/webapp`, how the
app boots and wires providers, where key features live, and how it integrates
with the rest of the Rain Raindex workspace.
