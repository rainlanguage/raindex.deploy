# Raindex Market Data API

Public market data service backed by the Raindex Rust SDK and its persistent
local indexer. It does not require API keys.

## Run locally

```sh
cargo run -p raindex_rest_api
```

The first startup creates `.raindex/market-data.sqlite`, synchronizes the
configured orderbook, and warms the market cache before accepting requests.
Swagger UI is served at `http://127.0.0.1:8000/swagger/`.

## Routes

- `GET /tickers` — 24-hour ticker data for every direct quote-token market found
  in active indexed Raindex orders, including its stable unique market ID as
  `pool_id` for the orderbook DEX schema.
- `GET /orderbook?ticker_id=...&depth=100` — executable bids and asks for one
  market.
- `GET /v1/markets` — complete active-market overview for the Raindex UI.
- `GET /v1/markets?ticker_id=...` — one market with statistics, trades, and its
  executable book.
- `GET /health` and `GET /health/detailed` — service and local-indexer health.

## Configuration

All settings are optional environment variables:

| Variable                                 | Default                                  |
| ---------------------------------------- | ---------------------------------------- |
| `RAINDEX_REGISTRY_URL`                   | Pinned `rain.strategies` registry commit |
| `RAINDEX_LOCAL_DB_PATH`                  | `.raindex/market-data.sqlite`            |
| `RAINDEX_LOG_DIR`                        | `.raindex/logs`                          |
| `RAINDEX_LOCAL_DB_READY_TIMEOUT_SECONDS` | `600`                                    |
| `RAINDEX_CACHE_TTL_SECONDS`              | `60`                                     |
| `RAINDEX_RATE_LIMIT_GLOBAL_RPM`          | `6000`                                   |
| `RAINDEX_RATE_LIMIT_PER_IP_RPM`          | `120`                                    |
| `RAINDEX_SNAPSHOT_RECENT_TRADES_LIMIT`   | `20`                                     |
| `RAINDEX_TRUSTED_PROXY_IP_HEADER`        | unset; direct socket IP is used          |

The all-market overview is refreshed in the background. Market discovery and
statistics use the persistent local index first, with the SDK's configured
subgraph fallback until the local index is ready. Executable orderbooks are
cached per ticker so an orderbook request quotes only the requested market.
Overview responses omit per-market trade arrays to keep minutely UI reads small;
requesting one `ticker_id` includes its configured recent-trade window.
Orderbooks use a fixed 1000-level SDK snapshot. A `depth` of `0` returns that
entire snapshot; positive values return at most that many levels split evenly
between bids and asks. The optional historical-trades compatibility endpoint is
intentionally deferred until the SDK can serve arbitrary indexed time ranges
without truncating to a cached 24-hour window.

Console and file logs are emitted as JSON. File logs rotate daily and retain the
most recent 14 files. Production should place `RAINDEX_LOG_DIR` on the
persistent data volume.

Set `RAINDEX_TRUSTED_PROXY_IP_HEADER` only when a trusted ingress overwrites
that header before forwarding requests. Client-provided forwarding headers are
ignored by default so public rate limits cannot be bypassed by spoofing them.
