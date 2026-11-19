use crate::provider::RaindexProvider;
use moka::future::Cache;
use raindex_common::raindex_client::markets::{
    MarketSnapshotOptions, RaindexMarketDataErrorSeverity, RaindexMarketSnapshot,
};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::RwLock;

pub const MAX_ORDERBOOK_DEPTH: u16 = 1_000;
const OVERVIEW_ORDERBOOK_DEPTH: u16 = 2;
const DETAIL_FAILURE_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
struct OverviewState {
    snapshots: Option<Arc<Vec<RaindexMarketSnapshot>>>,
    last_success_at: Option<u64>,
    refresh_healthy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewHealth {
    pub ready: bool,
    pub last_success_at: Option<u64>,
    pub refresh_healthy: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum MarketCacheError {
    #[error(transparent)]
    Sdk(#[from] raindex_common::raindex_client::RaindexError),
    #[error("invalid market snapshot: {0}")]
    InvalidSnapshot(String),
}

#[derive(Clone)]
pub struct MarketSnapshotCache {
    overview: Arc<RwLock<OverviewState>>,
    details: Cache<String, Arc<Vec<RaindexMarketSnapshot>>>,
    detail_failures: Cache<String, Arc<MarketCacheError>>,
    orderbook_depth: u16,
    recent_trades_limit: u16,
}

impl MarketSnapshotCache {
    pub fn new(ttl: Duration, recent_trades_limit: u16) -> Self {
        Self {
            overview: Arc::new(RwLock::new(OverviewState::default())),
            details: Cache::builder()
                .max_capacity(1_000)
                .time_to_live(ttl)
                .build(),
            detail_failures: Cache::builder()
                .max_capacity(1_000)
                .time_to_live(DETAIL_FAILURE_TTL)
                .build(),
            orderbook_depth: MAX_ORDERBOOK_DEPTH,
            recent_trades_limit,
        }
    }

    pub async fn get_overview(
        &self,
        provider: &RaindexProvider,
    ) -> Result<Arc<Vec<RaindexMarketSnapshot>>, MarketCacheError> {
        match self.overview.read().await.snapshots.clone() {
            Some(snapshots) => Ok(snapshots),
            None => self.refresh_overview(provider).await,
        }
    }

    pub async fn get_market(
        &self,
        provider: &RaindexProvider,
        ticker_id: &str,
    ) -> Result<Arc<Vec<RaindexMarketSnapshot>>, Arc<MarketCacheError>> {
        let key = ticker_id.to_ascii_lowercase();
        if let Some(error) = self.detail_failures.get(&key).await {
            return Err(error);
        }
        let options = self.market_options(&key);
        let result = self
            .details
            .try_get_with(key.clone(), async move {
                provider
                    .client()
                    .get_market_snapshots(Some(options))
                    .await
                    .map_err(MarketCacheError::Sdk)
                    .and_then(validate_detail)
                    .map(Arc::new)
            })
            .await;
        match result {
            Ok(snapshots) => {
                self.detail_failures.invalidate(&key).await;
                Ok(snapshots)
            }
            Err(error) => {
                self.detail_failures.insert(key, Arc::clone(&error)).await;
                Err(error)
            }
        }
    }

    pub async fn refresh_overview(
        &self,
        provider: &RaindexProvider,
    ) -> Result<Arc<Vec<RaindexMarketSnapshot>>, MarketCacheError> {
        let result = provider
            .client()
            .get_market_snapshots(Some(self.overview_options()))
            .await
            .map_err(MarketCacheError::Sdk)
            .and_then(validate_overview)
            .map(Arc::new);
        let mut state = self.overview.write().await;
        match result {
            Ok(snapshots) => {
                state.snapshots = Some(Arc::clone(&snapshots));
                state.last_success_at = Some(unix_now());
                state.refresh_healthy = true;
                Ok(snapshots)
            }
            Err(error) => {
                state.refresh_healthy = false;
                Err(error)
            }
        }
    }

    pub async fn overview_health(&self) -> OverviewHealth {
        let state = self.overview.read().await;
        OverviewHealth {
            ready: state.snapshots.is_some(),
            last_success_at: state.last_success_at,
            refresh_healthy: state.refresh_healthy,
        }
    }

    fn overview_options(&self) -> MarketSnapshotOptions {
        MarketSnapshotOptions {
            orderbook_depth: Some(OVERVIEW_ORDERBOOK_DEPTH),
            recent_trades_limit: Some(self.recent_trades_limit),
            include_orderbook: Some(true),
            ..Default::default()
        }
    }

    fn market_options(&self, ticker_id: &str) -> MarketSnapshotOptions {
        let ticker_id = ticker_id.to_ascii_lowercase();
        MarketSnapshotOptions {
            markets: raindex_common::raindex_client::markets::MarketListOptions {
                ticker_ids: Some(vec![ticker_id]),
                ..Default::default()
            },
            orderbook_depth: Some(self.orderbook_depth),
            recent_trades_limit: Some(0),
            include_trades: Some(false),
            ..Default::default()
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn validate_overview(
    mut snapshots: Vec<RaindexMarketSnapshot>,
) -> Result<Vec<RaindexMarketSnapshot>, MarketCacheError> {
    let snapshot_count = snapshots.len();
    let rejected_ticker_ids = snapshots
        .iter()
        .filter(|snapshot| has_critical_snapshot_error(snapshot))
        .map(|snapshot| snapshot.market.ticker_id.clone())
        .collect::<Vec<_>>();
    snapshots.retain(|snapshot| !has_critical_snapshot_error(snapshot));
    if snapshot_count > 0 && snapshots.is_empty() {
        return Err(MarketCacheError::InvalidSnapshot(
            "critical market data reads failed for every market".into(),
        ));
    }
    if !rejected_ticker_ids.is_empty() {
        tracing::warn!(
            rejected_market_count = rejected_ticker_ids.len(),
            rejected_ticker_ids = ?rejected_ticker_ids,
            "omitting markets with critical data read failures from the overview"
        );
    }
    validate_unique_ticker_ids(&snapshots)?;
    Ok(snapshots)
}

pub(crate) fn validate_detail(
    snapshots: Vec<RaindexMarketSnapshot>,
) -> Result<Vec<RaindexMarketSnapshot>, MarketCacheError> {
    if snapshots.iter().any(has_critical_snapshot_error) {
        return Err(MarketCacheError::InvalidSnapshot(
            "a critical market data read failed".into(),
        ));
    }
    validate_unique_ticker_ids(&snapshots)?;
    Ok(snapshots)
}

fn has_critical_snapshot_error(snapshot: &RaindexMarketSnapshot) -> bool {
    snapshot.errors.iter().any(|error| {
        error.severity == RaindexMarketDataErrorSeverity::Error
            && matches!(
                error.source.as_str(),
                "registry" | "trades" | "orderbook" | "ratios"
            )
    })
}

fn validate_unique_ticker_ids(snapshots: &[RaindexMarketSnapshot]) -> Result<(), MarketCacheError> {
    let mut ticker_ids = HashSet::new();
    if snapshots
        .iter()
        .map(|snapshot| snapshot.market.ticker_id.to_ascii_lowercase())
        .any(|ticker_id| !ticker_ids.insert(ticker_id))
    {
        return Err(MarketCacheError::InvalidSnapshot(
            "ticker IDs must be unique across configured chains".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_cache_uses_canonical_ticker_for_sdk_filter() {
        let cache = MarketSnapshotCache::new(Duration::from_secs(60), 100);
        assert_eq!(
            cache.market_options("0xAB_0xCD").markets.ticker_ids,
            Some(vec!["0xab_0xcd".to_string()])
        );
    }

    #[test]
    fn overview_reads_only_the_best_level_on_each_side() {
        let cache = MarketSnapshotCache::new(Duration::from_secs(60), 100);
        let options = cache.overview_options();
        assert_eq!(options.orderbook_depth, Some(2));
        assert_eq!(options.include_orderbook, Some(true));
    }

    #[test]
    fn overview_validation_accepts_empty_market_configuration() {
        assert_eq!(
            validate_overview(Vec::new()).expect("valid overview"),
            vec![]
        );
    }
}
