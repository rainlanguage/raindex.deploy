use crate::{
    cache::MAX_ORDERBOOK_DEPTH, error::ApiError, fairings::PublicRateLimit, service::MarketDetail,
    AppState,
};
use raindex_common::raindex_client::markets::RaindexMarketSnapshot;
use rocket::{serde::json::Json, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

const DEFAULT_DEPTH: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct TickerResponse {
    pub ticker_id: String,
    pub base_currency: String,
    pub target_currency: String,
    pub pool_id: String,
    pub last_price: String,
    pub base_volume: String,
    pub target_volume: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<String>,
}

impl TickerResponse {
    fn from_snapshot(snapshot: &RaindexMarketSnapshot) -> Option<Self> {
        Some(Self {
            ticker_id: snapshot.market.ticker_id.clone(),
            base_currency: format!("{:#x}", snapshot.market.base.address),
            target_currency: format!("{:#x}", snapshot.market.quote.address),
            pool_id: snapshot.market.id.clone(),
            last_price: snapshot.stats.last_price.clone()?,
            base_volume: snapshot.stats.base_volume_24h.clone(),
            target_volume: snapshot.stats.target_volume_24h.clone(),
            bid: snapshot.orderbook.best_bid.clone(),
            ask: snapshot.orderbook.best_ask.clone(),
            high: snapshot.stats.high_24h.clone(),
            low: snapshot.stats.low_24h.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct OrderbookResponse {
    pub ticker_id: String,
    pub timestamp: u64,
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
}

async fn snapshots(state: &AppState) -> Result<Arc<Vec<RaindexMarketSnapshot>>, ApiError> {
    state.source.snapshots().await.map_err(|error| {
        tracing::error!(error = %error, "market snapshot refresh failed");
        ApiError::UpstreamUnavailable("market data is temporarily unavailable".into())
    })
}

async fn market_snapshot(state: &AppState, ticker_id: &str) -> Result<MarketDetail, ApiError> {
    state
        .source
        .market(ticker_id)
        .await
        .map_err(|error| {
            tracing::error!(ticker_id, error = %error, "market detail refresh failed");
            ApiError::UpstreamUnavailable("market data is temporarily unavailable".into())
        })?
        .ok_or_else(|| ApiError::NotFound(format!("unknown ticker_id: {ticker_id}")))
}

fn canonical_ticker_id(ticker_id: &str) -> String {
    ticker_id.to_ascii_lowercase()
}

fn validate_depth(depth: Option<usize>) -> Result<usize, ApiError> {
    let depth = depth.unwrap_or(DEFAULT_DEPTH);
    let max_depth = usize::from(MAX_ORDERBOOK_DEPTH);
    (depth <= max_depth)
        .then_some(depth)
        .ok_or_else(|| ApiError::BadRequest(format!("depth must be between 0 and {max_depth}")))
}

fn parse_depth(depth: Option<&str>) -> Result<usize, ApiError> {
    depth
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| ApiError::BadRequest("depth must be an unsigned integer".into()))
        })
        .transpose()
        .and_then(validate_depth)
}

#[utoipa::path(get, path = "/tickers", tag = "Markets", responses((status = 200, body = [TickerResponse])))]
#[get("/tickers")]
pub async fn tickers(
    _limit: PublicRateLimit,
    state: &State<AppState>,
) -> Result<Json<Vec<TickerResponse>>, ApiError> {
    Ok(Json(
        snapshots(state)
            .await?
            .iter()
            .filter_map(TickerResponse::from_snapshot)
            .collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/orderbook",
    tag = "Markets",
    params(("ticker_id" = String, Query), ("depth" = Option<usize>, Query)),
    responses((status = 200, body = OrderbookResponse))
)]
#[get("/orderbook?<ticker_id>&<depth>")]
pub async fn orderbook(
    ticker_id: &str,
    depth: Option<&str>,
    _limit: PublicRateLimit,
    state: &State<AppState>,
) -> Result<Json<OrderbookResponse>, ApiError> {
    let depth = parse_depth(depth)?;
    let ticker_id = canonical_ticker_id(ticker_id);
    let detail = market_snapshot(state, &ticker_id).await?;
    let snapshot = &detail.snapshot;
    let per_side = if depth == 0 {
        usize::MAX
    } else {
        depth.div_ceil(2)
    };
    Ok(Json(OrderbookResponse {
        ticker_id: snapshot.market.ticker_id.clone(),
        timestamp: detail.orderbook_observed_at.saturating_mul(1_000),
        bids: snapshot
            .orderbook
            .bids
            .iter()
            .take(per_side)
            .map(|level| [level.price.clone(), level.base_quantity.clone()])
            .collect(),
        asks: snapshot
            .orderbook
            .asks
            .iter()
            .take(per_side)
            .map(|level| [level.price.clone(), level.base_quantity.clone()])
            .collect(),
    }))
}

#[utoipa::path(get, path = "/v1/markets", tag = "Raindex", responses((status = 200, description = "Cached market overviews or one complete market snapshot")))]
#[get("/markets?<chain_id>&<ticker_id>")]
pub async fn markets(
    chain_id: Option<u32>,
    ticker_id: Option<&str>,
    _limit: PublicRateLimit,
    state: &State<AppState>,
) -> Result<Json<Vec<RaindexMarketSnapshot>>, ApiError> {
    let ticker_id = ticker_id.map(canonical_ticker_id);
    let include_recent_trades = ticker_id.is_some();
    let snapshots = match ticker_id.as_deref() {
        Some(ticker_id) => Arc::new(vec![market_snapshot(state, ticker_id)
            .await?
            .snapshot
            .as_ref()
            .clone()]),
        None => snapshots(state).await?,
    };
    Ok(Json(
        snapshots
            .iter()
            .filter(|snapshot| chain_id.is_none_or(|id| snapshot.market.chain_id == id))
            .filter(|snapshot| {
                ticker_id
                    .as_deref()
                    .is_none_or(|id| snapshot.market.ticker_id == id)
            })
            .map(|snapshot| {
                let mut snapshot = snapshot.clone();
                if !include_recent_trades {
                    snapshot.recent_trades.clear();
                }
                snapshot
            })
            .collect(),
    ))
}

pub fn compatibility_routes() -> Vec<rocket::Route> {
    routes![tickers, orderbook]
}

pub fn raindex_routes() -> Vec<rocket::Route> {
    routes![markets]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fairings::RateLimiter, service::MarketDataSource};
    use alloy::primitives::{Address, B256};
    use async_trait::async_trait;
    use raindex_common::raindex_client::markets::{
        RaindexMarket, RaindexMarketOrderbook, RaindexMarketOrderbookLevel, RaindexMarketStats,
        RaindexMarketToken, RaindexMarketTrade, RaindexMarketTradeSide,
    };
    use rocket::{
        http::{Header, Status},
        local::asynchronous::Client,
    };
    use std::collections::HashMap;

    struct MockMarketData {
        snapshots: Arc<Vec<RaindexMarketSnapshot>>,
        orderbook_observed_at: u64,
    }

    #[async_trait]
    impl MarketDataSource for MockMarketData {
        async fn snapshots(
            &self,
        ) -> Result<Arc<Vec<RaindexMarketSnapshot>>, crate::service::MarketDataError> {
            Ok(Arc::clone(&self.snapshots))
        }

        async fn market(
            &self,
            _ticker_id: &str,
        ) -> Result<Option<MarketDetail>, crate::service::MarketDataError> {
            Ok(self
                .snapshots
                .first()
                .cloned()
                .map(|snapshot| MarketDetail {
                    snapshot: Arc::new(snapshot),
                    orderbook_observed_at: self.orderbook_observed_at,
                }))
        }

        async fn health(
            &self,
        ) -> Result<crate::service::IndexerHealth, crate::service::MarketDataError> {
            Err(crate::service::MarketDataError::Unavailable(
                "unused".into(),
            ))
        }
    }

    fn token(address_byte: u8, symbol: &str) -> RaindexMarketToken {
        RaindexMarketToken {
            chain_id: 8453,
            address: Address::from([address_byte; 20]),
            name: symbol.to_string(),
            symbol: symbol.to_string(),
            decimals: Some(18),
            logo_uri: None,
            extensions: Some(HashMap::new()),
            unwrapped_address: None,
            legacy_address: None,
            receipt_address: None,
            variants: Vec::new(),
        }
    }

    fn level(price: &str, quantity: &str) -> RaindexMarketOrderbookLevel {
        RaindexMarketOrderbookLevel {
            price: price.into(),
            base_quantity: quantity.into(),
            target_quantity: "10".into(),
            chain_id: 8453,
            raindex: Address::ZERO,
            order_hash: B256::ZERO,
            source_token: Address::ZERO,
            block_number: 10,
        }
    }

    fn trade(side: RaindexMarketTradeSide, timestamp: u64) -> RaindexMarketTrade {
        RaindexMarketTrade {
            trade_id: format!("trade-{timestamp}"),
            price: "2".into(),
            base_volume: "3".into(),
            target_volume: "6".into(),
            timestamp,
            block_number: 10,
            trade_event_id: format!("event-{timestamp}"),
            trade_event_kind: "take-order".into(),
            side,
            chain_id: 8453,
            raindex: Address::ZERO,
            order_hash: B256::ZERO,
            source_token: Address::ZERO,
        }
    }

    fn snapshot() -> RaindexMarketSnapshot {
        let base = token(1, "BASE");
        let quote = token(2, "QUOTE");
        RaindexMarketSnapshot {
            market: RaindexMarket {
                id: "8453:base_quote".into(),
                ticker_id: format!("{:#x}_{:#x}", base.address, quote.address),
                chain_id: 8453,
                base,
                quote,
                raindex_addresses: vec![Address::ZERO],
            },
            orderbook: RaindexMarketOrderbook {
                best_bid: Some("1.9".into()),
                best_ask: Some("2.1".into()),
                midpoint: Some("2".into()),
                bids: vec![level("1.9", "5"), level("1.8", "4")],
                asks: vec![level("2.1", "6"), level("2.2", "7")],
            },
            stats: RaindexMarketStats {
                last_price: Some("2".into()),
                high_24h: Some("3".into()),
                low_24h: Some("1".into()),
                base_volume_24h: "30".into(),
                target_volume_24h: "60".into(),
                trade_count_24h: 2,
            },
            recent_trades: vec![
                trade(RaindexMarketTradeSide::Buy, 200),
                trade(RaindexMarketTradeSide::Sell, 100),
            ],
            observed_at: 300,
            block_number: Some(10),
            ratio_block_number: None,
            assets_per_share: None,
            errors: Vec::new(),
        }
    }

    async fn client_with_snapshots_and_rate_limit(
        snapshots: Vec<RaindexMarketSnapshot>,
        per_ip_rpm: u64,
    ) -> Client {
        let orderbook_observed_at = snapshots.first().map_or(0, |snapshot| snapshot.observed_at);
        client_with_snapshots_rate_limit_and_orderbook_time(
            snapshots,
            per_ip_rpm,
            orderbook_observed_at,
        )
        .await
    }

    async fn client_with_snapshots_rate_limit_and_orderbook_time(
        snapshots: Vec<RaindexMarketSnapshot>,
        per_ip_rpm: u64,
        orderbook_observed_at: u64,
    ) -> Client {
        let state = AppState {
            source: Arc::new(MockMarketData {
                snapshots: Arc::new(snapshots),
                orderbook_observed_at,
            }),
        };
        Client::tracked(
            rocket::custom(crate::rocket_figment(None))
                .manage(state)
                .manage(RateLimiter::new(10_000, per_ip_rpm))
                .mount("/", compatibility_routes())
                .mount("/v1", raindex_routes())
                .register("/", crate::catchers::catchers())
                .attach(crate::fairings::RequestLogger)
                .attach(crate::fairings::RateLimitHeaders),
        )
        .await
        .expect("test Rocket client")
    }

    async fn client_with_rate_limit(per_ip_rpm: u64) -> Client {
        client_with_snapshots_and_rate_limit(vec![snapshot()], per_ip_rpm).await
    }

    async fn client() -> Client {
        client_with_rate_limit(10_000).await
    }

    #[test]
    fn validates_depth_bounds() {
        assert_eq!(validate_depth(None).expect("default depth"), 100);
        assert_eq!(validate_depth(Some(0)).expect("full depth"), 0);
        assert!(validate_depth(Some(usize::from(MAX_ORDERBOOK_DEPTH) + 1)).is_err());
    }

    #[rocket::async_test]
    async fn ticker_route_maps_cached_snapshot() {
        let client = client().await;
        let response = client.get("/tickers").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        let body = response
            .into_json::<Vec<TickerResponse>>()
            .await
            .expect("json");
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].pool_id, "8453:base_quote");
        assert_eq!(body[0].last_price, "2");
        assert_eq!(body[0].base_volume, "30");
        assert_eq!(body[0].bid.as_deref(), Some("1.9"));
    }

    #[rocket::async_test]
    async fn ticker_route_excludes_markets_without_a_genuine_last_trade() {
        let mut never_traded = snapshot();
        never_traded.market.id = "8453:never_traded".into();
        never_traded.market.ticker_id = "never_traded_quote".into();
        never_traded.stats.last_price = None;
        let client =
            client_with_snapshots_and_rate_limit(vec![snapshot(), never_traded.clone()], 10_000)
                .await;

        let tickers = client
            .get("/tickers")
            .dispatch()
            .await
            .into_json::<Vec<TickerResponse>>()
            .await
            .expect("tickers json");
        assert_eq!(tickers.len(), 1);
        assert_eq!(tickers[0].last_price, "2");

        let markets = client
            .get("/v1/markets")
            .dispatch()
            .await
            .into_json::<Vec<RaindexMarketSnapshot>>()
            .await
            .expect("markets json");
        assert_eq!(markets.len(), 2);
        assert!(markets.iter().any(|snapshot| {
            snapshot.market.ticker_id == never_traded.market.ticker_id
                && snapshot.stats.last_price.is_none()
        }));
    }

    #[rocket::async_test]
    async fn empty_market_configuration_returns_successful_empty_collections() {
        let client = client_with_snapshots_and_rate_limit(vec![], 10_000).await;

        let tickers = client.get("/tickers").dispatch().await;
        assert_eq!(tickers.status(), Status::Ok);
        assert_eq!(
            tickers
                .into_json::<Vec<TickerResponse>>()
                .await
                .expect("tickers json"),
            vec![]
        );

        let markets = client.get("/v1/markets").dispatch().await;
        assert_eq!(markets.status(), Status::Ok);
        assert_eq!(
            markets
                .into_json::<Vec<RaindexMarketSnapshot>>()
                .await
                .expect("markets json"),
            vec![]
        );
    }

    #[rocket::async_test]
    async fn orderbook_route_applies_total_depth_across_both_sides() {
        let ticker_id = snapshot().market.ticker_id;
        let client = client().await;
        let response = client
            .get(format!("/orderbook?ticker_id={ticker_id}&depth=2"))
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Ok);
        let body = response
            .into_json::<OrderbookResponse>()
            .await
            .expect("json");
        assert_eq!(body.timestamp, 300_000);
        assert_eq!(body.bids, vec![[String::from("1.9"), String::from("5")]]);
        assert_eq!(body.asks, vec![[String::from("2.1"), String::from("6")]]);
    }

    #[rocket::async_test]
    async fn orderbook_route_uses_detail_observation_timestamp() {
        let snapshot = snapshot();
        let ticker_id = snapshot.market.ticker_id.clone();
        let client =
            client_with_snapshots_rate_limit_and_orderbook_time(vec![snapshot], 10_000, 450).await;

        let body = client
            .get(format!("/orderbook?ticker_id={ticker_id}"))
            .dispatch()
            .await
            .into_json::<OrderbookResponse>()
            .await
            .expect("json");

        assert_eq!(body.timestamp, 450_000);
    }

    #[rocket::async_test]
    async fn orderbook_route_rounds_odd_depth_up_per_side() {
        let ticker_id = snapshot().market.ticker_id;
        let client = client().await;
        let response = client
            .get(format!("/orderbook?ticker_id={ticker_id}&depth=3"))
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Ok);
        let body = response
            .into_json::<OrderbookResponse>()
            .await
            .expect("json");
        assert_eq!(body.bids.len(), 2);
        assert_eq!(body.asks.len(), 2);
    }

    #[rocket::async_test]
    async fn raindex_overview_omits_per_market_trade_payloads() {
        let client = client().await;
        let response = client.get("/v1/markets?chain_id=8453").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        let body = response
            .into_json::<Vec<RaindexMarketSnapshot>>()
            .await
            .expect("json");
        assert!(body[0].recent_trades.is_empty());
        assert_eq!(body[0].stats, snapshot().stats);
    }

    #[rocket::async_test]
    async fn raindex_market_detail_preserves_recent_trades() {
        let expected = snapshot();
        let client = client().await;
        let response = client
            .get(format!(
                "/v1/markets?ticker_id={}",
                expected.market.ticker_id
            ))
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Ok);
        let body = response
            .into_json::<Vec<RaindexMarketSnapshot>>()
            .await
            .expect("json");
        assert_eq!(body, vec![expected]);
    }

    #[rocket::async_test]
    async fn public_routes_return_structured_rate_limit_errors() {
        let client = client_with_rate_limit(1).await;
        assert_eq!(client.get("/tickers").dispatch().await.status(), Status::Ok);
        let response = client.get("/tickers").dispatch().await;
        assert_eq!(response.status(), Status::TooManyRequests);
        assert_eq!(response.headers().get_one("Retry-After"), Some("60"));
        assert_eq!(
            response.headers().get_one("X-RateLimit-Remaining"),
            Some("0")
        );
        let body = response
            .into_json::<crate::error::ApiErrorResponse>()
            .await
            .expect("structured error");
        assert_eq!(body.error.code, crate::error::ApiErrorCode::RateLimited);
    }

    #[rocket::async_test]
    async fn spoofed_ip_headers_do_not_bypass_public_rate_limit() {
        let client = client_with_rate_limit(1).await;
        let first = client
            .get("/tickers")
            .header(Header::new("X-Real-IP", "192.0.2.1"))
            .dispatch()
            .await;
        assert_eq!(first.status(), Status::Ok);
        let second = client
            .get("/tickers")
            .header(Header::new("X-Real-IP", "192.0.2.2"))
            .dispatch()
            .await;
        assert_eq!(second.status(), Status::TooManyRequests);
    }

    #[rocket::async_test]
    async fn unknown_routes_return_structured_errors_with_request_ids() {
        let client = client().await;
        let response = client.get("/missing").dispatch().await;
        assert_eq!(response.status(), Status::NotFound);
        let request_id = response
            .headers()
            .get_one("X-Request-Id")
            .expect("request ID")
            .to_string();
        assert!(uuid::Uuid::parse_str(&request_id).is_ok());
        let body = response
            .into_json::<crate::error::ApiErrorResponse>()
            .await
            .expect("structured error");
        assert_eq!(body.error.code, crate::error::ApiErrorCode::NotFound);
        assert_eq!(body.request_id, request_id);
    }

    #[rocket::async_test]
    async fn malformed_query_parameters_return_structured_bad_requests() {
        let ticker_id = snapshot().market.ticker_id;
        let client = client().await;
        let response = client
            .get(format!("/orderbook?ticker_id={ticker_id}&depth=invalid"))
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::BadRequest);
        let body = response
            .into_json::<crate::error::ApiErrorResponse>()
            .await
            .expect("structured error");
        assert_eq!(body.error.code, crate::error::ApiErrorCode::BadRequest);
    }
}
