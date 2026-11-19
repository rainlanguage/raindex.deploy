Rain Raindex Bindings — Architecture

Summary

- Purpose: Provide strongly typed Rust bindings to the Rain Raindex Solidity
  contracts and small utilities for calling them from Rust (native and WASM).
  The crate centralizes ABI-derived types and call helpers so the rest of the
  workspace can construct calldata, perform reads, and expose safe JS-facing
  types.
- Scope: ABI-based type generation via Alloy, a read‑only provider builder with
  multi‑RPC fallback, and WASM interop shims (TypeScript typings + conversions).

File Layout

- Cargo.toml: Declares the crate `raindex_bindings`. Key deps: `alloy`
  (codegen + types + RPC), `serde` (Serialize/Deserialize), `tower` (layers),
  `url`, `thiserror`. For WASM builds it uses `wasm-bindgen-utils` and
  `wasm-bindgen-test` for tests.
- src/lib.rs: Declares contract bindings using Alloy’s `sol!` macro and
  re‑exports internal modules. Conditionally includes WASM modules.
- src/provider.rs: Builds a read‑only provider with multi‑RPC fallback and
  sensible default request fillers.
- src/js_api.rs (wasm only): JS/WASM interop. Implements wasm conversion traits
  and custom TypeScript interfaces for selected ABI types used in the order
  builder.
- src/wasm_traits.rs (wasm only): Utility trait to convert JS `BigInt` to `U256`
  with negative/overflow handling plus tests.

Generated Solidity Bindings (Alloy `sol!`)

- The crate uses `alloy::sol!` to generate Rust modules, types, and (optionally)
  RPC call helpers from contract ABIs produced by Foundry. ABIs are read from
  the repository’s `out/` directory.

- Bindings defined in `src/lib.rs`:
  - `IRaindexV5, "../../out/IRaindexV5.sol/IRaindexV5.json"`
    - Attributes: `#![sol(all_derives = true, rpc)]`,
      `#![sol(extra_derives(serde::Serialize, serde::Deserialize))]`.
    - Effect: Generates Rust types for all ABI structs/enums/events and RPC
      instance helpers for calling the contract. Adds `Serialize/Deserialize`
      derives for ergonomic (de)serialization.
  - `Raindex, "../../out/Raindex.sol/Raindex.json"`
    - Attributes: `#![sol(all_derives = true)]`,
      `#![sol(extra_derives(serde::Serialize, serde::Deserialize))]`.
    - Effect: Same as above but without the `rpc` helpers. This is sufficient
      for constructing/calculating calldata (e.g., `multicallCall`) without
      needing a bound instance type.
  - `IERC20, "../../out/IERC20.sol/IERC20.json"`
    - Attributes: `#![sol(all_derives = true, rpc)]`.
    - Effect: ERC‑20 call helpers and Rust types (e.g., `approveCall`,
      `allowanceCall`) with RPC instance support.
  - `ERC20, "../../out/ERC20.sol/ERC20.json"`
    - Attributes: `#![sol(all_derives = true)]`.
    - Effect: Full types for concrete ERC‑20; used for calldata construction and
      decoding when instance helpers aren’t required.

- Practical result of the `sol!(… rpc)` attribute:
  - For `IRaindexV5` and `IERC20`, you can construct an instance bound to a
    provider and call methods with strong typing, for example:
    `let ob = IRaindexV5Instance::new(address, provider.clone()); ob.quote3(config).await?`.
  - For all bindings, you can still directly use generated call structs, e.g.,
    `IRaindexV5::removeOrder4Call { ... }.abi_encode()` or
    `Raindex::multicallCall { ... }`.

- Derives and defaults:
  - `all_derives = true` enables useful traits on generated types, including
    `Clone`, `Debug`, `Default`, `Eq`, `PartialEq`, and more, which the codebase
    relies on (e.g., `OrderV4::default()`).
  - `extra_derives(serde::Serialize, serde::Deserialize)` ensures all ABI types
    can be serialized to/from JSON, which is critical for CLI/builder I/O and
    WASM interop.

ABI Source of Truth

- The referenced JSON files under `out/` are produced by Foundry
  (`forge build`). If ABIs change, re‑build the contracts so `out/.../*.json`
  stays in sync. The `sol!` macro reads these on compile.

Provider Utilities (`src/provider.rs`)

- Type alias
  - `pub type ReadProvider = FillProvider<JoinedRecommendedFillers, RootProvider<AnyNetwork>, AnyNetwork>;`
  - Meaning: a provider stack consisting of an `RpcClient` → `RootProvider` for
    `AnyNetwork`, with the “recommended” fillers layered in
    (gas/nonce/chain-id/etc.) to auto‑complete missing call fields.

- `mk_read_provider(rpcs: &[Url]) -> Result<ReadProvider, ReadProviderError>`
  - Accepts one or more RPC URLs for the same chain.
  - Builds an HTTP transport stack wrapped with `FallbackLayer` so requests
    survive individual RPC outages by trying the next available transport.
  - `with_active_transport_count(NonZeroUsize::new(size)?)` activates as many
    transports as provided URLs, enabling concurrent/fallback behavior.
  - Connects an `RpcClient` to an `AnyNetwork` provider via
    `ProviderBuilder::new_with_network::<AnyNetwork>().connect_client(client)`.
  - Errors:
    - `UrlParse`: invalid URL parsing failed (bubbled from `url` crate when
      constructing inputs elsewhere).
    - `NoRpcs`: the input slice was empty (no transports to build).

- When to use
  - Use this provider for read‑only flows (e.g., quoting, allowances, balances)
    needing resilience across multiple public/provider endpoints.
    Signing/submitting transactions is performed elsewhere in the workspace;
    this crate doesn’t include wallet/signing.

WASM and JS Interop

- Conditional compilation: `src/js_api.rs` and `src/wasm_traits.rs` are compiled
  only for `target_family = "wasm"`.

- `src/js_api.rs` bridges key ABI types to predictable TypeScript shapes for the
  order builder, using the workspace’s `wasm-bindgen-utils` macros:
  - `impl_wasm_traits!(T)` implements glue code to convert between Rust and JS
    values (e.g., Serde + wasm-bindgen interop) for the given type.
  - `impl_custom_tsify!(T, "…TS interface…")` pins the exact TypeScript surface
    for WASM consumers. This avoids relying on auto‑generated typings that may
    drift with upstream changes.

- Exposed interfaces (hand‑written TS definitions):
  - `IOV2`: `{ token: string; vaultId: string; }`
  - `QuoteV3`:
    `{ order: OrderV4; inputIOIndex: string; outputIOIndex: string; signedContext: SignedContextV2[]; }`
  - `OrderV4`:
    `{ owner: string; evaluable: EvaluableV4; validInputs: IOV2[]; validOutputs: IOV2[]; nonce: string; }`
  - `EvaluableV4`: `{ interpreter: string; store: string; bytecode: string; }`
  - `SignedContextV2`:
    `{ signer: string; domainSeparator: string; context: string[]; signature: string; }`
  - `TakeOrderConfigV5`:
    `{ order: OrderV4; inputIOIndex: string; outputIOIndex: string; signedContext: SignedContextV2[]; }`
  - `TakeOrdersConfigV6`:
    `{ minimumIO: string; maximumIO: string; maximumIORatio: string; IOIsInput: string; orders: TakeOrderConfigV5[]; data: string; }`

- Why many fields are `string` in TS
  - Large numeric values (e.g., `U256`) are represented as strings to avoid
    precision issues and to keep interop predictable across JS runtimes. Hex
    strings are used for byte data. This matches how the rest of the workspace
    serializes on the boundary.

- WASM tests (in `src/js_api.rs`)
  - Use `wasm-bindgen-test` to validate that the serialized JS values expose
    exactly the properties declared by the TS interfaces (e.g.,
    `'owner' in obj`). The tests create default Rust values, convert with
    `to_js_value`, and assert property presence.

WASM Numeric Conversion (`src/wasm_traits.rs`)

- Trait: `TryIntoU256` for JS `BigInt` → `U256` conversion.
  - Implementation parses the stringified `BigInt` into `U256` using Alloy’s
    `ruint` parser.
  - Error handling covers:
    - Negative values → `ParseError::InvalidDigit('-')`.
    - Overflow beyond `U256::MAX` → `ParseError::BaseConvertError(Overflow)`.
  - Tests cover `0`, a small positive, `U256::MAX`, overflow (`2^256`), and
    negatives.

How This Crate Is Used Elsewhere

- Quote engine (`crates/quote`):
  - Imports `IRaindexV5::IRaindexV5Instance` and `mk_read_provider`.
  - Binds the instance to a provider and calls `quote3` for many orders via
    Alloy’s multicall helper.
- Common utilities (`crates/common`):
  - Uses ABI‑generated call structs (e.g., `deposit3Call`, `withdraw3Call`,
    `removeOrder4Call`, `IERC20::approveCall`) to build calldata for
    transactions.
- JS API (`crates/js_api`):
  - Builds order builder calldata for approvals/deposits/add‑order and uses call
    structs like `Raindex::multicallCall` without needing on‑chain RPC instance
    helpers.
- CLI (`crates/cli`):
  - Consumes ABI struct types such as `OrderV4` and `IOV2` to construct orders
    from user inputs.

Design Choices and Rationale

- Keep bindings centralized: All ABI structs and call helpers live in one place
  so downstream crates share a single, consistent type system.
- Split `rpc` vs. non‑`rpc` bindings:
  - `IRaindexV5`/`IERC20` include RPC instance helpers for ergonomic reads.
  - `Raindex`/`ERC20` are included without `rpc` when only calldata
    construction/decoding is required.
- Provider is read‑only by design: This crate does not handle signing or nonce
  management beyond auto‑fillers. Submission/signature flows live in other
  crates.
- WASM boundary is explicit: Hand‑authored TS interfaces lock the surface area
  for the webapp, preventing accidental breaking changes from codegen drift.

Error Handling

- `ReadProviderError` enumerates provider construction failures:
  - `UrlParse(url::ParseError)` and `NoRpcs`.
- ABI call errors, revert decoding, and multicall aggregation are handled in
  consumer crates (e.g., `quote`), leveraging these bindings for
  encoding/decoding.

Build and Test

- Build (workspace): `nix develop -c cargo build -p raindex_bindings`.
- Tests (native and wasm; wasm executed via runner in the flake):
  - Native: `nix develop -c cargo test -p raindex_bindings`.
  - WASM: the workspace’s Nix config sets
    `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER='wasm-bindgen-test-runner'` and
    runs `cargo test --target wasm32-unknown-unknown -p raindex_bindings`.

Examples

- Constructing a read provider and querying via an instance (native):
  - Parse URLs to `url::Url` and build:
    `let provider = mk_read_provider(&rpcs)?;`
  - Bind to a raindex:
    `let ob = IRaindexV5Instance::new(raindex_addr, provider.clone());`
  - Call a view: `let quote = ob.quote3(config).await?;`

- Building calldata without an instance:
  - `use raindex_bindings::IRaindexV5::removeOrder4Call;`
  - Construct the struct and encode:
    `let bytes = removeOrder4Call { order, tasks }.abi_encode();`

Limitations and Notes

- Not a signer: The provider is for reads; transaction signing/broadcast is out
  of scope.
- Assumes ABIs are current: If Foundry output changes, re‑build before compiling
  Rust.
- `AnyNetwork` provider: Consumers are responsible for ensuring the supplied
  RPCs point to the intended chain.

Updating Bindings

- Modify the Solidity contracts as needed, run `nix develop -c forge build`,
  then rebuild Rust. If new structs/functions are added to the ABIs, they will
  appear under the corresponding Rust modules after recompilation. Add/update
  WASM TS interfaces in `src/js_api.rs` as needed for order builder usage.
