use crate::{cache::MarketSnapshotCache, provider::RaindexProvider};
use async_trait::async_trait;
use raindex_common::raindex_client::{
    local_db::LocalDbSyncSnapshot, markets::RaindexMarketSnapshot,
};
use std::{sync::Arc, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexerHealth {
    pub configured: bool,
    pub healthy: bool,
    pub sync_healthy: bool,
    pub network_count: usize,
    pub orderbook_count: usize,
    pub snapshot_ready: bool,
    pub snapshot_last_success_at: Option<u64>,
    pub snapshot_refresh_healthy: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum MarketDataError {
    #[error("market data source unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait MarketDataSource: Send + Sync {
    async fn snapshots(&self) -> Result<Arc<Vec<RaindexMarketSnapshot>>, MarketDataError>;
    async fn market(&self, ticker_id: &str) -> Result<Option<MarketDetail>, MarketDataError>;
    async fn health(&self) -> Result<IndexerHealth, MarketDataError>;
}

#[derive(Debug, Clone)]
pub struct MarketDetail {
    pub snapshot: Arc<RaindexMarketSnapshot>,
    pub orderbook_observed_at: u64,
}

pub struct CachedMarketData {
    provider: Arc<RaindexProvider>,
    cache: MarketSnapshotCache,
}

impl CachedMarketData {
    pub fn new(provider: Arc<RaindexProvider>, cache: MarketSnapshotCache) -> Self {
        Self { provider, cache }
    }

    pub async fn warm(&self) -> Result<(), MarketDataError> {
        self.cache
            .refresh_overview(&self.provider)
            .await
            .map(|_| ())
            .map_err(|error| MarketDataError::Unavailable(error.to_string()))
    }

    pub async fn wait_for_local_index(&self, timeout: Duration) -> Result<(), MarketDataError> {
        let wait = async {
            loop {
                let snapshot = self
                    .provider
                    .client()
                    .get_local_db_sync_snapshot()
                    .await
                    .map_err(|error| MarketDataError::Unavailable(error.to_string()))?;
                match local_index_readiness(&snapshot) {
                    LocalIndexReadiness::Ready => return Ok(()),
                    LocalIndexReadiness::Failed(message) => {
                        return Err(MarketDataError::Unavailable(message));
                    }
                    LocalIndexReadiness::Waiting => {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        };
        tokio::time::timeout(timeout, wait).await.map_err(|_| {
            MarketDataError::Unavailable(format!(
                "local index did not become ready within {} seconds",
                timeout.as_secs()
            ))
        })?
    }

    pub fn start_background_refresh(self: Arc<Self>, interval: Duration) {
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            timer.tick().await;
            loop {
                timer.tick().await;
                match self.cache.refresh_overview(&self.provider).await {
                    Ok(snapshots) => {
                        tracing::info!(market_count = snapshots.len(), "market cache refreshed")
                    }
                    Err(error) => tracing::error!(
                        error = %error,
                        "market cache refresh failed; retaining the previous snapshot"
                    ),
                }
            }
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalIndexReadiness {
    Ready,
    Waiting,
    Failed(String),
}

fn local_index_readiness(snapshot: &LocalDbSyncSnapshot) -> LocalIndexReadiness {
    if !snapshot.configured {
        return LocalIndexReadiness::Ready;
    }
    if !snapshot.healthy {
        let errors = snapshot
            .networks
            .iter()
            .filter_map(|network| network.error.as_deref())
            .chain(
                snapshot
                    .raindexes
                    .iter()
                    .filter_map(|raindex| raindex.error.as_deref()),
            )
            .collect::<Vec<_>>()
            .join("; ");
        return LocalIndexReadiness::Failed(if errors.is_empty() {
            "local index synchronization failed".into()
        } else {
            format!("local index synchronization failed: {errors}")
        });
    }
    if !snapshot.networks.is_empty() && snapshot.networks.iter().all(|network| network.ready) {
        LocalIndexReadiness::Ready
    } else {
        LocalIndexReadiness::Waiting
    }
}

#[async_trait]
impl MarketDataSource for CachedMarketData {
    async fn snapshots(&self) -> Result<Arc<Vec<RaindexMarketSnapshot>>, MarketDataError> {
        self.cache
            .get_overview(&self.provider)
            .await
            .map_err(|error| MarketDataError::Unavailable(error.to_string()))
    }

    async fn market(&self, ticker_id: &str) -> Result<Option<MarketDetail>, MarketDataError> {
        let overview = self.snapshots().await?;
        let Some(overview_snapshot) = overview
            .iter()
            .find(|snapshot| snapshot.market.ticker_id.eq_ignore_ascii_case(ticker_id))
        else {
            return Ok(None);
        };
        let details = self
            .cache
            .get_market(&self.provider, ticker_id)
            .await
            .map_err(|error| MarketDataError::Unavailable(error.to_string()))?;
        Ok(details.first().map(|detail| MarketDetail {
            snapshot: Arc::new(merge_detail(overview_snapshot, detail)),
            orderbook_observed_at: detail.observed_at,
        }))
    }

    async fn health(&self) -> Result<IndexerHealth, MarketDataError> {
        let sync = self
            .provider
            .client()
            .get_local_db_sync_snapshot()
            .await
            .map_err(|error| MarketDataError::Unavailable(error.to_string()))?;
        let cache = self.cache.overview_health().await;
        Ok(IndexerHealth {
            configured: sync.configured,
            healthy: sync.healthy && cache.ready && cache.refresh_healthy,
            sync_healthy: sync.healthy,
            network_count: sync.networks.len(),
            orderbook_count: sync.raindexes.len(),
            snapshot_ready: cache.ready,
            snapshot_last_success_at: cache.last_success_at,
            snapshot_refresh_healthy: cache.refresh_healthy,
        })
    }
}

fn merge_detail(
    overview: &RaindexMarketSnapshot,
    detail: &RaindexMarketSnapshot,
) -> RaindexMarketSnapshot {
    let mut merged = overview.clone();
    merged.orderbook = detail.orderbook.clone();
    merged.block_number = detail.block_number.or(merged.block_number);
    merged.errors.extend(
        detail
            .errors
            .iter()
            .filter(|error| error.source == "orderbook")
            .cloned(),
    );
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sync_snapshot(healthy: bool, ready: bool, error: Option<&str>) -> LocalDbSyncSnapshot {
        serde_json::from_value(json!({
            "configured": true,
            "healthy": healthy,
            "status": if healthy && ready { "active" } else if healthy { "syncing" } else { "failure" },
            "schedulerState": "leader",
            "networks": [{
                "chainId": 8453,
                "networkKey": "base",
                "status": if healthy && ready { "active" } else if healthy { "syncing" } else { "failure" },
                "schedulerState": "leader",
                "raindexCount": 1,
                "ready": ready,
                "error": error
            }],
            "raindexes": []
        }))
        .expect("valid sync snapshot")
    }

    #[test]
    fn market_warmup_allows_an_unconfigured_local_index() {
        assert_eq!(
            local_index_readiness(&LocalDbSyncSnapshot::not_configured()),
            LocalIndexReadiness::Ready
        );
    }

    #[test]
    fn market_warmup_waits_until_every_configured_network_is_ready() {
        assert_eq!(
            local_index_readiness(&sync_snapshot(true, false, None)),
            LocalIndexReadiness::Waiting
        );
        assert_eq!(
            local_index_readiness(&sync_snapshot(true, true, None)),
            LocalIndexReadiness::Ready
        );
    }

    #[test]
    fn market_warmup_surfaces_local_index_failures() {
        assert_eq!(
            local_index_readiness(&sync_snapshot(false, false, Some("RPCs unavailable"))),
            LocalIndexReadiness::Failed(
                "local index synchronization failed: RPCs unavailable".into()
            )
        );
    }

    fn snapshot(
        last_price: &str,
        best_bid: Option<&str>,
        errors: serde_json::Value,
    ) -> RaindexMarketSnapshot {
        serde_json::from_value(json!({
            "market": {
                "id": "8453:base_quote",
                "tickerId": "0x0000000000000000000000000000000000000001_0x0000000000000000000000000000000000000002",
                "chainId": 8453,
                "base": {
                    "chainId": 8453,
                    "address": "0x0000000000000000000000000000000000000001",
                    "name": "Base",
                    "symbol": "BASE",
                    "decimals": 18,
                    "variants": []
                },
                "quote": {
                    "chainId": 8453,
                    "address": "0x0000000000000000000000000000000000000002",
                    "name": "Quote",
                    "symbol": "QUOTE",
                    "decimals": 6,
                    "variants": []
                },
                "raindexAddresses": []
            },
            "orderbook": {
                "bestBid": best_bid,
                "bids": [],
                "asks": []
            },
            "stats": {
                "lastPrice": last_price,
                "baseVolume24h": "10",
                "targetVolume24h": "20",
                "tradeCount24h": 1
            },
            "recentTrades": [],
            "observedAt": 100,
            "errors": errors
        }))
        .expect("valid snapshot fixture")
    }

    #[test]
    fn detail_merge_keeps_overview_stats_and_adds_only_orderbook_results() {
        let overview = snapshot(
            "2",
            None,
            json!([{"source": "ratios", "message": "unavailable"}]),
        );
        let detail = snapshot(
            "0",
            Some("1.9"),
            json!([
                {"source": "ratios", "message": "duplicate"},
                {"source": "orderbook", "message": "partial quote"}
            ]),
        );
        let merged = merge_detail(&overview, &detail);
        assert_eq!(merged.stats.last_price.as_deref(), Some("2"));
        assert_eq!(merged.orderbook.best_bid.as_deref(), Some("1.9"));
        assert_eq!(merged.observed_at, overview.observed_at);
        assert_eq!(merged.errors.len(), 2);
        assert_eq!(merged.errors[1].source, "orderbook");
    }

    #[test]
    fn overview_validation_rejects_ratio_errors() {
        let snapshots = vec![snapshot(
            "2",
            None,
            json!([{"source": "ratios", "message": "unavailable"}]),
        )];
        assert!(matches!(
            crate::cache::validate_overview(snapshots),
            Err(crate::cache::MarketCacheError::InvalidSnapshot(_))
        ));
    }

    #[test]
    fn overview_validation_rejects_orderbook_errors() {
        let snapshots = vec![snapshot(
            "2",
            None,
            json!([{"source": "orderbook", "message": "crossed book"}]),
        )];
        assert!(matches!(
            crate::cache::validate_overview(snapshots),
            Err(crate::cache::MarketCacheError::InvalidSnapshot(_))
        ));
    }

    #[test]
    fn overview_validation_rejects_trade_errors() {
        let snapshots = vec![snapshot(
            "0",
            None,
            json!([{"source": "trades", "message": "timed out"}]),
        )];
        assert!(matches!(
            crate::cache::validate_overview(snapshots),
            Err(crate::cache::MarketCacheError::InvalidSnapshot(_))
        ));
    }

    #[test]
    fn overview_validation_omits_failed_markets_and_keeps_healthy_markets() {
        let failed = snapshot(
            "2",
            None,
            json!([{"source": "ratios", "message": "timed out"}]),
        );
        let mut healthy = snapshot("3", None, json!([]));
        healthy.market.id = "8453:healthy_quote".into();
        healthy.market.ticker_id =
            "0x0000000000000000000000000000000000000003_0x0000000000000000000000000000000000000002"
                .into();

        let validated = crate::cache::validate_overview(vec![failed, healthy.clone()])
            .expect("healthy markets remain serviceable");

        assert_eq!(validated, vec![healthy]);
    }

    #[test]
    fn overview_validation_allows_trade_ordering_warnings() {
        let snapshots = vec![snapshot(
            "2",
            None,
            json!([{
                "source": "trades",
                "severity": "warning",
                "message": "same-block ordering is unavailable"
            }]),
        )];
        assert!(crate::cache::validate_overview(snapshots).is_ok());
    }

    #[test]
    fn overview_validation_rejects_duplicate_cross_chain_tickers() {
        let first = snapshot("2", None, json!([]));
        let mut second = first.clone();
        second.market.chain_id = 1;
        second.market.id = "1:base_quote".into();
        assert!(matches!(
            crate::cache::validate_overview(vec![first, second]),
            Err(crate::cache::MarketCacheError::InvalidSnapshot(_))
        ));
    }

    #[test]
    fn detail_validation_rejects_orderbook_errors() {
        let snapshots = vec![snapshot(
            "2",
            None,
            json!([{"source": "orderbook", "message": "timed out"}]),
        )];
        assert!(matches!(
            crate::cache::validate_detail(snapshots),
            Err(crate::cache::MarketCacheError::InvalidSnapshot(_))
        ));
    }

    #[test]
    fn detail_validation_rejects_ratio_normalization_errors() {
        let snapshots = vec![snapshot(
            "2",
            Some("1.9"),
            json!([{"source": "ratios", "message": "timed out"}]),
        )];
        assert!(matches!(
            crate::cache::validate_detail(snapshots),
            Err(crate::cache::MarketCacheError::InvalidSnapshot(_))
        ));
    }

    #[test]
    fn detail_validation_rejects_duplicate_cross_chain_tickers() {
        let first = snapshot("2", None, json!([]));
        let mut second = first.clone();
        second.market.chain_id = 1;
        second.market.id = "1:base_quote".into();

        assert!(matches!(
            crate::cache::validate_detail(vec![first, second]),
            Err(crate::cache::MarketCacheError::InvalidSnapshot(_))
        ));
    }

    #[test]
    fn detail_validation_keeps_levels_when_a_variant_ratio_is_unavailable() {
        let snapshots = vec![snapshot(
            "2",
            Some("1.9"),
            json!([{
                "source": "ratios",
                "severity": "warning",
                "message": "legacy ERC4626 ratio is unavailable"
            }]),
        )];

        let validated = crate::cache::validate_detail(snapshots).expect("partial orderbook");

        assert_eq!(validated[0].orderbook.best_bid.as_deref(), Some("1.9"));
        assert_eq!(
            validated[0].errors[0].severity,
            raindex_common::raindex_client::markets::RaindexMarketDataErrorSeverity::Warning
        );
    }
}
