# Raindex CLI — LocalDB

The legacy subcommands in this directory have been replaced by a single
orchestration entry point: `local-db sync`. It drives the entire SQLite
pipeline—bootstrap, fetch, decode, apply, export, and manifest generation—using
the same engine that powers the browser sync.

## Pipeline Overview

- Parse the Rain settings YAML to discover networks, RPCs, raindexes, and
  per-network sync parameters.
- Optionally download and import the most recent dump referenced by each
  raindex’s `local-db-remote` manifest.
- Run the local DB engine for every raindex concurrently, fetching logs via
  HyperRPC, decoding events, fetching token metadata, and applying the resulting
  SQL into a fresh SQLite database.
- Export a gzipped SQL dump for the synced state and write an aggregated
  `manifest.yaml` that maps release URLs to the produced dumps when all jobs
  succeed.

## Usage

Run the command from the workspace root so the CLI crate is available:

```bash
nix develop -c cargo run -p raindex_cli -- local-db sync \
  --settings-yaml "https://github.com/rainlanguage/rain.strategies/blob/main/settings.yaml" \
  --api-token "$HYPERRPC_TOKEN" \
  --release-base-url "https://github.com/rainlanguage/rain.local-db.remote/releases/latest" \
  --out-root "./local-db"
```

### Arguments

- `--settings-yaml <YAML>` (required): inline contents of a valid Rain settings
  document. You can embed it with command substitution as shown above.
- `--api-token <TOKEN>` (required): HyperRPC API token used when fetching logs.
- `--release-base-url <URL>` (required): base URL that will prefix the generated
  dump filenames inside the manifest (e.g. your CDN or GitHub release path).
- `--out-root <PATH>` (optional, default `./local-db`): directory where SQLite
  databases, dumps, and the manifest are written.

## Settings YAML Expectations

The runner consumes the same schema as `crates/cli/settings.yaml`:

- `networks`: chain metadata plus RPC endpoints used for metadata reads.
- `raindexes`: each entry must reference a `network`, declare a
  `deployment-block`, and point to a `local-db-remote` manifest URL.
- `local-db-remotes`: map of manifest aliases to URLs; manifests describe
  previously published dumps that can be used as a bootstrap baseline.
- `local-db-sync`: per-network fetch configuration (batch size, concurrency,
  retry policy, finality depth).

Validation is handled by `raindex_app_settings`, and missing sections will
surface as CLI errors before any network calls are made.

## Outputs

All artifacts live under `--out-root`:

- `<chain-id>/<raindex-address>.db`: fresh SQLite database containing the synced
  state.
- `<chain-id>/<chain-id>-<raindex-address>.sql.gz`: gzipped SQL transaction with
  the data delta at the synced head.
- `manifest.yaml`: generated only when every raindex finishes successfully;
  references each dump using the provided `--release-base-url`.

Each run starts from a clean SQLite file. When a remote manifest exposes a prior
dump, it is downloaded and replayed before the new sync to avoid replaying the
entire chain from genesis.

## Operational Notes

- The command reports a per-raindex summary once all jobs finish; non-zero
  failures prevent manifest emission.
- HyperRPC endpoints use Envio's numeric chain-ID alias, so configured networks
  do not require a matching hostname entry in the CLI.
- Upload the generated `.sql.gz` files to the location represented by
  `--release-base-url` before distributing `manifest.yaml`.
