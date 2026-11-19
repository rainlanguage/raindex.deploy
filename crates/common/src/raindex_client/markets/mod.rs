#[cfg(not(target_family = "wasm"))]
use super::order_quotes::{get_order_quotes_batch_for_pairs, RaindexOrderQuote};
use super::orders::{GetOrdersFilters, GetOrdersTokenFilter, RaindexOrder};
#[cfg(not(target_family = "wasm"))]
use super::trades::{GetTradesFilters, GetTradesTokenFilter, RaindexTrade};
use super::ChainIds;
#[cfg(not(target_family = "wasm"))]
use super::TimeFilter;
use super::{RaindexClient, RaindexError};
use alloy::primitives::{Address, B256};
#[cfg(not(target_family = "wasm"))]
use alloy::{primitives::U256, sol_types::SolValue};
#[cfg(not(target_family = "wasm"))]
use chrono::Utc;
#[cfg(not(target_family = "wasm"))]
use futures::future::join_all;
#[cfg(not(target_family = "wasm"))]
use rain_erc::erc4626::{self, Erc4626BatchResponse, Erc4626BatchVault};
#[cfg(not(target_family = "wasm"))]
use rain_math_float::Float;
use raindex_app_settings::token::TokenCfg;
#[cfg(not(target_family = "wasm"))]
use raindex_bindings::provider::mk_read_provider;
#[cfg(not(target_family = "wasm"))]
use raindex_bindings::IRaindexV7::OrderV4;
use serde_json::Value;
#[cfg(not(target_family = "wasm"))]
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
#[cfg(not(target_family = "wasm"))]
use std::future::Future;
#[cfg(not(target_family = "wasm"))]
use std::ops::{Add, Div, Mul, Sub};
use wasm_bindgen_utils::{prelude::*, wasm_export};

mod catalog;
#[cfg(not(target_family = "wasm"))]
mod orderbook;
#[cfg(not(target_family = "wasm"))]
mod trades;
mod types;

use catalog::discover_markets;
#[cfg(not(target_family = "wasm"))]
use orderbook::{
    apply_book_errors, apply_book_levels, build_canonical_variant_map, build_variant_map,
    fetch_book_levels, read_ratios,
};
#[cfg(not(target_family = "wasm"))]
use trades::{apply_trades, fetch_normalized_trades};
pub use types::*;

const MARKET_QUOTE_EXTENSION: &str = "marketQuote";
const MAX_QUERY_PAGES: u16 = 1_000;
const QUERY_PAGE_SIZE: u16 = 1_000;
#[cfg(not(target_family = "wasm"))]
const DEFAULT_ORDERBOOK_DEPTH: u16 = 100;
#[cfg(not(target_family = "wasm"))]
const DEFAULT_RECENT_TRADES_LIMIT: u16 = 20;
#[cfg(not(target_family = "wasm"))]
const TRADES_READ_TIMEOUT_MS: u64 = 8_000;
#[cfg(not(target_family = "wasm"))]
const RATIO_RPC_TIMEOUT_MS: u64 = 2_000;

async fn fetch_market_orders_paginated(
    client: &RaindexClient,
    chain_ids: Vec<u32>,
    filters: GetOrdersFilters,
) -> Result<Vec<RaindexOrder>, RaindexError> {
    let mut orders = Vec::new();
    let mut fetched = 0usize;
    let mut total_count = 0usize;

    for page in 1..=MAX_QUERY_PAGES {
        let result = client
            .get_orders(
                Some(ChainIds(chain_ids.clone())),
                Some(filters.clone()),
                Some(page),
                Some(QUERY_PAGE_SIZE),
            )
            .await?;
        let page_orders = result.orders().to_vec();
        total_count = result.total_count() as usize;
        if page_orders.is_empty() {
            return Ok(orders);
        }
        fetched = fetched.saturating_add(page_orders.len());
        orders.extend(page_orders);
        if fetched >= total_count {
            return Ok(orders);
        }
    }

    Err(RaindexError::PreflightError(format!(
        "order query exhausted the {MAX_QUERY_PAGES}-page safety limit with {fetched} of {total_count} rows"
    )))
}

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Clone, Copy)]
struct Variant {
    canonical_address: Address,
    price_multiplier: Float,
}

#[cfg(not(target_family = "wasm"))]
type VariantBuild = (HashMap<Address, Variant>, Vec<(String, String)>);

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Clone)]
struct RatioValue {
    asset_address: Address,
    assets_per_share: Float,
    formatted_assets_per_share: String,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Clone, Default)]
struct RatioRead {
    values: HashMap<Address, RatioValue>,
    block_number: Option<u64>,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Clone)]
struct BookLevel {
    canonical_address: Address,
    side: BookSide,
    price: Float,
    base_quantity: Float,
    target_quantity: Float,
    chain_id: u32,
    raindex: Address,
    order_hash: B256,
    source_token: Address,
    block_number: u64,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Debug)]
struct BookRead {
    levels: Vec<BookLevel>,
    errors: Vec<(Address, String)>,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BookSide {
    Bid,
    Ask,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Clone)]
struct NormalizedTrade {
    canonical_address: Address,
    trade: RaindexMarketTrade,
    source_log_index: Option<u64>,
    price: Float,
    base_volume: Float,
    target_volume: Float,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Copy)]
struct SnapshotReadOptions {
    observed_at: u64,
    orderbook_depth: usize,
    recent_trades_limit: usize,
    include_orderbook: bool,
    include_ratios: bool,
    include_trades: bool,
}

#[wasm_export]
impl RaindexClient {
    #[wasm_export(
        js_name = "getMarkets",
        return_description = "Quote-token markets discovered from active Raindex orders",
        unchecked_return_type = "RaindexMarket[]"
    )]
    pub async fn get_markets(
        &self,
        options: Option<MarketListOptions>,
    ) -> Result<Vec<RaindexMarket>, RaindexError> {
        discover_markets(self, &options.unwrap_or_default()).await
    }
}

#[cfg(not(target_family = "wasm"))]
impl RaindexClient {
    pub async fn get_market_snapshots(
        &self,
        options: Option<MarketSnapshotOptions>,
    ) -> Result<Vec<RaindexMarketSnapshot>, RaindexError> {
        let options = options.unwrap_or_default();
        let markets = discover_markets(self, &options.markets).await?;
        let observed_at = u64::try_from(Utc::now().timestamp())
            .map_err(|_| RaindexError::PreflightError("system time is before Unix epoch".into()))?;
        let read_options = SnapshotReadOptions {
            observed_at,
            orderbook_depth: usize::from(
                options.orderbook_depth.unwrap_or(DEFAULT_ORDERBOOK_DEPTH),
            ),
            recent_trades_limit: usize::from(
                options
                    .recent_trades_limit
                    .unwrap_or(DEFAULT_RECENT_TRADES_LIMIT),
            ),
            include_orderbook: options.include_orderbook.unwrap_or(true),
            include_ratios: options.include_ratios.unwrap_or(true),
            include_trades: options.include_trades.unwrap_or(true),
        };

        let chain_ids = markets
            .iter()
            .map(|market| market.chain_id)
            .collect::<HashSet<_>>();
        let chain_reads = chain_ids.into_iter().map(|chain_id| {
            let chain_markets = markets
                .iter()
                .filter(|market| market.chain_id == chain_id)
                .cloned()
                .collect::<Vec<_>>();
            let client = self.clone();
            async move {
                let mut snapshots = chain_markets
                    .iter()
                    .cloned()
                    .map(|market| {
                        (
                            market.id.clone(),
                            RaindexMarketSnapshot {
                                market,
                                orderbook: RaindexMarketOrderbook::default(),
                                stats: RaindexMarketStats::default(),
                                recent_trades: Vec::new(),
                                observed_at,
                                block_number: None,
                                ratio_block_number: None,
                                assets_per_share: None,
                                errors: Vec::new(),
                            },
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                populate_chain_snapshots(&client, &chain_markets, &mut snapshots, &read_options)
                    .await;
                snapshots
            }
        });
        let mut snapshots = join_all(chain_reads)
            .await
            .into_iter()
            .flatten()
            .collect::<BTreeMap<_, _>>();

        Ok(markets
            .iter()
            .filter_map(|market| snapshots.remove(&market.id))
            .collect())
    }
}

#[cfg(not(target_family = "wasm"))]
async fn populate_chain_snapshots(
    client: &RaindexClient,
    markets: &[RaindexMarket],
    snapshots: &mut BTreeMap<String, RaindexMarketSnapshot>,
    options: &SnapshotReadOptions,
) {
    let Some(first_market) = markets.first() else {
        return;
    };
    let chain_id = first_market.chain_id;
    let network = match client.get_network_by_chain_id(chain_id) {
        Ok(network) => network,
        Err(error) => {
            push_chain_error(snapshots, markets, "registry", error.to_string());
            return;
        }
    };
    let ratios = if options.include_ratios {
        let share_addresses = ratio_share_addresses(markets);
        match read_ratios(&network.rpcs, share_addresses).await {
            Ok(ratios) => ratios,
            Err(error) => {
                push_chain_error(snapshots, markets, "ratios", error);
                RatioRead::default()
            }
        }
    } else {
        RatioRead::default()
    };
    let (variant_map, variant_errors) = if options.include_ratios {
        match build_variant_map(markets, &ratios.values) {
            Ok(result) => result,
            Err(error) => {
                push_chain_error(snapshots, markets, "ratios", error.to_string());
                return;
            }
        }
    } else {
        match build_canonical_variant_map(markets) {
            Ok(variants) => (variants, Vec::new()),
            Err(error) => {
                push_chain_error(snapshots, markets, "ratios", error.to_string());
                return;
            }
        }
    };
    apply_variant_warnings(snapshots, variant_errors);
    for market in markets {
        if let Some(snapshot) = snapshots.get_mut(&market.id) {
            snapshot.ratio_block_number = ratios.block_number;
            snapshot.assets_per_share = ratios
                .values
                .get(&market.base.address)
                .map(|ratio| ratio.formatted_assets_per_share.clone());
        }
    }

    if options.include_trades {
        let trades_client = client.clone();
        let trade_markets = markets.to_vec();
        let trade_variants = variant_map.clone();
        match with_market_timeout(
            async move {
                fetch_normalized_trades(
                    &trades_client,
                    &trade_markets,
                    &trade_variants,
                    options.observed_at,
                )
                .await
            },
            TRADES_READ_TIMEOUT_MS,
        )
        .await
        {
            Some(Ok(trades)) => apply_trades(
                snapshots,
                markets,
                trades.window,
                trades.latest,
                options.recent_trades_limit,
            ),
            Some(Err(error)) => push_chain_error(snapshots, markets, "trades", error.to_string()),
            None => push_chain_error(
                snapshots,
                markets,
                "trades",
                format!("trade read timed out after {TRADES_READ_TIMEOUT_MS}ms"),
            ),
        }
    }

    if options.include_orderbook {
        match fetch_book_levels(client, markets, &variant_map).await {
            Ok(book) => {
                apply_book_levels(snapshots, markets, book.levels, options.orderbook_depth);
                apply_book_errors(snapshots, markets, book.errors);
            }
            Err(error) => push_chain_error(snapshots, markets, "orderbook", error.to_string()),
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn ratio_share_addresses(markets: &[RaindexMarket]) -> Vec<Address> {
    markets
        .iter()
        .filter(|market| {
            market.base.unwrapped_address.is_some() || market.base.legacy_address.is_some()
        })
        .flat_map(|market| [Some(market.base.address), market.base.legacy_address])
        .flatten()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(not(target_family = "wasm"))]
async fn with_market_timeout<F>(future: F, timeout_ms: u64) -> Option<F::Output>
where
    F: Future,
{
    tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), future)
        .await
        .ok()
}

#[cfg(not(target_family = "wasm"))]
fn market_raindex_addresses(markets: &[RaindexMarket]) -> Vec<Address> {
    let mut addresses = markets
        .iter()
        .flat_map(|market| market.raindex_addresses.iter().copied())
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    addresses
}

#[cfg(not(target_family = "wasm"))]
fn push_chain_error(
    snapshots: &mut BTreeMap<String, RaindexMarketSnapshot>,
    markets: &[RaindexMarket],
    source: &str,
    message: String,
) {
    markets.iter().for_each(|market| {
        push_error(snapshots, &market.id, source, message.clone());
    });
}

#[cfg(not(target_family = "wasm"))]
fn push_error(
    snapshots: &mut BTreeMap<String, RaindexMarketSnapshot>,
    market_id: &str,
    source: &str,
    message: String,
) {
    push_issue(
        snapshots,
        market_id,
        source,
        RaindexMarketDataErrorSeverity::Error,
        message,
    );
}

#[cfg(not(target_family = "wasm"))]
fn push_warning(
    snapshots: &mut BTreeMap<String, RaindexMarketSnapshot>,
    market_id: &str,
    source: &str,
    message: String,
) {
    push_issue(
        snapshots,
        market_id,
        source,
        RaindexMarketDataErrorSeverity::Warning,
        message,
    );
}

#[cfg(not(target_family = "wasm"))]
fn apply_variant_warnings(
    snapshots: &mut BTreeMap<String, RaindexMarketSnapshot>,
    warnings: Vec<(String, String)>,
) {
    warnings.into_iter().for_each(|(market_id, message)| {
        push_warning(snapshots, &market_id, "ratios", message);
    });
}

#[cfg(not(target_family = "wasm"))]
fn push_issue(
    snapshots: &mut BTreeMap<String, RaindexMarketSnapshot>,
    market_id: &str,
    source: &str,
    severity: RaindexMarketDataErrorSeverity,
    message: String,
) {
    if let Some(snapshot) = snapshots.get_mut(market_id) {
        snapshot.errors.push(RaindexMarketDataError {
            source: source.to_string(),
            severity,
            message,
        });
    }
}

#[cfg(not(target_family = "wasm"))]
fn float_cmp_asc(a: Float, b: Float) -> Ordering {
    if a.lt(b).unwrap_or(false) {
        Ordering::Less
    } else if a.gt(b).unwrap_or(false) {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

#[cfg(not(target_family = "wasm"))]
fn float_cmp_desc(a: Float, b: Float) -> Ordering {
    float_cmp_asc(b, a)
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    fn market(
        base: Address,
        unwrapped_address: Option<Address>,
        legacy_address: Option<Address>,
    ) -> RaindexMarket {
        let token = |address, symbol: &str| RaindexMarketToken {
            chain_id: 8453,
            address,
            name: symbol.to_string(),
            symbol: symbol.to_string(),
            decimals: Some(18),
            logo_uri: None,
            extensions: None,
            unwrapped_address: None,
            legacy_address: None,
            receipt_address: None,
            variants: vec![address],
        };
        let mut base_token = token(base, "BASE");
        base_token.unwrapped_address = unwrapped_address;
        base_token.legacy_address = legacy_address;
        RaindexMarket {
            id: "market".into(),
            ticker_id: "ticker".into(),
            chain_id: 8453,
            base: base_token,
            quote: token(Address::repeat_byte(0x99), "QUOTE"),
            raindex_addresses: vec![],
        }
    }

    fn snapshot(market: RaindexMarket) -> RaindexMarketSnapshot {
        RaindexMarketSnapshot {
            market,
            orderbook: RaindexMarketOrderbook::default(),
            stats: RaindexMarketStats::default(),
            recent_trades: Vec::new(),
            observed_at: 0,
            block_number: None,
            ratio_block_number: None,
            assets_per_share: None,
            errors: Vec::new(),
        }
    }

    #[test]
    fn ratio_reads_exclude_ordinary_tokens() {
        assert!(
            ratio_share_addresses(&[market(Address::repeat_byte(0x11), None, None)]).is_empty()
        );
    }

    #[test]
    fn ratio_reads_include_only_declared_share_variants() {
        let canonical = Address::repeat_byte(0x11);
        let underlying = Address::repeat_byte(0x22);
        let legacy = Address::repeat_byte(0x33);
        let addresses = ratio_share_addresses(&[market(canonical, Some(underlying), Some(legacy))]);

        assert_eq!(addresses.len(), 2);
        assert!(addresses.contains(&canonical));
        assert!(addresses.contains(&legacy));
        assert!(!addresses.contains(&underlying));
    }

    #[test]
    fn unavailable_variant_ratios_are_nonfatal_warnings() {
        let market = market(
            Address::repeat_byte(0x11),
            Some(Address::repeat_byte(0x22)),
            Some(Address::repeat_byte(0x33)),
        );
        let mut snapshots = BTreeMap::from([(market.id.clone(), snapshot(market.clone()))]);

        apply_variant_warnings(
            &mut snapshots,
            vec![(
                market.id.clone(),
                "legacy ERC4626 ratio is unavailable".into(),
            )],
        );

        assert_eq!(snapshots[&market.id].errors.len(), 1);
        assert_eq!(
            snapshots[&market.id].errors[0].severity,
            RaindexMarketDataErrorSeverity::Warning
        );
    }

    #[tokio::test]
    async fn market_timeout_returns_completed_work() {
        assert_eq!(with_market_timeout(async { 42 }, 100).await, Some(42));
    }

    #[tokio::test]
    async fn market_timeout_cancels_stalled_work() {
        assert_eq!(
            with_market_timeout(std::future::pending::<()>(), 1).await,
            None
        );
    }
}
