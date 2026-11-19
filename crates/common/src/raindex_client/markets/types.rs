use super::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen_utils::{impl_wasm_traits, prelude::Tsify};

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MarketListOptions {
    #[tsify(optional)]
    pub chain_ids: Option<Vec<u32>>,
    #[tsify(optional)]
    pub ticker_ids: Option<Vec<String>>,
}
impl_wasm_traits!(MarketListOptions);

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MarketSnapshotOptions {
    #[serde(flatten)]
    pub markets: MarketListOptions,
    #[tsify(optional)]
    pub orderbook_depth: Option<u16>,
    #[tsify(optional)]
    pub recent_trades_limit: Option<u16>,
    #[tsify(optional)]
    pub include_orderbook: Option<bool>,
    #[tsify(optional)]
    pub include_ratios: Option<bool>,
    #[tsify(optional)]
    pub include_trades: Option<bool>,
}
impl_wasm_traits!(MarketSnapshotOptions);

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RaindexMarketToken {
    pub chain_id: u32,
    #[tsify(type = "Address")]
    pub address: Address,
    pub name: String,
    pub symbol: String,
    pub decimals: Option<u8>,
    #[tsify(optional)]
    pub logo_uri: Option<String>,
    #[tsify(optional, type = "Map<string, any>")]
    pub extensions: Option<HashMap<String, Value>>,
    #[tsify(optional, type = "Address")]
    pub unwrapped_address: Option<Address>,
    #[tsify(optional, type = "Address")]
    pub legacy_address: Option<Address>,
    #[tsify(optional, type = "Address")]
    pub receipt_address: Option<Address>,
    #[tsify(type = "Address[]")]
    pub variants: Vec<Address>,
}
impl_wasm_traits!(RaindexMarketToken);

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RaindexMarket {
    pub id: String,
    pub ticker_id: String,
    pub chain_id: u32,
    pub base: RaindexMarketToken,
    pub quote: RaindexMarketToken,
    #[tsify(type = "Address[]")]
    pub raindex_addresses: Vec<Address>,
}
impl_wasm_traits!(RaindexMarket);

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RaindexMarketDataError {
    pub source: String,
    #[serde(default)]
    pub severity: RaindexMarketDataErrorSeverity,
    pub message: String,
}
impl_wasm_traits!(RaindexMarketDataError);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Tsify, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RaindexMarketDataErrorSeverity {
    Warning,
    #[default]
    Error,
}
impl_wasm_traits!(RaindexMarketDataErrorSeverity);

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RaindexMarketOrderbookLevel {
    pub price: String,
    pub base_quantity: String,
    pub target_quantity: String,
    pub chain_id: u32,
    #[tsify(type = "Address")]
    pub raindex: Address,
    #[tsify(type = "Hex")]
    pub order_hash: B256,
    #[tsify(type = "Address")]
    pub source_token: Address,
    pub block_number: u64,
}
impl_wasm_traits!(RaindexMarketOrderbookLevel);

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RaindexMarketOrderbook {
    #[tsify(optional)]
    pub best_bid: Option<String>,
    #[tsify(optional)]
    pub best_ask: Option<String>,
    #[tsify(optional)]
    pub midpoint: Option<String>,
    pub bids: Vec<RaindexMarketOrderbookLevel>,
    pub asks: Vec<RaindexMarketOrderbookLevel>,
}
impl_wasm_traits!(RaindexMarketOrderbook);

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RaindexMarketTradeSide {
    Buy,
    Sell,
}
impl_wasm_traits!(RaindexMarketTradeSide);

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RaindexMarketTrade {
    #[tsify(type = "Hex")]
    pub trade_id: String,
    pub price: String,
    pub base_volume: String,
    pub target_volume: String,
    pub timestamp: u64,
    pub block_number: u64,
    #[tsify(type = "Hex")]
    pub trade_event_id: String,
    pub trade_event_kind: String,
    pub side: RaindexMarketTradeSide,
    pub chain_id: u32,
    #[tsify(type = "Address")]
    pub raindex: Address,
    #[tsify(type = "Hex")]
    pub order_hash: B256,
    #[tsify(type = "Address")]
    pub source_token: Address,
}
impl_wasm_traits!(RaindexMarketTrade);

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RaindexMarketStats {
    #[tsify(optional)]
    pub last_price: Option<String>,
    #[tsify(optional)]
    pub high_24h: Option<String>,
    #[tsify(optional)]
    pub low_24h: Option<String>,
    pub base_volume_24h: String,
    pub target_volume_24h: String,
    pub trade_count_24h: u64,
}
impl Default for RaindexMarketStats {
    fn default() -> Self {
        Self {
            last_price: None,
            high_24h: None,
            low_24h: None,
            base_volume_24h: "0".to_string(),
            target_volume_24h: "0".to_string(),
            trade_count_24h: 0,
        }
    }
}
impl_wasm_traits!(RaindexMarketStats);

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RaindexMarketSnapshot {
    pub market: RaindexMarket,
    pub orderbook: RaindexMarketOrderbook,
    pub stats: RaindexMarketStats,
    pub recent_trades: Vec<RaindexMarketTrade>,
    pub observed_at: u64,
    #[tsify(optional)]
    pub block_number: Option<u64>,
    #[tsify(optional)]
    pub ratio_block_number: Option<u64>,
    #[tsify(optional)]
    pub assets_per_share: Option<String>,
    pub errors: Vec<RaindexMarketDataError>,
}
impl_wasm_traits!(RaindexMarketSnapshot);
