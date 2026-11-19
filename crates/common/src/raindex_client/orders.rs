use super::local_db::orders::LocalDbOrders;
use super::local_db::query::fetch_order_vaults_volume::fetch_order_vaults_volume;
use super::trades::RaindexTrade;
use super::ClientRef;
use super::QuerySource;
use super::*;
use crate::local_db::query::fetch_orders::LocalDbOrder;
use crate::local_db::query::fetch_vaults::LocalDbVault;
use crate::local_db::RaindexIdentifier;
use crate::parsed_meta::ParsedMeta;
use crate::raindex_client::order_quotes::RaindexOrderQuote;
use crate::raindex_client::take_orders::single::{RpcContext, TakeOrderExecutionParams};
use crate::raindex_client::take_orders::{
    build_candidate_from_quote, estimate_take_order, execute_single_take, TakeOrderEstimate,
    TakeOrdersCalldataResult,
};
use crate::raindex_client::vaults_list::RaindexVaultsList;
use crate::rpc_client::RpcClient;
use crate::take_orders::{ParsedTakeOrdersMode, TakeOrdersMode};
use crate::utils::timing::Timing;
use crate::{
    meta::TryDecodeRainlangSource,
    raindex_client::{
        transactions::RaindexTransaction,
        vaults::{RaindexVault, RaindexVaultType, RaindexVaultVolume},
    },
};
use alloy::primitives::{b256, keccak256, Address, Bytes, B256, U256};
use async_trait::async_trait;
use csv::{ReaderBuilder, Terminator};
use futures::{stream, StreamExt, TryStreamExt};
use rain_math_float::Float;
use rain_metaboard_subgraph::metaboard_client::MetaboardSubgraphClient;
use rain_metaboard_subgraph::types::metas::Bytes as MetaBytes;
use rain_metadata::types::dotrain::source_v1::DotrainSourceV1;
use rain_metadata::{KnownMagic, RainMetaDocumentV1Item};
use raindex_subgraph_client::{
    types::{
        common::{
            SgBigInt, SgBytes, SgOrder, SgOrderAsIO, SgOrdersListFilterArgs,
            SgOrdersTokensFilterArgs, SgRaindex, SgVault,
        },
        Id,
    },
    MultiRaindexSubgraphClient, RaindexSubgraphClient, RaindexSubgraphClientError,
    SgPaginationArgs,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io::Cursor, str::FromStr};
use tracing::{debug, error, info, info_span, warn, Instrument, Level};
use tsify::Tsify;
use wasm_bindgen_utils::impl_wasm_traits;
#[cfg(target_family = "wasm")]
use wasm_bindgen_utils::prelude::js_sys::BigInt;

const DEFAULT_PAGE_SIZE: u16 = 100;
// Limit concurrent dotrain source fetches to avoid overwhelming the subgraph/metaboard.
const MAX_CONCURRENT_DOTRAIN_SOURCE_FETCHES: usize = 5;
const MAX_INFO_ORDER_INVENTORY_LOGS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
struct OrderFilterTraceSummary {
    owners_count: usize,
    has_active_filter: bool,
    has_order_hash_filter: bool,
    input_tokens_count: usize,
    output_tokens_count: usize,
    raindexes_count: usize,
    has_positive_output_vault_balance_filter: bool,
}

fn summarize_order_filters(filters: &GetOrdersFilters) -> OrderFilterTraceSummary {
    OrderFilterTraceSummary {
        owners_count: filters.owners.len(),
        has_active_filter: filters.active.is_some(),
        has_order_hash_filter: filters.order_hash.is_some(),
        input_tokens_count: filters
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.inputs.as_ref())
            .map_or(0, Vec::len),
        output_tokens_count: filters
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.outputs.as_ref())
            .map_or(0, Vec::len),
        raindexes_count: filters.raindex_addresses.as_ref().map_or(0, Vec::len),
        has_positive_output_vault_balance_filter: filters
            .has_positive_output_vault_balance
            .is_some(),
    }
}

fn query_source_label(has_local_db: bool, has_subgraph: bool) -> &'static str {
    match (has_local_db, has_subgraph) {
        (true, true) => "mixed",
        (true, false) => "local_db",
        (false, true) => "subgraph",
        (false, false) => "none",
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct RaindexOrdersListResult {
    pub(crate) orders: Vec<RaindexOrder>,
    pub(crate) total_count: u32,
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl RaindexOrdersListResult {
    #[wasm_bindgen(getter)]
    pub fn orders(&self) -> Vec<RaindexOrder> {
        self.orders.clone()
    }

    #[wasm_bindgen(getter, js_name = "totalCount")]
    pub fn total_count(&self) -> u32 {
        self.total_count
    }
}

#[cfg(not(target_family = "wasm"))]
impl RaindexOrdersListResult {
    pub fn orders(&self) -> &[RaindexOrder] {
        &self.orders
    }

    pub fn total_count(&self) -> u32 {
        self.total_count
    }
}

pub(crate) struct SubgraphOrders<'a> {
    client: &'a RaindexClient,
}
impl<'a> SubgraphOrders<'a> {
    pub(crate) fn new(client: &'a RaindexClient) -> Self {
        Self { client }
    }
}

#[cfg_attr(target_family = "wasm", async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
pub(crate) trait OrdersDataSource {
    async fn list(
        &self,
        chain_ids: Option<Vec<u32>>,
        filters: &GetOrdersFilters,
        page: Option<u16>,
        page_size: Option<u16>,
    ) -> Result<RaindexOrdersListResult, RaindexError>;

    async fn get_by_hash(
        &self,
        raindex_id: &RaindexIdentifier,
        order_hash: &B256,
    ) -> Result<Option<RaindexOrder>, RaindexError>;

    async fn get_added_by_tx_hash(
        &self,
        chain_id: u32,
        raindex: Address,
        tx_hash: B256,
    ) -> Result<Vec<RaindexOrder>, RaindexError>;

    async fn get_removed_by_tx_hash(
        &self,
        chain_id: u32,
        raindex: Address,
        tx_hash: B256,
    ) -> Result<Vec<RaindexOrder>, RaindexError>;

    async fn trades_list(
        &self,
        raindex_id: &RaindexIdentifier,
        order_hash: &B256,
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
        page: Option<u16>,
    ) -> Result<Vec<RaindexTrade>, RaindexError>;

    async fn trades_count(
        &self,
        raindex_id: &RaindexIdentifier,
        order_hash: &B256,
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
    ) -> Result<u64, RaindexError>;
}

/// A single order representation within a given raindex.
///
/// RaindexOrder represents a trading order on a specific blockchain with its associated
/// input and output vaults, metadata, and performance tracking capabilities. Each order
/// is deployed on a specific raindex contract and can be queried for volume and
/// performance metrics over time.
///
/// The order contains both the raw order data (bytes and hash) and structured access
/// to its vaults, which define what tokens can be traded and their current balances.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct RaindexOrder {
    raindex_client: ClientRef,
    chain_id: u32,
    id: B256,
    order_bytes: Bytes,
    order_hash: B256,
    owner: Address,
    inputs: Vec<RaindexVault>,
    outputs: Vec<RaindexVault>,
    raindex: Address,
    active: bool,
    timestamp_added: U256,
    timestamp_removed: Option<U256>,
    meta: Option<Bytes>,
    parsed_meta: Vec<ParsedMeta>,
    rainlang: Option<String>,
    transaction: Option<RaindexTransaction>,
    trades_count: u16,
}

fn get_io_by_type(order: &RaindexOrder, vault_type: RaindexVaultType) -> Vec<RaindexVault> {
    let vaults = order.vaults_list().items();
    vaults
        .into_iter()
        .filter(|v| v.vault_type() == Some(vault_type.clone()))
        .collect()
}

fn format_order_vaults(vaults: &[RaindexVault]) -> Vec<String> {
    vaults
        .iter()
        .map(|vault| {
            let vault_id = vault.vault_id_string();

            format!(
                "vault_id={},token={},balance={}",
                vault_id,
                vault.token().address(),
                vault.formatted_balance()
            )
        })
        .collect()
}

fn log_order_inventory_for_pair(
    chain_id: u32,
    sell_token: Address,
    buy_token: Address,
    orders: &[RaindexOrder],
) {
    if !tracing::enabled!(Level::DEBUG) {
        return;
    }

    for (order_index, order) in orders
        .iter()
        .take(MAX_INFO_ORDER_INVENTORY_LOGS)
        .enumerate()
    {
        debug!(
            chain_id,
            sell_token = %sell_token,
            buy_token = %buy_token,
            order_index,
            order_hash = %order.order_hash(),
            raindex = %order.raindex(),
            input_vaults = ?format_order_vaults(&order.inputs),
            output_vaults = ?format_order_vaults(&order.outputs),
            "order considered for take-orders pair"
        );
    }

    let omitted = orders.len().saturating_sub(MAX_INFO_ORDER_INVENTORY_LOGS);
    if omitted > 0 {
        debug!(
            chain_id,
            sell_token = %sell_token,
            buy_token = %buy_token,
            logged_order_count = MAX_INFO_ORDER_INVENTORY_LOGS,
            omitted_order_count = omitted,
            "omitted additional order inventory logs"
        );
    }
}

impl RaindexOrder {
    pub(crate) fn from_local_db_order(
        raindex_client: ClientRef,
        order: LocalDbOrder,
        inputs: Vec<LocalDbVault>,
        outputs: Vec<LocalDbVault>,
    ) -> Result<Self, RaindexError> {
        let chain_id = order.chain_id;
        let rainlang = order
            .meta
            .as_ref()
            .and_then(|meta| meta.to_string().try_decode_rainlangsource().ok());

        let mut id = Vec::from(order.raindex_address.as_slice());
        id.extend_from_slice(order.order_hash.as_ref());

        Ok(Self {
            raindex_client: ClientRef::clone(&raindex_client),
            chain_id,
            id: keccak256(&id),
            order_bytes: order.order_bytes,
            order_hash: order.order_hash,
            owner: order.owner,
            inputs: inputs
                .into_iter()
                .map(|v| {
                    RaindexVault::try_from_local_db(
                        ClientRef::clone(&raindex_client),
                        v,
                        Some(RaindexVaultType::Input),
                    )
                })
                .collect::<Result<Vec<RaindexVault>, RaindexError>>()?,
            outputs: outputs
                .into_iter()
                .map(|v| {
                    RaindexVault::try_from_local_db(
                        ClientRef::clone(&raindex_client),
                        v,
                        Some(RaindexVaultType::Output),
                    )
                })
                .collect::<Result<Vec<RaindexVault>, RaindexError>>()?,
            raindex: order.raindex_address,
            active: order.active,
            timestamp_added: U256::from(order.block_timestamp),
            timestamp_removed: order.block_timestamp_removed.map(U256::from),
            meta: order.meta.clone(),
            parsed_meta: order
                .meta
                .as_ref()
                .and_then(|meta| ParsedMeta::parse_from_bytes(meta).ok())
                .unwrap_or_default(),
            rainlang,
            transaction: Some(RaindexTransaction::from_local_parts(
                order.transaction_hash,
                order.owner,
                order.block_number,
                order.block_timestamp,
            )?),
            trades_count: order.trade_count as u16,
        })
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl RaindexOrder {
    #[wasm_bindgen(getter = chainId)]
    pub fn chain_id(&self) -> u32 {
        self.chain_id
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Hex")]
    pub fn id(&self) -> String {
        self.id.to_string()
    }
    #[wasm_bindgen(getter = orderBytes, unchecked_return_type = "Hex")]
    pub fn order_bytes(&self) -> String {
        self.order_bytes.to_string()
    }
    #[wasm_bindgen(getter = orderHash, unchecked_return_type = "Hex")]
    pub fn order_hash(&self) -> String {
        self.order_hash.to_string()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Address")]
    pub fn owner(&self) -> String {
        self.owner.to_string()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Address")]
    pub fn raindex(&self) -> String {
        self.raindex.to_string()
    }
    #[wasm_bindgen(getter)]
    pub fn active(&self) -> bool {
        self.active
    }
    #[wasm_bindgen(getter = timestampAdded)]
    pub fn timestamp_added(&self) -> Result<BigInt, RaindexError> {
        BigInt::from_str(&self.timestamp_added.to_string())
            .map_err(|e| RaindexError::JsError(e.to_string().into()))
    }
    #[wasm_bindgen(getter = timestampRemoved)]
    pub fn timestamp_removed(&self) -> Result<Option<BigInt>, RaindexError> {
        self.timestamp_removed
            .map(|timestamp| {
                BigInt::from_str(&timestamp.to_string())
                    .map_err(|e| RaindexError::JsError(e.to_string().into()))
            })
            .transpose()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Hex | undefined")]
    pub fn meta(&self) -> Option<String> {
        self.meta.clone().map(|meta| meta.to_string())
    }
    #[wasm_bindgen(getter = parsedMeta)]
    pub fn parsed_meta(&self) -> Vec<ParsedMeta> {
        self.parsed_meta.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn rainlang(&self) -> Option<String> {
        self.rainlang.clone()
    }
    #[wasm_bindgen(getter = dotrainSource)]
    pub fn dotrain_source(&self) -> Option<String> {
        self.parsed_meta().into_iter().find_map(|meta| match meta {
            ParsedMeta::DotrainSourceV1(source) => Some(source.0),
            _ => None,
        })
    }
    #[wasm_bindgen(getter = orderBuilderState)]
    pub fn order_builder_state(&self) -> Option<String> {
        self.parsed_meta().into_iter().find_map(|meta| match meta {
            ParsedMeta::OrderBuilderStateV1(state) => serde_json::to_string(&state).ok(),
            _ => None,
        })
    }
    /// Returns the signed context oracle URL if this order has oracle metadata.
    #[wasm_bindgen(getter = oracleUrl)]
    pub fn oracle_url(&self) -> Option<String> {
        self.parsed_meta().into_iter().find_map(|meta| match meta {
            ParsedMeta::RaindexSignedContextOracleV1(oracle) => Some(oracle.0),
            _ => None,
        })
    }
    #[wasm_bindgen(getter)]
    pub fn transaction(&self) -> Option<RaindexTransaction> {
        self.transaction.clone()
    }
    #[wasm_bindgen(getter = tradesCount)]
    pub fn trades_count(&self) -> u16 {
        self.trades_count
    }

    #[wasm_bindgen(getter = vaultsList)]
    pub fn vaults_list(&self) -> RaindexVaultsList {
        RaindexVaultsList::new(get_vaults_with_type(
            self.inputs.clone(),
            self.outputs.clone(),
        ))
    }
    #[wasm_bindgen(getter = inputsList)]
    pub fn inputs_list(&self) -> RaindexVaultsList {
        RaindexVaultsList::new(get_io_by_type(self, RaindexVaultType::Input))
    }
    #[wasm_bindgen(getter = outputsList)]
    pub fn outputs_list(&self) -> RaindexVaultsList {
        RaindexVaultsList::new(get_io_by_type(self, RaindexVaultType::Output))
    }
    #[wasm_bindgen(getter = inputsOutputsList)]
    pub fn inputs_outputs_list(&self) -> RaindexVaultsList {
        RaindexVaultsList::new(get_io_by_type(self, RaindexVaultType::InputOutput))
    }
}
impl RaindexOrder {
    pub(crate) fn input_vaults(&self) -> &[RaindexVault] {
        &self.inputs
    }

    /// The order's output vaults, in the same order as the subgraph returned
    /// them. Unlike [`Self::outputs_list`] this is not filtered by vault type,
    /// so vaults that act as both input and output are still included.
    pub(crate) fn output_vaults(&self) -> &[RaindexVault] {
        &self.outputs
    }

    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn raw_order_bytes(&self) -> &Bytes {
        &self.order_bytes
    }

    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn raw_order_hash(&self) -> B256 {
        self.order_hash
    }

    pub(crate) fn raw_raindex(&self) -> Address {
        self.raindex
    }
}
#[cfg(not(target_family = "wasm"))]
impl RaindexOrder {
    pub fn chain_id(&self) -> u32 {
        self.chain_id
    }
    pub fn id(&self) -> B256 {
        self.id
    }
    pub fn order_bytes(&self) -> Bytes {
        self.order_bytes.clone()
    }
    pub fn order_hash(&self) -> B256 {
        self.order_hash
    }
    pub fn owner(&self) -> Address {
        self.owner
    }
    pub fn raindex(&self) -> Address {
        self.raindex
    }
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn timestamp_added(&self) -> U256 {
        self.timestamp_added
    }
    pub fn timestamp_removed(&self) -> Option<U256> {
        self.timestamp_removed
    }
    pub fn meta(&self) -> Option<Bytes> {
        self.meta.clone()
    }
    pub fn parsed_meta(&self) -> Vec<ParsedMeta> {
        self.parsed_meta.clone()
    }
    pub fn rainlang(&self) -> Option<String> {
        self.rainlang.clone()
    }
    pub fn dotrain_source(&self) -> Option<String> {
        self.parsed_meta().into_iter().find_map(|meta| match meta {
            ParsedMeta::DotrainSourceV1(source) => Some(source.0),
            _ => None,
        })
    }
    pub fn order_builder_state(&self) -> Option<String> {
        self.parsed_meta().into_iter().find_map(|meta| match meta {
            ParsedMeta::OrderBuilderStateV1(state) => serde_json::to_string(&state).ok(),
            _ => None,
        })
    }
    /// Returns the signed context oracle URL if this order has oracle metadata.
    pub fn oracle_url(&self) -> Option<String> {
        self.parsed_meta().into_iter().find_map(|meta| match meta {
            ParsedMeta::RaindexSignedContextOracleV1(oracle) => Some(oracle.0),
            _ => None,
        })
    }
    pub fn transaction(&self) -> Option<RaindexTransaction> {
        self.transaction.clone()
    }
    pub fn trades_count(&self) -> u16 {
        self.trades_count
    }
    pub fn vaults_list(&self) -> RaindexVaultsList {
        RaindexVaultsList::new(get_vaults_with_type(
            self.inputs.clone(),
            self.outputs.clone(),
        ))
    }
    pub fn inputs_list(&self) -> RaindexVaultsList {
        RaindexVaultsList::new(get_io_by_type(self, RaindexVaultType::Input))
    }
    pub fn outputs_list(&self) -> RaindexVaultsList {
        RaindexVaultsList::new(get_io_by_type(self, RaindexVaultType::Output))
    }
    pub fn inputs_outputs_list(&self) -> RaindexVaultsList {
        RaindexVaultsList::new(get_io_by_type(self, RaindexVaultType::InputOutput))
    }
}

fn get_vaults_with_type(
    inputs: Vec<RaindexVault>,
    outputs: Vec<RaindexVault>,
) -> Vec<RaindexVault> {
    let mut vaults: Vec<RaindexVault> = Vec::new();

    let input_ids: HashSet<String> = inputs.iter().map(|v| v.id().to_string()).collect();
    let output_ids: HashSet<String> = outputs.iter().map(|v| v.id().to_string()).collect();

    // First add inputs (excluding input_outputs)
    for vault in &inputs {
        if !output_ids.contains(&vault.id().to_string()) {
            vaults.push(vault.clone());
        }
    }
    // Then add outputs (excluding input_outputs)
    for vault in &outputs {
        if !input_ids.contains(&vault.id().to_string()) {
            vaults.push(vault.clone());
        }
    }
    // Finally add input_outputs (only once for vaults that are both input and output)
    for vault in &inputs {
        if output_ids.contains(&vault.id().to_string()) {
            let input_output_vault = vault.with_vault_type(RaindexVaultType::InputOutput);
            vaults.push(input_output_vault);
        }
    }
    vaults
}

#[wasm_export]
impl RaindexOrder {
    #[wasm_export(skip)]
    pub fn get_raindex_client(&self) -> ClientRef {
        ClientRef::clone(&self.raindex_client)
    }
    #[wasm_export(skip)]
    pub fn get_raindex_subgraph_client(&self) -> Result<RaindexSubgraphClient, RaindexError> {
        self.raindex_client
            .get_raindex_subgraph_client(self.raindex)
    }
    #[wasm_export(skip)]
    pub fn get_rpc_urls(&self) -> Result<Vec<Url>, RaindexError> {
        self.raindex_client.get_rpc_urls_for_chain(self.chain_id)
    }
    #[wasm_export(skip)]
    pub fn get_metaboard_client(&self) -> Result<Option<MetaboardSubgraphClient>, RaindexError> {
        let raindex_client = self.get_raindex_client();
        let network = raindex_client
            .raindex_yaml
            .get_network_by_chain_id(self.chain_id)?;
        let metaboard = match raindex_client.raindex_yaml.get_metaboard(&network.key) {
            Ok(metaboard) => metaboard,
            Err(YamlError::KeyNotFound(_) | YamlError::NotFound(_)) => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        Ok(Some(MetaboardSubgraphClient::new(metaboard.url.clone())))
    }

    #[wasm_export(
        js_name = "getVaultsVolume",
        return_description = "Volume data for each vault over the specified period",
        unchecked_return_type = "RaindexVaultVolume[]",
        preserve_js_class
    )]
    pub async fn get_vaults_volume(
        &self,
        #[wasm_export(
            js_name = "startTimestamp",
            param_description = "Unix timestamp for the start of the query period (optional)"
        )]
        start_timestamp: Option<u64>,
        #[wasm_export(
            js_name = "endTimestamp",
            param_description = "Unix timestamp for the end of the query period (optional)"
        )]
        end_timestamp: Option<u64>,
    ) -> Result<Vec<RaindexVaultVolume>, RaindexError> {
        let chain_id = self.chain_id();
        #[cfg(target_family = "wasm")]
        let raindex_addr = Address::from_str(&self.raindex())?;
        #[cfg(not(target_family = "wasm"))]
        let raindex_addr = self.raindex();

        let raindex_client = self.get_raindex_client();
        match raindex_client.query_source(chain_id) {
            QuerySource::LocalDb(local_db) => {
                #[cfg(target_family = "wasm")]
                let order_hash = B256::from_str(&self.order_hash())?;
                #[cfg(not(target_family = "wasm"))]
                let order_hash = B256::from_str(&self.order_hash().to_string())?;

                let volumes = fetch_order_vaults_volume(
                    &local_db,
                    &RaindexIdentifier::new(chain_id, raindex_addr),
                    order_hash,
                    start_timestamp,
                    end_timestamp,
                )
                .await?;

                volumes
                    .into_iter()
                    .map(|v| RaindexVaultVolume::try_from_local_db_vault_volume(chain_id, v))
                    .collect()
            }
            QuerySource::Subgraph => {
                let client = self.get_raindex_subgraph_client()?;
                let volumes = client
                    .order_vaults_volume(
                        Id::new(self.id.to_string()),
                        start_timestamp,
                        end_timestamp,
                    )
                    .await?;
                volumes
                    .into_iter()
                    .map(|v| RaindexVaultVolume::try_from_vault_volume(self.chain_id, v))
                    .collect()
            }
        }
    }

    // /// Gets comprehensive performance metrics and analytics for this order over a specified time period
    // ///
    // /// Retrieves detailed performance data including profit/loss, volume statistics, and other
    // /// key metrics that help assess the effectiveness of the trading algorithm implemented by this order.
    // ///
    // /// ## Examples
    // ///
    // /// ```javascript
    // /// const result = await order.getPerformance(
    // ///   Math.floor(Date.now() / 1000) - 604800, // 1 week ago
    // ///   Math.floor(Date.now() / 1000)
    // /// );
    // /// if (result.error) {
    // ///   console.error("Error fetching performance:", result.error.readableMsg);
    // ///   return;
    // /// }
    // /// const performance = result.value;
    // /// // Do something with performance
    // /// ```
    // #[wasm_export(
    //     js_name = "getPerformance",
    //     return_description = "Comprehensive performance metrics for the order",
    //     unchecked_return_type = "OrderPerformance"
    // )]
    // pub async fn get_performance(
    //     &self,
    //     #[wasm_export(
    //         js_name = "startTimestamp",
    //         param_description = "Unix timestamp for the start of the analysis period (optional, defaults to order creation)"
    //     )]
    //     start_timestamp: Option<u64>,
    //     #[wasm_export(
    //         js_name = "endTimestamp",
    //         param_description = "Unix timestamp for the end of the analysis period (optional, defaults to current time)"
    //     )]
    //     end_timestamp: Option<u64>,
    // ) -> Result<OrderPerformance, RaindexError> {
    //     let client = self.get_raindex_subgraph_client()?;
    //     let performance = client
    //         .order_performance(Id::new(self.id.to_string()), start_timestamp, end_timestamp)
    //         .await?;
    //     Ok(performance)
    // }

    /// Converts the order from RaindexOrder to an SgOrder type
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const sgOrder = await order.convertToSgOrder();
    /// // Do something with sgOrder
    /// ```
    #[wasm_export(
        js_name = "convertToSgOrder",
        return_description = "Order as SgOrder type",
        unchecked_return_type = "SgOrder"
    )]
    pub fn convert_to_sg_order(&self) -> Result<SgOrder, RaindexError> {
        let sg_order = self.clone().into_sg_order()?;
        Ok(sg_order)
    }

    /// Generates calldata for `IRaindexV7.takeOrders5` targeting this specific order.
    ///
    /// Unlike `getTakeOrdersCalldata` on `RaindexClient` which discovers orders from the subgraph,
    /// this method generates calldata for taking this single known order that the user has already
    /// selected. The method fetches a fresh quote at execution time using the provided pair indices.
    ///
    /// This method includes preflight simulation to validate the transaction will succeed.
    ///
    /// Parameters:
    /// - `inputIndex`: Index of the input token in the order's validInputs array
    /// - `outputIndex`: Index of the output token in the order's validOutputs array
    /// - `taker`: Address of the account that will execute the takeOrders transaction
    /// - `mode`: One of `buyExact`, `buyUpTo`, `spendExact`, or `spendUpTo`
    /// - `amount`: Target amount (output tokens for buy modes, input tokens for spend modes)
    /// - `priceCap`: human-readable decimal string for max sell per 1 buy
    ///
    /// Returns calldata plus pricing info:
    /// - `calldata`: ABI-encoded bytes for `takeOrders5`.
    /// - `effectivePrice`: expected blended sell per 1 buy from the simulation.
    /// - `prices`: per-leg ratios (single entry for this order).
    /// - `expectedSell`: simulated sell at current quotes.
    /// - `maxSellCap`: worst-case on-chain spend cap.
    ///
    /// ## Example (JS)
    /// ```javascript
    /// const res = await order.getTakeCalldata(
    ///   0, // inputIndex
    ///   0, // outputIndex
    ///   "0xTAKER...",
    ///   "buyUpTo",
    ///   "10",
    ///   "1.2",
    /// );
    /// if (res.error) {
    ///   console.error(res.error.readableMsg);
    /// } else {
    ///   const { calldata, effectivePrice, expectedSell, maxSellCap, prices, raindex } = res.value;
    /// }
    /// ```
    #[wasm_export(
        js_name = "getTakeCalldata",
        return_description = "Encoded takeOrders5 calldata and price information for this order",
        unchecked_return_type = "TakeOrdersCalldataResult",
        preserve_js_class
    )]
    pub async fn get_take_calldata(
        &self,
        #[wasm_export(
            js_name = "inputIndex",
            param_description = "Index of the input token in the order's validInputs array"
        )]
        input_index: u32,
        #[wasm_export(
            js_name = "outputIndex",
            param_description = "Index of the output token in the order's validOutputs array"
        )]
        output_index: u32,
        #[wasm_export(param_description = "Address of the taker account")] taker: String,
        #[wasm_export(
            param_description = "Take orders mode: buyExact, buyUpTo, spendExact, or spendUpTo"
        )]
        mode: TakeOrdersMode,
        #[wasm_export(param_description = "Target amount as decimal string")] amount: String,
        #[wasm_export(
            js_name = "priceCap",
            param_description = "Maximum price cap as decimal string"
        )]
        price_cap: String,
    ) -> Result<TakeOrdersCalldataResult, RaindexError> {
        let taker_addr = Address::from_str(&taker)?;
        let parsed_mode = ParsedTakeOrdersMode::parse(mode, &amount)?;
        let parsed_price_cap = Float::parse(price_cap)?;

        let zero = Float::zero()?;
        if parsed_price_cap.lt(zero)? {
            return Err(RaindexError::NegativePriceCap);
        }

        let sell_token = self
            .inputs
            .get(input_index as usize)
            .ok_or(RaindexError::InvalidInputIndex(input_index))?
            .token()
            .address();
        #[cfg(target_family = "wasm")]
        let sell_token = Address::from_str(&sell_token)?;

        if (output_index as usize) >= self.outputs.len() {
            return Err(RaindexError::InvalidOutputIndex(output_index));
        }

        let rpc_urls = self.get_rpc_urls()?;

        let rpc_client = RpcClient::new_with_urls(rpc_urls.clone())?;
        let block_number = rpc_client.get_latest_block_number().await?;

        let fresh_quotes = self.get_quotes(Some(block_number), None).await?;

        let fresh_quote = fresh_quotes
            .into_iter()
            .find(|q| q.pair.input_index == input_index && q.pair.output_index == output_index)
            .ok_or(RaindexError::NoLiquidity)?;

        let candidate =
            build_candidate_from_quote(self, &fresh_quote)?.ok_or(RaindexError::NoLiquidity)?;

        let execution_params = TakeOrderExecutionParams {
            mode: parsed_mode,
            price_cap: parsed_price_cap,
            taker: taker_addr,
            sell_token,
        };
        let rpc_context = RpcContext {
            rpc_urls: &rpc_urls,
            block_number: Some(block_number),
        };

        execute_single_take(candidate, execution_params, rpc_context).await
    }

    #[wasm_export(
        js_name = "estimateTakeOrder",
        return_description = "Estimated spend/receive amounts and partial fill indicator",
        unchecked_return_type = "TakeOrderEstimate",
        preserve_js_class
    )]
    pub fn estimate_take_order_method(
        &self,
        #[wasm_export(param_description = "Quote for the order pair")] quote: &RaindexOrderQuote,
        #[wasm_export(
            js_name = "isBuy",
            param_description = "True if buying output token, false if selling input token"
        )]
        is_buy: bool,
        #[wasm_export(param_description = "Amount as decimal string")] amount: String,
    ) -> Result<TakeOrderEstimate, RaindexError> {
        let data = quote.data.as_ref().ok_or(RaindexError::QuoteDataMissing)?;
        let amount_float = Float::parse(amount)?;

        estimate_take_order(data.max_output, data.ratio, is_buy, amount_float)
    }
}

impl RaindexOrder {
    /// Non-wasm variant of [`Self::get_take_calldata`] that threads a
    /// caller-supplied [`SignedContextInjector`] through the quote pipeline.
    ///
    /// Mirrors the shape of [`RaindexClient::get_take_orders_calldata_with_injector`]:
    /// any injector-contributed `SignedContextV2` entries are appended after
    /// any oracle-fetched context on each `QuoteV3` before the quote multicall
    /// (composition order: `[oracle..., injected...]`). The candidate inherits
    /// that exact composed list, so `takeOrders5` executes with the same
    /// signed context the quote saw — required for gated orders whose
    /// `calculate-io` asserts on `signer<0>()` or similar.
    ///
    /// The injector is only needed at the quote stage; once the candidate is
    /// built with the correct `signed_context`, [`execute_single_take`] is
    /// sufficient without any injector parameter.
    #[allow(clippy::too_many_arguments)]
    pub async fn get_take_calldata_with_injector(
        &self,
        input_index: u32,
        output_index: u32,
        taker: String,
        mode: TakeOrdersMode,
        amount: String,
        price_cap: String,
        injector: &dyn raindex_quote::SignedContextInjector,
    ) -> Result<TakeOrdersCalldataResult, RaindexError> {
        let taker_addr = Address::from_str(&taker)?;
        let parsed_mode = ParsedTakeOrdersMode::parse(mode, &amount)?;
        let parsed_price_cap = Float::parse(price_cap)?;

        let zero = Float::zero()?;
        if parsed_price_cap.lt(zero)? {
            return Err(RaindexError::NegativePriceCap);
        }

        let sell_token = self
            .inputs
            .get(input_index as usize)
            .ok_or(RaindexError::InvalidInputIndex(input_index))?
            .token()
            .address();
        #[cfg(target_family = "wasm")]
        let sell_token = Address::from_str(&sell_token)?;

        if (output_index as usize) >= self.outputs.len() {
            return Err(RaindexError::InvalidOutputIndex(output_index));
        }

        let rpc_urls = self.get_rpc_urls()?;

        let rpc_client = RpcClient::new_with_urls(rpc_urls.clone())?;
        let block_number = rpc_client.get_latest_block_number().await?;

        let fresh_quotes = self
            .get_quotes_with_injector(Some(block_number), None, taker_addr, injector)
            .await?;

        let fresh_quote = fresh_quotes
            .into_iter()
            .find(|q| q.pair.input_index == input_index && q.pair.output_index == output_index)
            .ok_or(RaindexError::NoLiquidity)?;

        let candidate =
            build_candidate_from_quote(self, &fresh_quote)?.ok_or(RaindexError::NoLiquidity)?;

        let execution_params = TakeOrderExecutionParams {
            mode: parsed_mode,
            price_cap: parsed_price_cap,
            taker: taker_addr,
            sell_token,
        };
        let rpc_context = RpcContext {
            rpc_urls: &rpc_urls,
            block_number: Some(block_number),
        };

        execute_single_take(candidate, execution_params, rpc_context).await
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct RaindexOrderAsIO {
    #[tsify(type = "Hex")]
    pub id: B256,
    #[tsify(type = "Hex")]
    pub order_hash: B256,
    pub active: bool,
}
impl_wasm_traits!(RaindexOrderAsIO);
impl TryFrom<SgOrderAsIO> for RaindexOrderAsIO {
    type Error = RaindexError;
    fn try_from(order: SgOrderAsIO) -> Result<Self, Self::Error> {
        Ok(Self {
            id: B256::from_str(&order.id.0)?,
            order_hash: B256::from_str(&order.order_hash.0)?,
            active: order.active,
        })
    }
}
impl TryFrom<RaindexOrderAsIO> for SgOrderAsIO {
    type Error = RaindexError;
    fn try_from(order: RaindexOrderAsIO) -> Result<Self, Self::Error> {
        Ok(Self {
            id: SgBytes(order.id.to_string()),
            order_hash: SgBytes(order.order_hash.to_string()),
            active: order.active,
        })
    }
}

impl RaindexOrderAsIO {
    pub fn try_from_local_db_orders_csv(
        field_name: &str,
        csv: &Option<String>,
    ) -> Result<Vec<RaindexOrderAsIO>, RaindexError> {
        let mut result = Vec::new();
        let Some(csv_str) = csv.as_ref() else {
            return Ok(result);
        };
        if csv_str.is_empty() {
            return Ok(result);
        }

        let mut reader = ReaderBuilder::new()
            .has_headers(false)
            .delimiter(b':')
            .terminator(Terminator::Any(b','))
            .from_reader(Cursor::new(csv_str.as_bytes()));

        for record in reader.records() {
            let record = record.map_err(|err| {
                RaindexError::JsError(format!(
                    "Invalid {} entry: failed to parse record ({err})",
                    field_name
                ))
            })?;
            let mut fields = record.iter();
            let _id_str = fields.next().ok_or(RaindexError::JsError(format!(
                "Invalid {} entry: missing id",
                field_name
            )))?;
            let hash_str = fields.next().ok_or(RaindexError::JsError(format!(
                "Invalid {} entry: missing order hash",
                field_name
            )))?;
            let active_str = fields.next().ok_or(RaindexError::JsError(format!(
                "Invalid {} entry: missing active flag",
                field_name
            )))?;
            if fields.next().is_some() {
                return Err(RaindexError::JsError(format!(
                    "Invalid {} entry: too many fields",
                    field_name
                )));
            }
            let order_hash = B256::from_str(hash_str)?;
            let active = match active_str {
                "1" => true,
                "0" => false,
                _ => {
                    return Err(RaindexError::JsError(format!(
                        "Invalid active flag in {}: {}",
                        field_name, active_str
                    )))
                }
            };
            result.push(RaindexOrderAsIO {
                id: b256!("0x0000000000000000000000000000000000000000000000000000000000000001"),
                order_hash,
                active,
            });
        }
        Ok(result)
    }
}

#[wasm_export]
impl RaindexClient {
    /// Queries orders with filtering and pagination across configured networks
    ///
    /// Retrieves a list of orders from the specified network or all configured networks,
    /// with support for filtering by owner, status, and order hash. Results are paginated
    /// for efficient data retrieval.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await client.getOrders(
    ///   137, // Polygon network
    ///   {
    ///     owners: ["0x1234567890abcdef1234567890abcdef12345678"],
    ///     active: true
    ///   },
    ///   1
    /// );
    /// if (result.error) {
    ///   console.error("Error fetching orders:", result.error.readableMsg);
    ///   return;
    /// }
    /// const { orders, totalCount } = result.value;
    /// // Do something with orders
    /// ```
    #[wasm_export(
        js_name = "getOrders",
        return_description = "Orders list result with total count for pagination",
        unchecked_return_type = "RaindexOrdersListResult",
        preserve_js_class
    )]
    pub async fn get_orders(
        &self,
        #[wasm_export(
            js_name = "chainIds",
            param_description = "Specific blockchain network to query (optional, queries all networks if not specified)"
        )]
        chain_ids: Option<ChainIds>,
        #[wasm_export(
            param_description = "Filtering criteria including owners, active status, and order hash (optional)"
        )]
        filters: Option<GetOrdersFilters>,
        #[wasm_export(param_description = "Page number for pagination (optional, defaults to 1)")]
        page: Option<u16>,
        #[wasm_export(
            js_name = "pageSize",
            param_description = "Number of items per page (optional, defaults to 100)"
        )]
        page_size: Option<u16>,
    ) -> Result<RaindexOrdersListResult, RaindexError> {
        let filters = filters.unwrap_or_default();
        let filter_summary = summarize_order_filters(&filters);
        let page_number = page.unwrap_or(1).max(1);
        let page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE).max(1);
        let ids = chain_ids.map(|ChainIds(ids)| ids);
        let requested_chain_ids_count = ids.as_ref().map_or(0, Vec::len);
        let span = info_span!(
            "get_orders",
            requested_chain_ids_count,
            page = page_number,
            page_size,
            owners_count = filter_summary.owners_count,
            has_active_filter = filter_summary.has_active_filter,
            has_order_hash_filter = filter_summary.has_order_hash_filter,
            input_tokens_count = filter_summary.input_tokens_count,
            output_tokens_count = filter_summary.output_tokens_count,
            raindexes_count = filter_summary.raindexes_count,
            has_positive_output_vault_balance_filter =
                filter_summary.has_positive_output_vault_balance_filter,
        );

        async move {
            let started_at = Timing::now();
            let (local_db, local_ids, sg_ids) = self.classify_chains(ids)?;
            let local_chain_ids_count = local_ids.len();
            let subgraph_chain_ids_count = sg_ids.len();
            let query_source = query_source_label(local_db.is_some(), !sg_ids.is_empty());

            info!(
                query_source,
                local_chain_ids_count, subgraph_chain_ids_count, "fetching orders"
            );

            let mut all_orders = Vec::new();
            let mut total_count: u32 = 0;
            let mut local_rows = 0usize;
            let mut subgraph_rows = 0usize;

            if let Some(db) = local_db {
                let local_started_at = Timing::now();
                let local_source = LocalDbOrders::new(&db, ClientRef::new(self.clone()));
                let result = local_source
                    .list(
                        Some(local_ids),
                        &filters,
                        Some(page_number),
                        Some(page_size),
                    )
                    .instrument(info_span!("orders.fetching_local_db"))
                    .await
                    .map_err(|err| {
                        error!(
                            source = "local_db",
                            duration_ms = local_started_at.elapsed_ms(),
                            error = %err,
                            "failed fetching orders"
                        );
                        err
                    })?;
                local_rows = result.orders.len();
                total_count += result.total_count;
                all_orders.extend(result.orders);
                info!(
                    source = "local_db",
                    returned_order_count = local_rows,
                    total_count = result.total_count,
                    duration_ms = local_started_at.elapsed_ms(),
                    "fetched orders"
                );
            }

            if !sg_ids.is_empty() {
                let subgraph_started_at = Timing::now();
                let subgraph_source = SubgraphOrders::new(self);
                let result = subgraph_source
                    .list(Some(sg_ids), &filters, Some(page_number), Some(page_size))
                    .instrument(info_span!("orders.fetching_subgraph"))
                    .await
                    .map_err(|err| {
                        error!(
                            source = "subgraph",
                            duration_ms = subgraph_started_at.elapsed_ms(),
                            error = %err,
                            "failed fetching orders"
                        );
                        err
                    })?;
                subgraph_rows = result.orders.len();
                total_count += result.total_count;
                all_orders.extend(result.orders);
                info!(
                    source = "subgraph",
                    returned_order_count = subgraph_rows,
                    total_count = result.total_count,
                    duration_ms = subgraph_started_at.elapsed_ms(),
                    "fetched orders"
                );
            }

            info!(
                query_source,
                local_chain_ids_count,
                subgraph_chain_ids_count,
                local_rows,
                subgraph_rows,
                returned_order_count = all_orders.len(),
                total_count,
                duration_ms = started_at.elapsed_ms(),
                "completed get_orders"
            );

            Ok(RaindexOrdersListResult {
                orders: all_orders,
                total_count,
            })
        }
        .instrument(span)
        .await
    }

    /// Retrieves a specific order by its hash from a particular blockchain network
    ///
    /// Fetches complete order details including all vault information, metadata, and
    /// performance tracking capabilities for a specific order identified by its hash.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await client.getOrderByHash(
    ///   137, // Polygon network
    ///   "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
    /// );
    /// if (result.error) {
    ///   console.error("Error fetching order:", result.error.readableMsg);
    ///   return;
    /// }
    /// const order = result.value;
    /// // Do something with order
    /// ```
    #[wasm_export(
        js_name = "getOrderByHash",
        return_description = "Complete order details with vault and metadata information",
        unchecked_return_type = "RaindexOrder",
        preserve_js_class
    )]
    pub async fn get_order_by_hash_wasm_binding(
        &self,
        #[wasm_export(
            js_name = "chainId",
            param_description = "The blockchain network ID where the order exists"
        )]
        chain_id: u32,
        #[wasm_export(
            js_name = "raindexAddress",
            param_description = "Raindex contract address",
            unchecked_param_type = "Address"
        )]
        raindex_address: String,
        #[wasm_export(
            js_name = "orderHash",
            param_description = "The unique hash identifier of the order",
            unchecked_param_type = "Hex"
        )]
        order_hash: String,
    ) -> Result<RaindexOrder, RaindexError> {
        let raindex_address = Address::from_str(&raindex_address)?;
        let order_hash = B256::from_str(&order_hash)?;
        self.get_order_by_hash(
            &RaindexIdentifier::new(chain_id, raindex_address),
            order_hash,
        )
        .await
    }
}

#[cfg_attr(target_family = "wasm", async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
impl OrdersDataSource for SubgraphOrders<'_> {
    async fn list(
        &self,
        chain_ids: Option<Vec<u32>>,
        filters: &GetOrdersFilters,
        page: Option<u16>,
        page_size: Option<u16>,
    ) -> Result<RaindexOrdersListResult, RaindexError> {
        let raindex_client = ClientRef::new(self.client.clone());
        let multi_subgraph_args = self.client.get_multi_subgraph_args(chain_ids)?;

        let client = MultiRaindexSubgraphClient::new(
            multi_subgraph_args.values().flatten().cloned().collect(),
        );

        let sg_filter_args: SgOrdersListFilterArgs = filters.clone().try_into()?;
        let effective_page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE);

        let total_count = if page.is_some() {
            client.orders_count(sg_filter_args.clone()).await?
        } else {
            0
        };

        let orders = client
            .orders_list(
                sg_filter_args,
                SgPaginationArgs {
                    page: page.unwrap_or(1),
                    page_size: effective_page_size,
                },
            )
            .await?;

        let orders = orders
            .iter()
            .map(|order| {
                let chain_id = multi_subgraph_args
                    .iter()
                    .find(|(_, args)| args.iter().any(|arg| arg.name == order.subgraph_name))
                    .map(|(chain_id, _)| *chain_id)
                    .ok_or(RaindexError::SubgraphNotFound(
                        order.subgraph_name.clone(),
                        order.order.order_hash.0.clone(),
                    ))?;
                RaindexOrder::try_from_sg_order(
                    raindex_client.clone(),
                    chain_id,
                    order.order.clone(),
                    None,
                )
            })
            .collect::<Result<Vec<RaindexOrder>, RaindexError>>()?;

        Ok(RaindexOrdersListResult {
            orders,
            total_count,
        })
    }

    async fn get_by_hash(
        &self,
        raindex_id: &RaindexIdentifier,
        order_hash: &B256,
    ) -> Result<Option<RaindexOrder>, RaindexError> {
        let raindex_client = ClientRef::new(self.client.clone());
        let client = self
            .client
            .get_raindex_subgraph_client(raindex_id.raindex_address)?;
        let order = match client
            .order_detail_by_hash(SgBytes(order_hash.to_string()))
            .await
        {
            Ok(order) => order,
            Err(RaindexSubgraphClientError::Empty) => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let order =
            RaindexOrder::try_from_sg_order(raindex_client, raindex_id.chain_id, order, None)?;
        Ok(Some(order))
    }

    async fn get_added_by_tx_hash(
        &self,
        chain_id: u32,
        raindex: Address,
        tx_hash: B256,
    ) -> Result<Vec<RaindexOrder>, RaindexError> {
        let raindex_client = ClientRef::new(self.client.clone());
        let client = self.client.get_raindex_subgraph_client(raindex)?;
        let sg_orders = client
            .transaction_add_orders(Id::new(tx_hash.to_string()))
            .await?;
        sg_orders
            .into_iter()
            .map(|value| {
                let transaction = value.transaction.try_into()?;
                RaindexOrder::try_from_sg_order(
                    raindex_client.clone(),
                    chain_id,
                    value.order,
                    Some(transaction),
                )
            })
            .collect()
    }

    async fn get_removed_by_tx_hash(
        &self,
        chain_id: u32,
        raindex: Address,
        tx_hash: B256,
    ) -> Result<Vec<RaindexOrder>, RaindexError> {
        let raindex_client = ClientRef::new(self.client.clone());
        let client = self.client.get_raindex_subgraph_client(raindex)?;
        let sg_orders = client
            .transaction_remove_orders(Id::new(tx_hash.to_string()))
            .await?;
        sg_orders
            .into_iter()
            .map(|value| {
                let transaction = value.transaction.try_into()?;
                RaindexOrder::try_from_sg_order(
                    raindex_client.clone(),
                    chain_id,
                    value.order,
                    Some(transaction),
                )
            })
            .collect()
    }

    async fn trades_list(
        &self,
        raindex_id: &RaindexIdentifier,
        order_hash: &B256,
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
        page: Option<u16>,
    ) -> Result<Vec<RaindexTrade>, RaindexError> {
        let client = self
            .client
            .get_raindex_subgraph_client(raindex_id.raindex_address)?;

        let order = client
            .order_detail_by_hash(SgBytes(order_hash.to_string()))
            .await?;

        let trades = client
            .order_trades_list(
                Id::new(order.id.0.clone()),
                SgPaginationArgs {
                    page: page.unwrap_or(1),
                    page_size: DEFAULT_PAGE_SIZE,
                },
                start_timestamp,
                end_timestamp,
            )
            .await?;

        trades
            .into_iter()
            .map(|trade| RaindexTrade::try_from_sg_trade(raindex_id.chain_id, trade))
            .collect()
    }

    async fn trades_count(
        &self,
        raindex_id: &RaindexIdentifier,
        order_hash: &B256,
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
    ) -> Result<u64, RaindexError> {
        let client = self
            .client
            .get_raindex_subgraph_client(raindex_id.raindex_address)?;

        let order = client
            .order_detail_by_hash(SgBytes(order_hash.to_string()))
            .await?;

        let trades = client
            .order_trades_list_all(Id::new(order.id.0.clone()), start_timestamp, end_timestamp)
            .await?;

        Ok(trades.len() as u64)
    }
}

impl RaindexClient {
    pub async fn get_order_by_hash(
        &self,
        raindex_id: &RaindexIdentifier,
        order_hash: B256,
    ) -> Result<RaindexOrder, RaindexError> {
        let span = info_span!(
            "get_order_by_hash",
            chain_id = raindex_id.chain_id,
            raindex = %raindex_id.raindex_address,
            order_hash = %order_hash,
        );

        async move {
            let started_at = Timing::now();
            let raindex_cfg = self.get_raindex_by_address(raindex_id.raindex_address)?;
            if raindex_cfg.network.chain_id != raindex_id.chain_id {
                let err = RaindexError::RaindexNotFound(
                    raindex_id.raindex_address.to_string(),
                    raindex_id.chain_id,
                );
                warn!(
                    configured_chain_id = raindex_cfg.network.chain_id,
                    duration_ms = started_at.elapsed_ms(),
                    error = %err,
                    "order lookup rejected for mismatched raindex chain"
                );
                return Err(err);
            }

            let mut order = match self.query_source(raindex_id.chain_id) {
                QuerySource::LocalDb(local_db) => {
                    let source_started_at = Timing::now();
                    let local_source = LocalDbOrders::new(&local_db, ClientRef::new(self.clone()));
                    let order = local_source
                        .get_by_hash(raindex_id, &order_hash)
                        .instrument(info_span!("order.fetching_local_db"))
                        .await
                        .map_err(|err| {
                            error!(
                                source = "local_db",
                                duration_ms = source_started_at.elapsed_ms(),
                                error = %err,
                                "failed fetching order by hash"
                            );
                            err
                        })?
                        .ok_or_else(|| {
                            RaindexError::OrderNotFound(
                                raindex_id.raindex_address.to_string(),
                                raindex_id.chain_id,
                                order_hash,
                            )
                        })?;
                    info!(
                        source = "local_db",
                        duration_ms = source_started_at.elapsed_ms(),
                        "fetched order by hash"
                    );
                    order
                }
                QuerySource::Subgraph => {
                    let source_started_at = Timing::now();
                    let order = SubgraphOrders::new(self)
                        .get_by_hash(raindex_id, &order_hash)
                        .instrument(info_span!("order.fetching_subgraph"))
                        .await
                        .map_err(|err| {
                            error!(
                                source = "subgraph",
                                duration_ms = source_started_at.elapsed_ms(),
                                error = %err,
                                "failed fetching order by hash"
                            );
                            err
                        })?
                        .ok_or_else(|| {
                            RaindexError::OrderNotFound(
                                raindex_id.raindex_address.to_string(),
                                raindex_id.chain_id,
                                order_hash,
                            )
                        })?;
                    info!(
                        source = "subgraph",
                        duration_ms = source_started_at.elapsed_ms(),
                        "fetched order by hash"
                    );
                    order
                }
            };

            let dotrain_started_at = Timing::now();
            order.fetch_dotrain_source().await.map_err(|err| {
                error!(
                    duration_ms = dotrain_started_at.elapsed_ms(),
                    error = %err,
                    "failed fetching order dotrain source"
                );
                err
            })?;
            info!(
                has_dotrain_source = order.rainlang().is_some(),
                duration_ms = dotrain_started_at.elapsed_ms(),
                "fetched order dotrain source"
            );

            info!(
                duration_ms = started_at.elapsed_ms(),
                "completed get_order_by_hash"
            );
            Ok(order)
        }
        .instrument(span)
        .await
    }
}

impl RaindexClient {
    pub(crate) async fn fetch_orders_for_pair(
        &self,
        chain_id: u32,
        sell_token: Address,
        buy_token: Address,
    ) -> Result<Vec<RaindexOrder>, RaindexError> {
        let started_at = Timing::now();
        let filters = GetOrdersFilters {
            owners: vec![],
            active: Some(true),
            order_hash: None,
            tokens: Some(GetOrdersTokenFilter {
                inputs: Some(vec![sell_token]),
                outputs: Some(vec![buy_token]),
            }),
            raindex_addresses: None,
            has_positive_output_vault_balance: None,
        };

        let ids = Some(vec![chain_id]);
        let (local_db, local_ids, sg_ids) = self.classify_chains(ids)?;
        let local_chain_ids_count = local_ids.len();
        let subgraph_chain_ids_count = sg_ids.len();
        let query_source = match (local_db.is_some(), sg_ids.is_empty()) {
            (true, true) => "local_db",
            (false, false) => "subgraph",
            (true, false) => "mixed",
            (false, true) => "none",
        };

        info!(
            chain_id,
            sell_token = %sell_token,
            buy_token = %buy_token,
            query_source,
            local_chain_ids_count,
            subgraph_chain_ids_count,
            "fetching orders for token pair"
        );

        let mut all_orders = Vec::new();

        if let Some(db) = local_db {
            let local_source = LocalDbOrders::new(&db, ClientRef::new(self.clone()));
            let result = local_source
                .list(Some(local_ids), &filters, None, None)
                .await?;
            info!(
                chain_id,
                source = "local_db",
                returned_order_count = result.orders.len(),
                "fetched local orders for token pair"
            );
            all_orders.extend(result.orders);
        }

        if !sg_ids.is_empty() {
            let subgraph_source = SubgraphOrders::new(self);
            let result = subgraph_source
                .list(Some(sg_ids), &filters, None, None)
                .await?;
            info!(
                chain_id,
                source = "subgraph",
                returned_order_count = result.orders.len(),
                "fetched subgraph orders for token pair"
            );
            all_orders.extend(result.orders);
        }

        if all_orders.is_empty() {
            warn!(
                chain_id,
                sell_token = %sell_token,
                buy_token = %buy_token,
                query_source,
                local_chain_ids_count,
                subgraph_chain_ids_count,
                returned_order_count = 0usize,
                no_liquidity = true,
                duration_ms = started_at.elapsed_ms(),
                "no orders found for token pair"
            );
            return Err(RaindexError::NoLiquidity);
        }

        info!(
            chain_id,
            sell_token = %sell_token,
            buy_token = %buy_token,
            query_source,
            local_chain_ids_count,
            subgraph_chain_ids_count,
            returned_order_count = all_orders.len(),
            no_liquidity = false,
            duration_ms = started_at.elapsed_ms(),
            "fetched orders for token pair"
        );
        log_order_inventory_for_pair(chain_id, sell_token, buy_token, &all_orders);
        Ok(all_orders)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Tsify, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetOrdersFilters {
    #[tsify(type = "Address[]")]
    pub owners: Vec<Address>,
    #[tsify(optional)]
    pub active: Option<bool>,
    #[tsify(optional, type = "Hex")]
    pub order_hash: Option<B256>,
    #[tsify(optional)]
    pub tokens: Option<GetOrdersTokenFilter>,
    #[tsify(optional, type = "Address[]")]
    pub raindex_addresses: Option<Vec<Address>>,
    #[tsify(optional)]
    pub has_positive_output_vault_balance: Option<bool>,
}
impl_wasm_traits!(GetOrdersFilters);

#[derive(Serialize, Deserialize, Debug, Clone, Tsify, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetOrdersTokenFilter {
    #[cfg_attr(target_family = "wasm", tsify(optional, type = "Address[]"))]
    pub inputs: Option<Vec<Address>>,
    #[cfg_attr(target_family = "wasm", tsify(optional, type = "Address[]"))]
    pub outputs: Option<Vec<Address>>,
}
impl_wasm_traits!(GetOrdersTokenFilter);

impl TryFrom<GetOrdersFilters> for SgOrdersListFilterArgs {
    type Error = RaindexError;
    fn try_from(filters: GetOrdersFilters) -> Result<Self, Self::Error> {
        let tokens = filters.tokens.map(|token_filter| {
            let inputs = token_filter.inputs.unwrap_or_default();
            let outputs = token_filter.outputs.unwrap_or_default();
            let map_tokens = |list: Vec<Address>| {
                list.into_iter()
                    .map(|token| token.to_string().to_lowercase())
            };

            let inputs: Vec<String> = map_tokens(inputs).collect();
            let outputs: Vec<String> = map_tokens(outputs).collect();
            if inputs.is_empty() && outputs.is_empty() {
                None
            } else {
                Some(SgOrdersTokensFilterArgs { inputs, outputs })
            }
        });

        Ok(Self {
            owners: filters
                .owners
                .into_iter()
                .map(|owner| SgBytes(owner.to_string()))
                .collect(),
            active: filters.active,
            order_hash: filters
                .order_hash
                .map(|order_hash| SgBytes(order_hash.to_string())),
            tokens: tokens.flatten(),
            raindexes: filters
                .raindex_addresses
                .map(|addrs| {
                    addrs
                        .into_iter()
                        .map(|addr| addr.to_string().to_lowercase())
                        .collect()
                })
                .unwrap_or_default(),
            has_positive_output_vault_balance: filters.has_positive_output_vault_balance,
        })
    }
}

impl RaindexOrder {
    pub async fn fetch_dotrain_source(&mut self) -> Result<(), RaindexError> {
        if self
            .parsed_meta
            .iter()
            .any(|meta| matches!(meta, ParsedMeta::DotrainSourceV1(_)))
        {
            return Ok(());
        }

        let order_builder_state = match self.parsed_meta.iter().find_map(|meta| {
            if let ParsedMeta::OrderBuilderStateV1(state) = meta {
                Some(state.clone())
            } else {
                None
            }
        }) {
            Some(state) => state,
            None => return Ok(()),
        };

        let client = match self.get_metaboard_client()? {
            Some(client) => client,
            None => return Ok(()),
        };

        let subject_hash = order_builder_state.dotrain_hash();

        let metabytes = match client
            .get_metabytes_by_subject(&MetaBytes(alloy::hex::encode_prefixed(subject_hash)))
            .await
        {
            Ok(bytes) => bytes,
            Err(_) => return Ok(()),
        };

        for meta_bytes in metabytes {
            let documents = match RainMetaDocumentV1Item::cbor_decode(&meta_bytes) {
                Ok(documents) => documents,
                Err(_) => continue,
            };

            for document in documents {
                if document.magic == KnownMagic::DotrainSourceV1 {
                    if let Ok(source) = DotrainSourceV1::try_from(document) {
                        self.parsed_meta.push(ParsedMeta::DotrainSourceV1(source));
                        return Ok(());
                    }
                }
            }
        }

        Ok(())
    }

    pub fn try_from_sg_order(
        raindex_client: ClientRef,
        chain_id: u32,
        order: SgOrder,
        transaction: Option<RaindexTransaction>,
    ) -> Result<Self, RaindexError> {
        let rainlang = order
            .meta
            .as_ref()
            .and_then(|meta| meta.0.try_decode_rainlangsource().ok());

        Ok(Self {
            raindex_client: ClientRef::clone(&raindex_client),
            chain_id,
            id: B256::from_str(&order.id.0)?,
            order_bytes: Bytes::from_str(&order.order_bytes.0)?,
            order_hash: B256::from_str(&order.order_hash.0)?,
            owner: Address::from_str(&order.owner.0)?,
            inputs: {
                order
                    .inputs
                    .iter()
                    .map(|v| {
                        RaindexVault::try_from_sg_vault(
                            ClientRef::clone(&raindex_client),
                            chain_id,
                            v.clone(),
                            Some(RaindexVaultType::Input),
                        )
                    })
                    .collect::<Result<Vec<RaindexVault>, RaindexError>>()?
            },
            outputs: {
                order
                    .outputs
                    .iter()
                    .map(|v| {
                        RaindexVault::try_from_sg_vault(
                            ClientRef::clone(&raindex_client),
                            chain_id,
                            v.clone(),
                            Some(RaindexVaultType::Output),
                        )
                    })
                    .collect::<Result<Vec<RaindexVault>, RaindexError>>()?
            },
            raindex: Address::from_str(&order.raindex.id.0)?,
            active: order.active,
            timestamp_added: U256::from_str(&order.timestamp_added.0)?,
            timestamp_removed: latest_remove_timestamp(&order)?,
            meta: order
                .meta
                .clone()
                .map(|meta| Bytes::from_str(&meta.0))
                .transpose()?,
            parsed_meta: order
                .meta
                .as_ref()
                .and_then(|meta| alloy::hex::decode(&meta.0).ok())
                .and_then(|bytes| ParsedMeta::parse_from_bytes(&bytes).ok())
                .unwrap_or_default(),
            rainlang,
            transaction,
            trades_count: order.trades.len() as u16,
        })
    }

    pub fn into_sg_order(self) -> Result<SgOrder, RaindexError> {
        #[cfg(target_family = "wasm")]
        let timestamp_added = self.timestamp_added.to_string();
        #[cfg(not(target_family = "wasm"))]
        let timestamp_added = self.timestamp_added().to_string();

        Ok(SgOrder {
            id: SgBytes(self.id().to_string()),
            order_bytes: SgBytes(self.order_bytes().to_string()),
            order_hash: SgBytes(self.order_hash().to_string()),
            owner: SgBytes(self.owner().to_string()),
            outputs: self
                .outputs
                .clone()
                .into_iter()
                .map(|v| v.into_sg_vault())
                .collect::<Result<Vec<SgVault>, RaindexError>>()?,
            inputs: self
                .inputs
                .clone()
                .into_iter()
                .map(|v| v.into_sg_vault())
                .collect::<Result<Vec<SgVault>, RaindexError>>()?,
            raindex: SgRaindex {
                id: SgBytes(self.raindex().to_string()),
            },
            active: self.active(),
            timestamp_added: SgBigInt(timestamp_added),
            meta: self.meta().map(|meta| SgBytes(meta.to_string())),
            add_events: vec![],
            remove_events: vec![],
            trades: vec![],
        })
    }
}

fn latest_remove_timestamp(order: &SgOrder) -> Result<Option<U256>, RaindexError> {
    order
        .remove_events
        .iter()
        .map(|event| U256::from_str(&event.transaction.timestamp.0))
        .try_fold(None::<U256>, |latest, timestamp| {
            let timestamp = timestamp?;
            Ok::<Option<U256>, RaindexError>(Some(match latest {
                Some(latest) => latest.max(timestamp),
                None => timestamp,
            }))
        })
}

/// Fetch dotrain sources for a batch of orders with bounded concurrency.
pub(crate) async fn fetch_orders_dotrain_sources(
    orders: Vec<RaindexOrder>,
) -> Result<Vec<RaindexOrder>, RaindexError> {
    stream::iter(orders.into_iter().map(|mut order| async move {
        order.fetch_dotrain_source().await?;
        Ok::<_, RaindexError>(order)
    }))
    .buffered(MAX_CONCURRENT_DOTRAIN_SOURCE_FETCHES)
    .try_collect::<Vec<_>>()
    .await
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_family = "wasm"))]
    mod non_wasm {
        use super::super::*;
        use crate::raindex_client::tests::{
            get_test_yaml, new_with_local_db, CHAIN_ID_1_RAINDEX_ADDRESS,
        };
        use crate::{
            local_db::query::{
                fetch_orders::LocalDbOrder, FromDbJson, LocalDbQueryError, LocalDbQueryExecutor,
                SqlStatement, SqlStatementBatch,
            },
            raindex_client::local_db::LocalDb,
        };
        use alloy::primitives::{address, b256, U256};
        use httpmock::Method::POST;
        use httpmock::MockServer;
        use rain_math_float::Float;
        use rain_metadata::types::dotrain::{
            order_builder_state_v1::OrderBuilderStateV1, source_v1::DotrainSourceV1,
        };
        use rain_metadata::{
            ContentEncoding, ContentLanguage, ContentType, KnownMagic, RainMetaDocumentV1Item,
        };
        use raindex_subgraph_client::types::common::{
            SgAddOrder, SgBigInt, SgBytes, SgErc20, SgOrderAsIO, SgRaindex, SgRemoveOrder,
            SgTransaction, SgVault,
        };
        use raindex_subgraph_client::utils::float::*;
        use serde_bytes::ByteBuf;
        use serde_json::{json, Value};
        use std::collections::BTreeMap;
        use std::sync::Arc;
        use tracing_test::traced_test;

        fn sample_dotrain_source() -> DotrainSourceV1 {
            DotrainSourceV1("sample dotrain source".to_string())
        }

        fn sample_order_builder_state(source: &DotrainSourceV1) -> OrderBuilderStateV1 {
            OrderBuilderStateV1 {
                dotrain_hash: source.hash(),
                field_values: BTreeMap::new(),
                deposits: BTreeMap::new(),
                select_tokens: BTreeMap::new(),
                vault_ids: BTreeMap::new(),
                selected_deployment: "0".to_string(),
            }
        }

        fn encode_meta_items_hex(items: Vec<RainMetaDocumentV1Item>) -> String {
            let encoded =
                RainMetaDocumentV1Item::cbor_encode_seq(&items, KnownMagic::RainMetaDocumentV1)
                    .expect("meta encoding should succeed");
            format!("0x{}", alloy::hex::encode(encoded))
        }
        use std::str::FromStr;

        #[test]
        fn order_filter_trace_summary_counts_optional_filters() {
            let filters = GetOrdersFilters {
                owners: vec![address!("0000000000000000000000000000000000000001")],
                active: Some(true),
                order_hash: Some(b256!(
                    "0000000000000000000000000000000000000000000000000000000000000002"
                )),
                tokens: Some(GetOrdersTokenFilter {
                    inputs: Some(vec![address!("0000000000000000000000000000000000000003")]),
                    outputs: Some(vec![
                        address!("0000000000000000000000000000000000000004"),
                        address!("0000000000000000000000000000000000000005"),
                    ]),
                }),
                raindex_addresses: Some(vec![address!("0000000000000000000000000000000000000006")]),
                has_positive_output_vault_balance: Some(true),
            };

            assert_eq!(
                summarize_order_filters(&filters),
                OrderFilterTraceSummary {
                    owners_count: 1,
                    has_active_filter: true,
                    has_order_hash_filter: true,
                    input_tokens_count: 1,
                    output_tokens_count: 2,
                    raindexes_count: 1,
                    has_positive_output_vault_balance_filter: true,
                }
            );
        }

        #[test]
        fn query_source_label_describes_selected_sources() {
            assert_eq!(query_source_label(true, true), "mixed");
            assert_eq!(query_source_label(true, false), "local_db");
            assert_eq!(query_source_label(false, true), "subgraph");
            assert_eq!(query_source_label(false, false), "none");
        }

        async fn build_client_with_metaboard(
            metaboard_url: &str,
            subgraph_url: &str,
        ) -> RaindexClient {
            let yaml = get_test_yaml(
                subgraph_url,
                subgraph_url,
                "http://localhost:8545",
                "http://localhost:9545",
            );
            let yaml = yaml
                .replace("https://api.thegraph.com/subgraphs/name/xyz", metaboard_url)
                .replace(
                    "https://api.thegraph.com/subgraphs/name/polygon",
                    metaboard_url,
                );
            RaindexClient::new(vec![yaml], None, None).await.unwrap()
        }

        #[tokio::test]
        async fn try_from_sg_order_populates_parsed_meta() {
            let source = sample_dotrain_source();
            let builder_state = sample_order_builder_state(&source);
            let meta_hex = encode_meta_items_hex(vec![
                RainMetaDocumentV1Item::from(source.clone()),
                RainMetaDocumentV1Item::try_from(builder_state.clone()).unwrap(),
            ]);

            let mut sg_order = get_order1();
            sg_order.meta = Some(SgBytes(meta_hex));

            let client = RaindexClient::new(
                vec![get_test_yaml(
                    "http://localhost:3000/sg1",
                    "http://localhost:3000/sg2",
                    "http://localhost:3000/rpc1",
                    "http://localhost:3000/rpc2",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let order =
                RaindexOrder::try_from_sg_order(Arc::new(client), 1, sg_order, None).unwrap();

            assert_eq!(order.parsed_meta().len(), 2);
            assert_eq!(order.dotrain_source(), Some(source.0));
            let parsed_builder_state: OrderBuilderStateV1 =
                serde_json::from_str(&order.order_builder_state().unwrap()).unwrap();
            assert_eq!(parsed_builder_state, builder_state);
        }

        #[tokio::test]
        async fn try_from_sg_order_uses_latest_remove_timestamp() {
            let mut sg_order = get_order1();
            sg_order.remove_events = vec![
                SgRemoveOrder {
                    transaction: SgTransaction {
                        id: SgBytes(
                            "0x1111111111111111111111111111111111111111111111111111111111111111"
                                .to_string(),
                        ),
                        from: SgBytes("0x0000000000000000000000000000000000000001".to_string()),
                        block_number: SgBigInt("100".to_string()),
                        timestamp: SgBigInt("200".to_string()),
                    },
                },
                SgRemoveOrder {
                    transaction: SgTransaction {
                        id: SgBytes(
                            "0x2222222222222222222222222222222222222222222222222222222222222222"
                                .to_string(),
                        ),
                        from: SgBytes("0x0000000000000000000000000000000000000002".to_string()),
                        block_number: SgBigInt("101".to_string()),
                        timestamp: SgBigInt("300".to_string()),
                    },
                },
            ];

            let client = RaindexClient::new(
                vec![get_test_yaml(
                    "http://localhost:3000/sg1",
                    "http://localhost:3000/sg2",
                    "http://localhost:3000/rpc1",
                    "http://localhost:3000/rpc2",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let order =
                RaindexOrder::try_from_sg_order(Arc::new(client), 1, sg_order, None).unwrap();

            assert_eq!(order.timestamp_removed(), Some(U256::from(300)));
        }

        #[tokio::test]
        async fn from_local_db_order_populates_parsed_meta() {
            let source = sample_dotrain_source();
            let builder_state = sample_order_builder_state(&source);
            let meta_hex = encode_meta_items_hex(vec![
                RainMetaDocumentV1Item::from(source.clone()),
                RainMetaDocumentV1Item::try_from(builder_state.clone()).unwrap(),
            ]);
            let meta_bytes =
                alloy::hex::decode(meta_hex.strip_prefix("0x").unwrap_or(&meta_hex)).unwrap();

            let client = RaindexClient::new(
                vec![get_test_yaml(
                    "http://localhost:3000/sg1",
                    "http://localhost:3000/sg2",
                    "http://localhost:3000/rpc1",
                    "http://localhost:3000/rpc2",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let local_order = LocalDbOrder {
                chain_id: 1,
                order_hash: b256!(
                    "0000000000000000000000000000000000000000000000000000000000000abc"
                ),
                owner: address!("0x0000000000000000000000000000000000000001"),
                block_timestamp: 1,
                block_timestamp_removed: None,
                block_number: 1,
                raindex_address: Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                order_bytes: Bytes::from_str("0x01").unwrap(),
                transaction_hash: b256!(
                    "0000000000000000000000000000000000000000000000000000000000000002"
                ),
                inputs: None,
                outputs: None,
                trade_count: 0,
                active: true,
                meta: Some(Bytes::from(meta_bytes)),
            };

            let order =
                RaindexOrder::from_local_db_order(Arc::new(client), local_order, vec![], vec![])
                    .unwrap();

            assert_eq!(order.parsed_meta().len(), 2);
            assert_eq!(order.dotrain_source(), Some(source.0));
            let parsed_builder_state: OrderBuilderStateV1 =
                serde_json::from_str(&order.order_builder_state().unwrap()).unwrap();
            assert_eq!(parsed_builder_state, builder_state);
        }

        #[tokio::test]
        async fn from_local_db_order_populates_timestamp_removed() {
            let client = RaindexClient::new(
                vec![get_test_yaml(
                    "http://localhost:3000/sg1",
                    "http://localhost:3000/sg2",
                    "http://localhost:3000/rpc1",
                    "http://localhost:3000/rpc2",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let local_order = LocalDbOrder {
                chain_id: 1,
                order_hash: b256!(
                    "0000000000000000000000000000000000000000000000000000000000000def"
                ),
                owner: address!("0x0000000000000000000000000000000000000001"),
                block_timestamp: 100,
                block_timestamp_removed: Some(200),
                block_number: 1,
                raindex_address: Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                order_bytes: Bytes::from_str("0x01").unwrap(),
                transaction_hash: b256!(
                    "0000000000000000000000000000000000000000000000000000000000000002"
                ),
                inputs: None,
                outputs: None,
                trade_count: 0,
                active: false,
                meta: None,
            };

            let order =
                RaindexOrder::from_local_db_order(Arc::new(client), local_order, vec![], vec![])
                    .unwrap();

            assert_eq!(order.timestamp_removed(), Some(U256::from(200)));
        }

        #[tokio::test]
        async fn fetch_dotrain_source_skips_when_source_already_present() {
            let source = sample_dotrain_source();
            let builder_state = sample_order_builder_state(&source);
            let meta_hex = encode_meta_items_hex(vec![
                RainMetaDocumentV1Item::from(source.clone()),
                RainMetaDocumentV1Item::try_from(builder_state).unwrap(),
            ]);

            let mut sg_order = get_order1();
            sg_order.meta = Some(SgBytes(meta_hex));

            let client = RaindexClient::new(
                vec![get_test_yaml(
                    "http://localhost:3000/sg1",
                    "http://localhost:3000/sg2",
                    "http://localhost:3000/rpc1",
                    "http://localhost:3000/rpc2",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let mut order =
                RaindexOrder::try_from_sg_order(Arc::new(client), 1, sg_order, None).unwrap();
            order.fetch_dotrain_source().await.unwrap();
            assert_eq!(order.parsed_meta().len(), 2);
        }

        #[tokio::test]
        async fn fetch_dotrain_source_returns_ok_without_builder_state() {
            let mut sg_order = get_order1();
            sg_order.meta = None;

            let client = RaindexClient::new(
                vec![get_test_yaml(
                    "http://localhost:3000/sg1",
                    "http://localhost:3000/sg2",
                    "http://localhost:3000/rpc1",
                    "http://localhost:3000/rpc2",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let mut order =
                RaindexOrder::try_from_sg_order(Arc::new(client), 1, sg_order, None).unwrap();
            order.fetch_dotrain_source().await.unwrap();
            assert!(order.parsed_meta().is_empty());
        }

        #[tokio::test]
        async fn fetch_dotrain_source_adds_source_from_metaboard() {
            let source = sample_dotrain_source();
            let builder_state = sample_order_builder_state(&source);
            let rainlang_meta = RainMetaDocumentV1Item {
                payload: ByteBuf::from("sample rainlang source".as_bytes()),
                magic: KnownMagic::RainlangSourceV1,
                content_type: ContentType::OctetStream,
                content_encoding: ContentEncoding::None,
                content_language: ContentLanguage::None,
            };
            let order_meta_hex = encode_meta_items_hex(vec![
                rainlang_meta,
                RainMetaDocumentV1Item::try_from(builder_state.clone()).unwrap(),
            ]);

            let server = MockServer::start_async().await;
            let meta_bytes = RainMetaDocumentV1Item::cbor_encode_seq(
                &vec![RainMetaDocumentV1Item::from(source.clone())],
                KnownMagic::RainMetaDocumentV1,
            )
            .unwrap();
            let metaboard_meta_hex = format!("0x{}", alloy::hex::encode(meta_bytes));

            server.mock(|when, then| {
                when.method(POST).path("/");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "metaV1S": [
                            {
                                "meta": metaboard_meta_hex,
                                "metaHash": "0x01",
                                "sender": "0x0000000000000000000000000000000000000000",
                                "id": "0x01",
                                "metaBoard": { "address": "0x0000000000000000000000000000000000000000" },
                                "subject": builder_state.dotrain_hash().to_string()
                            }
                        ]
                    }
                }));
            });

            let client =
                Arc::new(build_client_with_metaboard(&server.url("/"), &server.url("/sg")).await);

            let mut sg_order = get_order1();
            sg_order.meta = Some(SgBytes(order_meta_hex));

            let mut order =
                RaindexOrder::try_from_sg_order(Arc::clone(&client), 1, sg_order, None).unwrap();

            order.fetch_dotrain_source().await.unwrap();

            assert_eq!(order.parsed_meta().len(), 2);
            assert_eq!(order.dotrain_source(), Some(source.0));
            let parsed_builder_state: OrderBuilderStateV1 =
                serde_json::from_str(&order.order_builder_state().unwrap()).unwrap();
            assert_eq!(parsed_builder_state, builder_state);
        }

        #[tokio::test]
        async fn fetch_dotrain_source_handles_invalid_meta_bytes() {
            let source = sample_dotrain_source();
            let builder_state = sample_order_builder_state(&source);
            let rainlang_meta = RainMetaDocumentV1Item {
                payload: ByteBuf::from("sample rainlang source".as_bytes()),
                magic: KnownMagic::RainlangSourceV1,
                content_type: ContentType::OctetStream,
                content_encoding: ContentEncoding::None,
                content_language: ContentLanguage::None,
            };
            let meta_hex = encode_meta_items_hex(vec![
                rainlang_meta,
                RainMetaDocumentV1Item::try_from(builder_state.clone()).unwrap(),
            ]);

            let server = MockServer::start_async().await;

            server.mock(|when, then| {
                when.method(POST).path("/");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "metaV1S": [
                            {
                                "meta": "0xzz",
                                "metaHash": "0x01",
                                "sender": "0x0000000000000000000000000000000000000000",
                                "id": "0x01",
                                "metaBoard": { "address": "0x0000000000000000000000000000000000000000" },
                                "subject": builder_state.dotrain_hash().to_string()
                            }
                        ]
                    }
                }));
            });

            let client =
                Arc::new(build_client_with_metaboard(&server.url("/"), &server.url("/sg")).await);

            let mut sg_order = get_order1();
            sg_order.meta = Some(SgBytes(meta_hex));

            let mut order =
                RaindexOrder::try_from_sg_order(Arc::clone(&client), 1, sg_order, None).unwrap();

            order.fetch_dotrain_source().await.unwrap();

            assert_eq!(order.parsed_meta().len(), 1);
            let parsed_builder_state: OrderBuilderStateV1 =
                serde_json::from_str(&order.order_builder_state().unwrap()).unwrap();
            assert_eq!(parsed_builder_state, builder_state);
        }

        #[derive(Clone)]
        struct StaticJsonExec {
            json: String,
        }

        #[cfg_attr(target_family = "wasm", async_trait(?Send))]
        #[cfg_attr(not(target_family = "wasm"), async_trait)]
        impl LocalDbQueryExecutor for StaticJsonExec {
            async fn execute_batch(
                &self,
                _batch: &SqlStatementBatch,
            ) -> Result<(), LocalDbQueryError> {
                Ok(())
            }

            async fn query_json<T>(&self, _stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
            where
                T: FromDbJson,
            {
                serde_json::from_str(&self.json)
                    .map_err(|e| LocalDbQueryError::deserialization(e.to_string()))
            }

            async fn query_text(&self, _stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
                Err(LocalDbQueryError::database(
                    "query_text not supported in StaticJsonExec",
                ))
            }

            async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
                Err(LocalDbQueryError::not_implemented("wipe_and_recreate"))
            }
        }

        #[test]
        fn try_from_get_orders_filters_maps_directional_tokens() {
            let input = address!("0xF3dEe5b36E3402893e6953A8670E37D329683ABB");
            let output = address!("0x7D3Dd01feD0C16A6c353ce3BACF26467726EF96e");
            let filters = GetOrdersFilters {
                tokens: Some(GetOrdersTokenFilter {
                    inputs: Some(vec![input]),
                    outputs: Some(vec![output]),
                }),
                ..Default::default()
            };

            let args = SgOrdersListFilterArgs::try_from(filters).unwrap();
            let tokens = args.tokens.unwrap();
            assert_eq!(
                tokens.inputs,
                vec!["0xf3dee5b36e3402893e6953a8670e37d329683abb".to_string()]
            );
            assert_eq!(
                tokens.outputs,
                vec!["0x7d3dd01fed0c16a6c353ce3bacf26467726ef96e".to_string()]
            );
        }

        #[test]
        fn try_from_get_orders_filters_drops_empty_token_lists() {
            let filters = GetOrdersFilters {
                tokens: Some(GetOrdersTokenFilter {
                    inputs: Some(vec![]),
                    outputs: Some(vec![]),
                }),
                ..Default::default()
            };

            let args = SgOrdersListFilterArgs::try_from(filters).unwrap();
            assert!(args.tokens.is_none());
        }

        #[test]
        fn try_from_local_db_orders_csv_parses_records() {
            let csv = Some(
                "0xdeadbeef:0xabc0000000000000000000000000000000000000000000000000000000000001:1,\
                 0xdeadbeee:0xabc0000000000000000000000000000000000000000000000000000000000002:0"
                    .to_string(),
            );
            let parsed =
                RaindexOrderAsIO::try_from_local_db_orders_csv("inputOrders", &csv).unwrap();
            assert_eq!(parsed.len(), 2);
            assert_eq!(
                parsed[0].order_hash,
                B256::from_str(
                    "0xabc0000000000000000000000000000000000000000000000000000000000001"
                )
                .unwrap()
            );
            assert!(parsed[0].active);
            assert!(!parsed[1].active);
        }

        #[test]
        fn try_from_local_db_orders_csv_rejects_invalid_active_flag() {
            let csv = Some(
                "0xdeadbeef:0xabc0000000000000000000000000000000000000000000000000000000000001:maybe"
                    .to_string(),
            );
            let err =
                RaindexOrderAsIO::try_from_local_db_orders_csv("testField", &csv).unwrap_err();
            match err {
                RaindexError::JsError(msg) => {
                    assert!(msg.contains("Invalid active flag in testField: maybe"))
                }
                _ => panic!("expected JsError"),
            }
        }

        #[test]
        fn get_orders_filters_to_sg_filter_args_maps_raindex_addresses() {
            use alloy::primitives::address;
            use raindex_subgraph_client::types::common::SgOrdersListFilterArgs;

            let filters = GetOrdersFilters {
                owners: vec![],
                active: None,
                order_hash: None,
                tokens: None,
                raindex_addresses: Some(vec![
                    address!("0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
                    address!("0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"),
                ]),
                has_positive_output_vault_balance: None,
            };

            let sg_filter_args: SgOrdersListFilterArgs = filters.try_into().unwrap();

            assert_eq!(sg_filter_args.raindexes.len(), 2);
            assert_eq!(
                sg_filter_args.raindexes[0],
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            );
            assert_eq!(
                sg_filter_args.raindexes[1],
                "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            );
        }

        #[test]
        fn get_orders_filters_to_sg_filter_args_empty_raindex_addresses() {
            use raindex_subgraph_client::types::common::SgOrdersListFilterArgs;

            let filters = GetOrdersFilters {
                owners: vec![],
                active: None,
                order_hash: None,
                tokens: None,
                raindex_addresses: None,
                has_positive_output_vault_balance: None,
            };

            let sg_filter_args: SgOrdersListFilterArgs = filters.try_into().unwrap();

            assert!(sg_filter_args.raindexes.is_empty());
        }

        #[test]
        fn get_orders_filters_to_sg_filter_args_lowercases_mixed_case_addresses() {
            use alloy::primitives::address;
            use raindex_subgraph_client::types::common::SgOrdersListFilterArgs;

            let filters = GetOrdersFilters {
                owners: vec![],
                active: None,
                order_hash: None,
                tokens: None,
                raindex_addresses: Some(vec![address!(
                    "0xDeaDbEEfDeaDbEEfDeaDbEEfDeaDbEEfDeaDbEEf"
                )]),
                has_positive_output_vault_balance: None,
            };

            let sg_filter_args: SgOrdersListFilterArgs = filters.try_into().unwrap();

            assert_eq!(sg_filter_args.raindexes.len(), 1);
            assert_eq!(
                sg_filter_args.raindexes[0],
                "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
            );
        }

        fn get_order1_json() -> Value {
            json!(                        {
              "id": "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1",
              "orderBytes": "0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000f08bcbce72f62c95dcb7c07dcb5ed26acfcfbc1100000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000005c00000000000000000000000000000000000000000000000000000000000000640392c489ef67afdc348209452c338ea5ba2b6152b936e152f610d05e1a20621a40000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae60000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000049d000000000000000000000000000000000000000000000000000000000000000f0000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000c7d713b49da0000914d696e20747261646520616d6f756e742e00000000000000000000000000008b616d6f756e742d75736564000000000000000000000000000000000000000000000000000000000000000000000000000000000000000340aad21b3b70000000000000000000000000000000000000000000000000006194049f30f7200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b1a2bc2ec500000000000000000000000000000000000000000000000000000e043da6172500008f6c6173742d74726164652d74696d65000000000000000000000000000000008d6c6173742d74726164652d696f0000000000000000000000000000000000008c696e697469616c2d74696d650000000000000000000000000000000000000000000000000000000000000000000000000000000000000006f05b59d3b200000000000000000000000000000000000000000000000000008ac7230489e80000000000000000000000020000915e36ef882941816356bc3718df868054f868ad000000000000000000000000000000000000000000000000000000000000027d0a00000024007400e0015801b401e001f40218025c080500040b20000200100001001000000b120003001000010b110004001000030b0100051305000201100001011000003d120000011000020010000003100404211200001d02000001100003031000010c1200004911000003100404001000012b12000001100003031000010c1200004a0200001a0b00090b1000060b20000700100000001000011b1200001a10000047120000001000001a1000004712000001100000011000002e12000001100005011000042e120000001000053d12000001100004001000042e1200000010000601100005001000032e120000481200011d0b020a0010000001100000011000062713000001100003031000010c12000049110000001000030010000247120000001000010b110008001000050110000700100001201200001f12000001100000011000004712000000100006001000073d120000011000002b12000000100008001000043b120000160901080b1000070b10000901100008001000013d1200001b12000001100006001000013d1200000b100009001000033a120000001000040010000248120001001000000b110008001000053d12000000100006001000042b1200000a0401011a10000001100009031000010c1200004a020000001000000110000a031000010c1200004a020000040200010110000b031000010c120000491100000803000201100009031000010c120000491100000110000a031000010c12000049110000100c01030110000d001000002e1200000110000c3e1200000010000100100001001000010010000100100001001000010010000100100001001000013d1a0000020100010210000e3611000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33",
              "orderHash": "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
              "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
              "outputs": [
                {
                  "id": "0x49f6b665c395c7b975caa2fc167cb5119981bbb86798bcaf3c4570153d09dfcf",
                  "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                  "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                  "balance": Float::parse("0.987".to_string()).unwrap(),
                  "token": {
                    "id": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                    "address": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                    "name": "Staked FLR",
                    "symbol": "sFLR",
                    "decimals": "18"
                  },
                  "raindex": {
                    "id": "0xcee8cd002f151a536394e564b84076c41bbbcd4d"
                  },
                  "ordersAsOutput": [
                    {
                      "id": "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1",
                      "orderHash": "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
                      "active": true
                    }
                  ],
                  "ordersAsInput": [],
                  "balanceChanges": []
                },
                {
                    "id": "0x0000000000000000000000000000000000000000",
                    "token": {
                      "id": "0x0000000000000000000000000000000000000000",
                      "address": "0x0000000000000000000000000000000000000000",
                      "name": "T1",
                      "symbol": "T1",
                      "decimals": "0"
                    },
                    "balance": F0,
                    "vaultId": "0",
                    "owner": "0x0000000000000000000000000000000000000000",
                    "ordersAsOutput": [],
                    "ordersAsInput": [],
                    "balanceChanges": [],
                    "raindex": {
                      "id": "0x0000000000000000000000000000000000000000"
                    }
                  }
              ],
              "inputs": [
                {
                  "id": "0x538830b4f8cc03840cea5af799dc532be4363a3ee8f4c6123dbff7a0acc86dac",
                  "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                  "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                  "balance": Float::parse("0.79799".to_string()).unwrap(),
                  "token": {
                    "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                    "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                    "name": "Wrapped Flare",
                    "symbol": "WFLR",
                    "decimals": "18"
                  },
                  "raindex": {
                    "id": "0xcee8cd002f151a536394e564b84076c41bbbcd4d"
                  },
                  "ordersAsOutput": [],
                  "ordersAsInput": [
                    {
                      "id": "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1",
                      "orderHash": "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
                      "active": true
                    }
                  ],
                  "balanceChanges": []
                },
                {
                    "id": "0x0000000000000000000000000000000000000000",
                    "token": {
                      "id": "0x0000000000000000000000000000000000000000",
                      "address": "0x0000000000000000000000000000000000000000",
                      "name": "T1",
                      "symbol": "T1",
                      "decimals": "0"
                    },
                    "balance": F0,
                    "vaultId": "0",
                    "owner": "0x0000000000000000000000000000000000000000",
                    "ordersAsOutput": [],
                    "ordersAsInput": [],
                    "balanceChanges": [],
                    "raindex": {
                      "id": "0x0000000000000000000000000000000000000000"
                    }
                  }
              ],
              "raindex": {
                "id": CHAIN_ID_1_RAINDEX_ADDRESS
              },
              "active": true,
              "timestampAdded": "1739448802",
              "meta": "0xff0a89c674ee7874a3005902252f2a20302e2063616c63756c6174652d696f202a2f200a7573696e672d776f7264732d66726f6d203078466532343131434461313933443945346538334135633233344337466433323031303138383361430a616d743a203130302c0a696f3a2063616c6c3c323e28293b0a0a2f2a20312e2068616e646c652d696f202a2f200a3a63616c6c3c333e28292c0a3a656e7375726528657175616c2d746f286f75747075742d7661756c742d64656372656173652829203130302920226d7573742074616b652066756c6c20616d6f756e7422293b0a0a2f2a20322e206765742d696f2d726174696f2d6e6f77202a2f200a656c61707365643a2063616c6c3c343e28292c0a696f3a2073617475726174696e672d73756228302e3031373733353620646976286d756c28656c61707365642073756228302e3031373733353620302e30313733383434292920363029293b0a0a2f2a20332e206f6e652d73686f74202a2f200a3a656e737572652869732d7a65726f286765742868617368286f726465722d68617368282920226861732d657865637574656422292929202268617320657865637574656422292c0a3a7365742868617368286f726465722d68617368282920226861732d657865637574656422292031293b0a0a2f2a20342e206765742d656c6170736564202a2f200a5f3a20737562286e6f772829206765742868617368286f726465722d68617368282920226465706c6f792d74696d65222929293b011bff13109e41336ff20278186170706c69636174696f6e2f6f637465742d73747265616d",
              "addEvents": [
                {
                  "transaction": {
                    "id": "0xb5d715bc74b7a7f2aac8cca544c1c95e209ed4113b82269ac3285142324bc6af",
                    "from": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                    "blockNumber": "37432554",
                    "timestamp": "1739448802"
                  }
                }
              ],
              "trades": [],
              "removeEvents": []
            })
        }

        fn get_order1() -> SgOrder {
            SgOrder {
                id: SgBytes("0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1".to_string()),
                order_bytes: SgBytes("0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000f08bcbce72f62c95dcb7c07dcb5ed26acfcfbc1100000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000005c00000000000000000000000000000000000000000000000000000000000000640392c489ef67afdc348209452c338ea5ba2b6152b936e152f610d05e1a20621a40000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae60000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000049d000000000000000000000000000000000000000000000000000000000000000f0000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000c7d713b49da0000914d696e20747261646520616d6f756e742e00000000000000000000000000008b616d6f756e742d75736564000000000000000000000000000000000000000000000000000000000000000000000000000000000000000340aad21b3b70000000000000000000000000000000000000000000000000006194049f30f7200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b1a2bc2ec500000000000000000000000000000000000000000000000000000e043da6172500008f6c6173742d74726164652d74696d65000000000000000000000000000000008d6c6173742d74726164652d696f0000000000000000000000000000000000008c696e697469616c2d74696d650000000000000000000000000000000000000000000000000000000000000000000000000000000000000006f05b59d3b200000000000000000000000000000000000000000000000000008ac7230489e80000000000000000000000020000915e36ef882941816356bc3718df868054f868ad000000000000000000000000000000000000000000000000000000000000027d0a00000024007400e0015801b401e001f40218025c080500040b20000200100001001000000b120003001000010b110004001000030b0100051305000201100001011000003d120000011000020010000003100404211200001d02000001100003031000010c1200004911000003100404001000012b12000001100003031000010c1200004a0200001a0b00090b1000060b20000700100000001000011b1200001a10000047120000001000001a1000004712000001100000011000002e12000001100005011000042e120000001000053d12000001100004001000042e1200000010000601100005001000032e120000481200011d0b020a0010000001100000011000062713000001100003031000010c12000049110000001000030010000247120000001000010b110008001000050110000700100001201200001f12000001100000011000004712000000100006001000073d120000011000002b12000000100008001000043b120000160901080b1000070b10000901100008001000013d1200001b12000001100006001000013d1200000b100009001000033a120000001000040010000248120001001000000b110008001000053d12000000100006001000042b1200000a0401011a10000001100009031000010c1200004a020000001000000110000a031000010c1200004a020000040200010110000b031000010c120000491100000803000201100009031000010c120000491100000110000a031000010c12000049110000100c01030110000d001000002e1200000110000c3e1200000010000100100001001000010010000100100001001000010010000100100001001000013d1a0000020100010210000e3611000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33".to_string()),
                order_hash: SgBytes("0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4".to_string()),
                owner: SgBytes("0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11".to_string()),
                outputs: vec![SgVault {
                    id: SgBytes("0x49f6b665c395c7b975caa2fc167cb5119981bbb86798bcaf3c4570153d09dfcf".to_string()),
                    owner: SgBytes("0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11".to_string()),
                    vault_id: SgBytes("75486334982066122983501547829219246999490818941767825330875804445439814023987".to_string()),
                    balance: SgBytes(Float::parse("0.987".to_string()).unwrap().as_hex()),
                    token: SgErc20 {
                        id: SgBytes("0x12e605bc104e93b45e1ad99f9e555f659051c2bb".to_string()),
                        address: SgBytes("0x12e605bc104e93b45e1ad99f9e555f659051c2bb".to_string()),
                        name: Some("Staked FLR".to_string()),
                        symbol: Some("sFLR".to_string()),
                        decimals: Some(SgBigInt("18".to_string())),
                    },
                    raindex: SgRaindex {
                        id: SgBytes("0xcee8cd002f151a536394e564b84076c41bbbcd4d".to_string()),
                    },
                    orders_as_output: vec![SgOrderAsIO {
                        id: SgBytes("0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1".to_string()),
                        order_hash: SgBytes("0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4".to_string()),
                        active: true,
                    }],
                    orders_as_input: vec![],
                    balance_changes: vec![],
                },
                SgVault {
                    id: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                    token: SgErc20 {
                        id: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                        address: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                        name: Some("T1".to_string()),
                        symbol: Some("T1".to_string()),
                        decimals: Some(SgBigInt("0".to_string())),
                    },
                    balance: SgBytes(F0.as_hex()),
                    vault_id: SgBytes("0".to_string()),
                    owner: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                    orders_as_output: vec![],
                    orders_as_input: vec![],
                    balance_changes: vec![],
                    raindex: SgRaindex {
                        id: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                    }
                }],
                inputs: vec![SgVault {
                    id: SgBytes("0x538830b4f8cc03840cea5af799dc532be4363a3ee8f4c6123dbff7a0acc86dac".to_string()),
                    owner: SgBytes("0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11".to_string()),
                    vault_id: SgBytes("75486334982066122983501547829219246999490818941767825330875804445439814023987".to_string()),
                    balance: SgBytes(Float::parse("0.79799".to_string()).unwrap().as_hex()),
                    token: SgErc20 {
                        id: SgBytes("0x1d80c49bbbcd1c0911346656b529df9e5c2f783d".to_string()),
                        address: SgBytes("0x1d80c49bbbcd1c0911346656b529df9e5c2f783d".to_string()),
                        name: Some("Wrapped Flare".to_string()),
                        symbol: Some("WFLR".to_string()),
                        decimals: Some(SgBigInt("18".to_string())),
                    },
                    raindex: SgRaindex {
                        id: SgBytes("0xcee8cd002f151a536394e564b84076c41bbbcd4d".to_string()),
                    },
                    orders_as_output: vec![],
                    orders_as_input: vec![SgOrderAsIO {
                        id: SgBytes("0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1".to_string()),
                        order_hash: SgBytes("0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4".to_string()),
                        active: true,
                    }],
                    balance_changes: vec![],
                },
                SgVault {
                    id: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                    token: SgErc20 {
                        id: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                        address: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                        name: Some("T1".to_string()),
                        symbol: Some("T1".to_string()),
                        decimals: Some(SgBigInt("0".to_string())),
                    },
                    balance: SgBytes(F0.as_hex()),
                    vault_id: SgBytes("0".to_string()),
                    owner: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                    orders_as_output: vec![],
                    orders_as_input: vec![],
                    balance_changes: vec![],
                    raindex: SgRaindex {
                        id: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                    }
                }],
                raindex: SgRaindex {
                    id: SgBytes(CHAIN_ID_1_RAINDEX_ADDRESS.to_string()),
                },
                active: true,
                timestamp_added: SgBigInt("1739448802".to_string()),
                meta: Some(SgBytes("0xff0a89c674ee7874a300590a932f2a20302e2063616c63756c6174652d696f202a2f200a7573696e672d776f7264732d66726f6d20307846653234313143446131393344394534653833413563323334433746643332303130313838336143203078393135453336656638383239343138313633353662433337313844663836383035344638363861440a616d6f756e742d65706f6368730a74726164652d65706f6368733a63616c6c3c323e28292c0a6d61782d6f75747075743a2063616c6c3c333e28616d6f756e742d65706f6368732074726164652d65706f636873292c0a696f3a2063616c6c3c343e2874726164652d65706f636873292c0a3a63616c6c3c353e28696f293b0a0a2f2a20312e2068616e646c652d696f202a2f200a6d696e2d616d6f756e743a206d756c283120302e39292c0a3a656e7375726528677265617465722d7468616e2d6f722d657175616c2d746f286f75747075742d7661756c742d64656372656173652829206d696e2d616d6f756e742920224d696e20747261646520616d6f756e742e22292c0a757365643a206765742868617368286f726465722d6861736828292022616d6f756e742d757365642229292c0a3a7365742868617368286f726465722d6861736828292022616d6f756e742d757365642229206164642875736564206f75747075742d7661756c742d6465637265617365282929293b0a0a2f2a20322e206765742d65706f6368202a2f200a696e697469616c2d74696d653a2063616c6c3c363e28292c0a6c6173742d74696d65205f3a2063616c6c3c373e28292c0a6475726174696f6e3a20737562286e6f77282920616e79286c6173742d74696d6520696e697469616c2d74696d652929292c0a746f74616c2d6475726174696f6e3a20737562286e6f77282920696e697469616c2d74696d65292c0a726174696f2d667265657a652d616d6f756e742d65706f6368733a2064697628312031292c0a726174696f2d667265657a652d74726164652d65706f6368733a206d756c28726174696f2d667265657a652d616d6f756e742d65706f63687320646976283630203138303029292c0a616d6f756e742d65706f6368733a2064697628746f74616c2d6475726174696f6e203630292c0a74726164652d65706f6368733a2073617475726174696e672d73756228646976286475726174696f6e20313830302920726174696f2d667265657a652d74726164652d65706f636873293b0a0a2f2a20332e20616d6f756e742d666f722d65706f6368202a2f200a616d6f756e742d65706f6368730a74726164652d65706f6368733a2c0a746f74616c2d617661696c61626c653a206c696e6561722d67726f7774682830203120616d6f756e742d65706f636873292c0a757365643a206765742868617368286f726465722d6861736828292022616d6f756e742d757365642229292c0a756e757365643a2073756228746f74616c2d617661696c61626c652075736564292c0a64656361793a2063616c6c3c383e2874726164652d65706f636873292c0a7368792d64656361793a20657665727928677265617465722d7468616e2874726164652d65706f63687320302e303529206465636179292c0a7661726961626c652d636f6d706f6e656e743a2073756228312031292c0a7461726765742d616d6f756e743a206164642831206d756c287661726961626c652d636f6d706f6e656e74207368792d646563617929292c0a6361707065642d756e757365643a206d696e28756e75736564207461726765742d616d6f756e74293b0a0a2f2a20342e20696f2d666f722d65706f6368202a2f200a65706f63683a2c0a6c6173742d696f3a2063616c6c3c373e28292c0a6d61782d6e6578742d74726164653a20616e79286d756c286c6173742d696f20312e3031292063616c6c3c393e2829292c0a626173656c696e652d6e6578742d74726164653a206d756c286c6173742d696f2030292c0a7265616c2d626173656c696e653a206d617828626173656c696e652d6e6578742d74726164652063616c6c3c393e2829292c0a7661726961626c652d636f6d706f6e656e743a2073617475726174696e672d737562286d61782d6e6578742d7472616465207265616c2d626173656c696e65292c0a61626f76652d626173656c696e653a206d756c287661726961626c652d636f6d706f6e656e742063616c6c3c383e2865706f636829292c0a5f3a20616464287265616c2d626173656c696e652061626f76652d626173656c696e65293b0a0a2f2a20352e207365742d6c6173742d7472616465202a2f200a6c6173742d696f3a2c0a3a7365742868617368286f726465722d68617368282920226c6173742d74726164652d74696d652229206e6f772829292c0a3a7365742868617368286f726465722d68617368282920226c6173742d74726164652d696f2229206c6173742d696f293b0a0a2f2a20362e206765742d696e697469616c2d74696d65202a2f200a5f3a6765742868617368286f726465722d6861736828292022696e697469616c2d74696d652229293b0a0a2f2a20372e206765742d6c6173742d7472616465202a2f200a6c6173742d74696d653a6765742868617368286f726465722d68617368282920226c6173742d74726164652d74696d652229292c0a6c6173742d696f3a6765742868617368286f726465722d68617368282920226c6173742d74726164652d696f2229293b0a0a2f2a20382e2068616c666c696665202a2f200a65706f63683a2c0a2f2a2a0a202a20536872696e6b696e6720746865206d756c7469706c696572206c696b6520746869730a202a207468656e206170706c79696e672069742031302074696d657320616c6c6f777320666f720a202a2062657474657220707265636973696f6e207768656e206d61782d696f2d726174696f0a202a2069732076657279206c617267652c20652e672e207e31653130206f72207e316532302b0a202a0a202a205468697320776f726b7320626563617573652060706f77657260206c6f7365730a202a20707265636973696f6e206f6e20626173652060302e3560207768656e207468650a202a206578706f6e656e74206973206c6172676520616e642063616e206576656e20676f0a202a20746f20603060207768696c652074686520696f2d726174696f206973207374696c6c0a202a206c617267652e2042657474657220746f206b65657020746865206d756c7469706c6965720a202a2068696768657220707265636973696f6e20616e642064726f702074686520696f2d726174696f0a202a20736d6f6f74686c7920666f72206173206c6f6e672061732077652063616e2e0a202a0a6d756c7469706c6965723a0a2020706f77657228302e35206469762865706f636820313029292c0a76616c3a0a20206d756c280a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a2020293b0a0a2f2a20392e2073666c722d626173656c696e652d696e76202a2f200a5f3a20696e762873666c722d65786368616e67652d726174652829293b011bff13109e41336ff20278186170706c69636174696f6e2f6f637465742d73747265616d".to_string())),
                add_events: vec![SgAddOrder {
                    transaction: SgTransaction {
                        id: SgBytes("0xb5d715bc74b7a7f2aac8cca544c1c95e209ed4113b82269ac3285142324bc6af".to_string()),
                        from: SgBytes("0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11".to_string()),
                        block_number: SgBigInt("37432554".to_string()),
                        timestamp: SgBigInt("1739448802".to_string()),
                    },
                }],
                trades: vec![],
                remove_events: vec![],
            }
        }

        #[traced_test]
        #[tokio::test]
        async fn test_get_orders() {
            let sg_server = MockServer::start_async().await;

            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                  "data": {
                    "orders": [
                      get_order1_json()
                    ]
                  }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2");
                then.status(200).json_body_obj(&json!({
                    "data": {
                      "orders": [
                        {
                          "id": "0x0000000000000000000000000000000000000000000000000000000000000234",
                          "orderBytes": "0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000001a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                          "orderHash": "0x0000000000000000000000000000000000000000000000000000000000002345",
                          "owner": "0x0000000000000000000000000000000000000000",
                          "outputs": [
                            {
                              "id": "0x0000000000000000000000000000000000000000",
                              "token": {
                                "id": "0x0000000000000000000000000000000000000000",
                                "address": "0x0000000000000000000000000000000000000000",
                                "name": "T1",
                                "symbol": "T1",
                                "decimals": "0"
                              },
                              "balance": F0,
                              "vaultId": "0",
                              "owner": "0x0000000000000000000000000000000000000000",
                              "ordersAsOutput": [],
                              "ordersAsInput": [],
                              "balanceChanges": [],
                              "raindex": {
                                "id": "0x0000000000000000000000000000000000000000"
                              }
                            }
                          ],
                          "inputs": [
                            {
                              "id": "0x0000000000000000000000000000000000000000",
                              "token": {
                                "id": "0x0000000000000000000000000000000000000000",
                                "address": "0x0000000000000000000000000000000000000000",
                                "name": "T2",
                                "symbol": "T2",
                                "decimals": "0"
                              },
                              "balance": F0,
                              "vaultId": "0",
                              "owner": "0x0000000000000000000000000000000000000000",
                              "ordersAsOutput": [],
                              "ordersAsInput": [],
                              "balanceChanges": [],
                              "raindex": {
                                "id": "0x0000000000000000000000000000000000000000"
                              }
                            }
                          ],
                          "active": true,
                          "addEvents": [
                            {
                              "transaction": {
                                "blockNumber": "0",
                                "timestamp": "0",
                                "id": "0x0000000000000000000000000000000000000000",
                                "from": "0x0000000000000000000000000000000000000000"
                              }
                            }
                          ],
                          "meta": null,
                          "timestampAdded": "0",
                          "raindex": {
                            "id": "0x0000000000000000000000000000000000000000"
                          },
                          "trades": [],
                          "removeEvents": []
                        }
                      ]
                    }
                  }));
            });

            let filter_args = GetOrdersFilters {
                owners: vec![],
                active: None,
                order_hash: None,
                tokens: None,
                raindex_addresses: None,
                has_positive_output_vault_balance: None,
            };
            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    // not used
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let result = raindex_client
                .get_orders(None, Some(filter_args), Some(1), None)
                .await
                .unwrap();

            assert_eq!(result.orders().len(), 2);
            assert_eq!(result.total_count(), 2);
            assert!(logs_contain("completed get_orders"));
            assert!(!logs_contain("fetched order dotrain sources"));

            let expected_order1 = RaindexOrder::try_from_sg_order(
                Arc::new(raindex_client.clone()),
                1,
                get_order1(),
                None,
            )
            .unwrap();

            let order1 = result.orders()[0].clone();
            assert_eq!(order1.chain_id, expected_order1.chain_id);
            assert_eq!(order1.id, expected_order1.id);
            assert_eq!(order1.order_bytes, expected_order1.order_bytes);
            assert_eq!(order1.order_hash, expected_order1.order_hash);
            assert_eq!(order1.owner, expected_order1.owner);
            assert_eq!(order1.outputs.len(), expected_order1.outputs.len());
            for (order1_output, expected_output) in
                order1.outputs.iter().zip(expected_order1.outputs.iter())
            {
                assert_eq!(order1_output.id(), expected_output.id());
                assert_eq!(order1_output.owner(), expected_output.owner());
                assert_eq!(order1_output.vault_id(), expected_output.vault_id());
                assert_eq!(order1_output.token().id(), expected_output.token().id());
                assert_eq!(
                    order1_output.token().address(),
                    expected_output.token().address()
                );
                assert_eq!(order1_output.token().name(), expected_output.token().name());
                assert_eq!(
                    order1_output.token().symbol(),
                    expected_output.token().symbol()
                );
                assert_eq!(
                    order1_output.token().decimals(),
                    expected_output.token().decimals()
                );
                assert_eq!(order1_output.raindex(), expected_output.raindex());
            }
            assert_eq!(order1.inputs.len(), expected_order1.inputs.len());
            for (order1_input, expected_input) in
                order1.inputs.iter().zip(expected_order1.inputs.iter())
            {
                assert_eq!(order1_input.id(), expected_input.id());
                assert_eq!(order1_input.owner(), expected_input.owner());
                assert_eq!(order1_input.vault_id(), expected_input.vault_id());
                assert_eq!(order1_input.token().id(), expected_input.token().id());
                assert_eq!(
                    order1_input.token().address(),
                    expected_input.token().address()
                );
                assert_eq!(order1_input.token().name(), expected_input.token().name());
                assert_eq!(
                    order1_input.token().symbol(),
                    expected_input.token().symbol()
                );
                assert_eq!(
                    order1_input.token().decimals(),
                    expected_input.token().decimals()
                );
                assert_eq!(order1_input.raindex(), expected_input.raindex());
            }

            assert_eq!(order1.raindex(), expected_order1.raindex());
            assert_eq!(order1.timestamp_added(), expected_order1.timestamp_added());
            assert!(order1.dotrain_source().is_none());

            let order2 = result.orders()[1].clone();
            assert_eq!(order2.chain_id, 137);
            assert_eq!(
                order2.id,
                B256::from_str(
                    "0x0000000000000000000000000000000000000000000000000000000000000234"
                )
                .unwrap()
            );
            assert_eq!(order2.order_bytes, Bytes::from_str("0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000001a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap());
            assert_eq!(
                order2.order_hash,
                B256::from_str(
                    "0x0000000000000000000000000000000000000000000000000000000000002345"
                )
                .unwrap()
            );
            assert_eq!(
                order2.owner,
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(order2.outputs.len(), 1);
            let order2_outputs = order2.outputs[0].clone();
            assert_eq!(
                order2_outputs.id(),
                Bytes::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(
                order2_outputs.owner(),
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(order2_outputs.vault_id(), U256::from(0));
            assert_eq!(
                order2_outputs.token().id(),
                "0x0000000000000000000000000000000000000000".to_string()
            );
            assert_eq!(
                order2_outputs.token().address(),
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(order2_outputs.token().name(), Some("T1".to_string()));
            assert_eq!(order2_outputs.token().symbol(), Some("T1".to_string()));
            assert_eq!(order2_outputs.token().decimals(), 0);
            assert_eq!(
                order2_outputs.raindex(),
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );

            assert_eq!(order2.inputs.len(), 1);
            let order2_inputs = order2.inputs[0].clone();
            assert_eq!(
                order2_inputs.id(),
                Bytes::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(
                order2_inputs.owner(),
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(order2_inputs.vault_id(), U256::from(0));
            assert_eq!(
                order2_inputs.token().id(),
                "0x0000000000000000000000000000000000000000".to_string()
            );
            assert_eq!(
                order2_inputs.token().address(),
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(order2_inputs.token().name(), Some("T2".to_string()));
            assert_eq!(order2_inputs.token().symbol(), Some("T2".to_string()));
            assert_eq!(order2_inputs.token().decimals(), 0);
            assert_eq!(
                order2_inputs.raindex(),
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(
                order2.raindex(),
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(order2.timestamp_added(), U256::from(0));
        }

        #[tokio::test]
        async fn test_get_order_by_hash() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [get_order1_json()]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    // not used
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let res = raindex_client
                .get_order_by_hash(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000123"),
                )
                .await
                .unwrap();

            let expected_order = RaindexOrder::try_from_sg_order(
                Arc::new(raindex_client.clone()),
                1,
                get_order1(),
                None,
            )
            .unwrap();
            assert_eq!(res.id, expected_order.id);
            assert_eq!(res.order_bytes, expected_order.order_bytes);
            assert_eq!(res.order_hash, expected_order.order_hash);
            assert_eq!(res.owner, expected_order.owner);
            assert_eq!(res.outputs.len(), 2);
            assert_eq!(res.inputs.len(), 2);

            assert_eq!(res.outputs[0].id(), expected_order.outputs[0].id());
            assert_eq!(res.inputs[0].id(), expected_order.inputs[0].id());
            assert_eq!(res.inputs[1].id(), expected_order.inputs[1].id());
            assert_eq!(res.outputs[1].id(), expected_order.outputs[1].id());

            assert_eq!(res.vaults_list().items().len(), 3);
            assert_eq!(
                res.vaults_list().items()[0].id(),
                expected_order.inputs[0].id()
            );
            assert_eq!(
                res.vaults_list().items()[1].id(),
                expected_order.outputs[0].id()
            );
            assert_eq!(
                res.vaults_list().items()[2].id(),
                expected_order.inputs[1].id()
            );
        }

        #[tokio::test]
        async fn test_get_order_by_hash_not_found() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": { "orders": [] }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    // not used
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let order_hash =
                b256!("0x0000000000000000000000000000000000000000000000000000000000000123");
            let res = raindex_client
                .get_order_by_hash(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    order_hash,
                )
                .await;

            match res {
                Err(RaindexError::OrderNotFound(address, chain_id, hash)) => {
                    assert_eq!(address, CHAIN_ID_1_RAINDEX_ADDRESS.to_string());
                    assert_eq!(chain_id, 1);
                    assert_eq!(hash, order_hash);
                }
                Err(err) => panic!("unexpected error {err:?}"),
                Ok(_) => panic!("expected error"),
            }
        }

        #[tokio::test]
        async fn test_invalid_meta() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [
                            json!({
                            "id": "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1",
                            "orderBytes": "0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000f08bcbce72f62c95dcb7c07dcb5ed26acfcfbc1100000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000005c00000000000000000000000000000000000000000000000000000000000000640392c489ef67afdc348209452c338ea5ba2b6152b936e152f610d05e1a20621a40000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae60000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000049d000000000000000000000000000000000000000000000000000000000000000f0000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000c7d713b49da0000914d696e20747261646520616d6f756e742e00000000000000000000000000008b616d6f756e742d75736564000000000000000000000000000000000000000000000000000000000000000000000000000000000000000340aad21b3b70000000000000000000000000000000000000000000000000006194049f30f7200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b1a2bc2ec500000000000000000000000000000000000000000000000000000e043da6172500008f6c6173742d74726164652d74696d65000000000000000000000000000000008d6c6173742d74726164652d696f0000000000000000000000000000000000008c696e697469616c2d74696d650000000000000000000000000000000000000000000000000000000000000000000000000000000000000006f05b59d3b200000000000000000000000000000000000000000000000000008ac7230489e80000000000000000000000020000915e36ef882941816356bc3718df868054f868ad000000000000000000000000000000000000000000000000000000000000027d0a00000024007400e0015801b401e001f40218025c080500040b20000200100001001000000b120003001000010b110004001000030b0100051305000201100001011000003d120000011000020010000003100404211200001d02000001100003031000010c1200004911000003100404001000012b12000001100003031000010c1200004a0200001a0b00090b1000060b20000700100000001000011b1200001a10000047120000001000001a1000004712000001100000011000002e12000001100005011000042e120000001000053d12000001100004001000042e1200000010000601100005001000032e120000481200011d0b020a0010000001100000011000062713000001100003031000010c12000049110000001000030010000247120000001000010b110008001000050110000700100001201200001f12000001100000011000004712000000100006001000073d120000011000002b12000000100008001000043b120000160901080b1000070b10000901100008001000013d1200001b12000001100006001000013d1200000b100009001000033a120000001000040010000248120001001000000b110008001000053d12000000100006001000042b1200000a0401011a10000001100009031000010c1200004a020000001000000110000a031000010c1200004a020000040200010110000b031000010c120000491100000803000201100009031000010c120000491100000110000a031000010c12000049110000100c01030110000d001000002e1200000110000c3e1200000010000100100001001000010010000100100001001000010010000100100001001000013d1a0000020100010210000e3611000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33",
                            "orderHash": "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
                            "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                            "outputs": [],
                            "inputs": [],
                            "raindex": {
                                "id": CHAIN_ID_1_RAINDEX_ADDRESS
                            },
                            "active": true,
                            "timestampAdded": "1739448802",
                            "meta": "0x123456",
                            "addEvents": [],
                            "trades": [],
                            "removeEvents": []
                            })
                        ]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    // not used
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let res = raindex_client
                .get_order_by_hash(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000123"),
                )
                .await;
            assert!(res.is_ok());
        }

        #[tokio::test]
        async fn test_order_detail_extended() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [get_order1_json()]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    // not used
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let res = raindex_client
                .get_order_by_hash(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000123"),
                )
                .await
                .unwrap();

            assert!(res.rainlang.is_some());
            assert_eq!(res.rainlang, Some("/* 0. calculate-io */ \nusing-words-from 0xFe2411CDa193D9E4e83A5c234C7Fd320101883aC\namt: 100,\nio: call<2>();\n\n/* 1. handle-io */ \n:call<3>(),\n:ensure(equal-to(output-vault-decrease() 100) \"must take full amount\");\n\n/* 2. get-io-ratio-now */ \nelapsed: call<4>(),\nio: saturating-sub(0.0177356 div(mul(elapsed sub(0.0177356 0.0173844)) 60));\n\n/* 3. one-shot */ \n:ensure(is-zero(get(hash(order-hash() \"has-executed\"))) \"has executed\"),\n:set(hash(order-hash() \"has-executed\") 1);\n\n/* 4. get-elapsed */ \n_: sub(now() get(hash(order-hash() \"deploy-time\")));".to_string()));
        }

        #[tokio::test]
        async fn local_db_orders_parse_ios() {
            let local_raindex = "0x0987654321098765432109876543210987654321";
            let owner = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            let token = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174";
            let inputs_payload = serde_json::to_string(&vec![json!({
                "ioIndex": 1,
                "vault": {
                    "chainId": 137,
                    "vaultId": "0x01",
                    "token": token,
                    "owner": owner,
                    "raindexAddress": local_raindex,
                    "tokenName": "USDC",
                    "tokenSymbol": "USDC",
                    "tokenDecimals": 6,
                    "balance": "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "inputOrders": null,
                    "outputOrders": null
                }
            })])
            .unwrap();
            let outputs_payload = serde_json::to_string(&vec![json!({
                "ioIndex": 0,
                "vault": {
                    "chainId": 137,
                    "vaultId": "0x02",
                    "token": token,
                    "owner": owner,
                    "raindexAddress": local_raindex,
                    "tokenName": "USDC",
                    "tokenSymbol": "USDC",
                    "tokenDecimals": 6,
                    "balance": "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "inputOrders": null,
                    "outputOrders": null
                }
            })])
            .unwrap();

            let local_order = LocalDbOrder {
                chain_id: 137,
                order_hash: b256!(
                    "0x0000000000000000000000000000000000000000000000000000000000000abc"
                ),
                owner: Address::from_str(owner).unwrap(),
                block_timestamp: 1,
                block_timestamp_removed: None,
                block_number: 1,
                raindex_address: Address::from_str(local_raindex).unwrap(),
                order_bytes: Bytes::from_str("0x01").unwrap(),
                transaction_hash: b256!(
                    "0x0000000000000000000000000000000000000000000000000000000000000010"
                ),
                inputs: Some(inputs_payload),
                outputs: Some(outputs_payload),
                trade_count: 2,
                active: true,
                meta: None,
            };

            let exec = StaticJsonExec {
                json: serde_json::to_string(&vec![local_order]).unwrap(),
            };
            let local_db = LocalDb::new(exec.clone());
            let client = new_with_local_db(
                vec![get_test_yaml(
                    "https://example/sg1",
                    "https://example/sg2",
                    "https://example/rpc1",
                    "https://example/rpc2",
                )],
                local_db.clone(),
                vec![137],
            )
            .await;

            let orders_source = LocalDbOrders::new(&local_db, ClientRef::new(client.clone()));
            let result = orders_source
                .list(Some(vec![137]), &GetOrdersFilters::default(), None, None)
                .await
                .unwrap();

            assert_eq!(result.orders().len(), 1);
            let order = &result.orders()[0];
            assert_eq!(order.inputs.len(), 1);
            assert_eq!(order.outputs.len(), 1);
            assert_eq!(order.trades_count, 2);
            assert_eq!(order.raindex, Address::from_str(local_raindex).unwrap());
        }

        #[tokio::test]
        async fn local_db_orders_bubble_deserialization_error() {
            let exec = StaticJsonExec {
                json: "not-json".to_string(),
            };
            let local_db = LocalDb::new(exec);
            let client = new_with_local_db(
                vec![get_test_yaml(
                    "https://example/sg1",
                    "https://example/sg2",
                    "https://example/rpc1",
                    "https://example/rpc2",
                )],
                local_db.clone(),
                vec![137],
            )
            .await;

            let orders_source = LocalDbOrders::new(&local_db, ClientRef::new(client.clone()));
            let err = orders_source
                .list(Some(vec![137]), &GetOrdersFilters::default(), None, None)
                .await
                .unwrap_err();
            match err {
                RaindexError::LocalDbQueryError(LocalDbQueryError::Deserialization { .. }) => {}
                other => panic!("unexpected error: {other:?}"),
            }
        }

        #[tokio::test]
        async fn get_orders_falls_back_to_subgraph_when_no_local_db() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg2");
                then.status(200).json_body_obj(&json!({
                  "data": {
                    "orders": [
                      {
                        "id": "0x0000000000000000000000000000000000000000000000000000000000000234",
                        "orderBytes": "0x01",
                        "orderHash": "0x0000000000000000000000000000000000000000000000000000000000002345",
                        "owner": "0x0000000000000000000000000000000000000000",
                        "outputs": [],
                        "inputs": [],
                        "active": true,
                        "addEvents": [
                          {
                            "transaction": {
                              "blockNumber": "0",
                              "timestamp": "0",
                              "id": "0x0000000000000000000000000000000000000000",
                              "from": "0x0000000000000000000000000000000000000000"
                            }
                          }
                        ],
                        "meta": null,
                        "timestampAdded": "0",
                        "raindex": {
                          "id": "0x0987654321098765432109876543210987654321"
                        },
                        "trades": [],
                        "removeEvents": []
                      }
                    ]
                  }
                }));
            });

            let client = RaindexClient::new(
                vec![get_test_yaml(
                    "https://example/sg1",
                    &sg_server.url("/sg2"),
                    "https://example/rpc1",
                    "https://example/rpc2",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let result = client
                .get_orders(
                    Some(ChainIds(vec![137])),
                    Some(GetOrdersFilters::default()),
                    Some(1),
                    None,
                )
                .await
                .unwrap();
            assert_eq!(result.orders().len(), 1);
            assert_eq!(
                result.orders()[0].order_hash,
                b256!("0x0000000000000000000000000000000000000000000000000000000000002345")
            );
        }

        #[tokio::test]
        async fn get_order_by_hash_hits_local_before_subgraph() {
            let local_order = LocalDbOrder {
                chain_id: 137,
                order_hash: b256!(
                    "0x0000000000000000000000000000000000000000000000000000000000000abc"
                ),
                owner: Address::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
                block_timestamp: 1,
                block_timestamp_removed: None,
                block_number: 1,
                raindex_address: Address::from_str("0x0987654321098765432109876543210987654321")
                    .unwrap(),
                order_bytes: Bytes::from_str("0x01").unwrap(),
                transaction_hash: b256!(
                    "0x0000000000000000000000000000000000000000000000000000000000000010"
                ),
                inputs: None,
                outputs: None,
                trade_count: 0,
                active: true,
                meta: None,
            };

            let exec = StaticJsonExec {
                json: serde_json::to_string(&vec![local_order]).unwrap(),
            };
            let local_db = LocalDb::new(exec);

            let client = new_with_local_db(
                vec![get_test_yaml(
                    "https://example/sg1",
                    "https://example/sg2",
                    "https://example/rpc1",
                    "https://example/rpc2",
                )],
                local_db,
                vec![137],
            )
            .await;

            let order = client
                .get_order_by_hash(
                    &RaindexIdentifier::new(
                        137,
                        Address::from_str("0x0987654321098765432109876543210987654321").unwrap(),
                    ),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000abc"),
                )
                .await
                .unwrap();

            assert_eq!(
                order.order_hash,
                b256!("0x0000000000000000000000000000000000000000000000000000000000000abc")
            );
            assert_eq!(order.chain_id, 137);
        }

        fn get_trades_json() -> Vec<Value> {
            let float50 = Float::parse("50".to_string()).unwrap().as_hex();
            let float100 = Float::parse("100".to_string()).unwrap().as_hex();
            vec![json!({
                "id": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                "timestamp": "1632000000",
                "tradeEvent": {
                    "id": "0x0000000000000000000000000000000000000000000000000000000000000001",
                    "__typename": "TakeOrder",
                    "sender": "0x0000000000000000000000000000000000000000",
                    "transaction": {
                        "id": "0x0000000000000000000000000000000000000000000000000000000000000000",
                        "from": "0x0000000000000000000000000000000000000000",
                        "timestamp": "1632000000",
                        "blockNumber": "0"
                    }
                },
                "order": {
                    "id": "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
                    "orderHash": "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
                    "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11"
                },
                "raindex": {
                    "id": CHAIN_ID_1_RAINDEX_ADDRESS
                },
                "inputVaultBalanceChange": {
                    "id": "input-change-1",
                    "__typename": "TradeVaultBalanceChange",
                    "amount": float50,
                    "newVaultBalance": float50,
                    "oldVaultBalance": "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "timestamp": "1632000000",
                    "transaction": {
                        "id": "0x0000000000000000000000000000000000000000000000000000000000000000",
                        "from": "0x0000000000000000000000000000000000000000",
                        "timestamp": "1632000000",
                        "blockNumber": "0"
                    },
                    "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                    "vault": {
                        "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                        "vaultId": "0x1234",
                        "token": {
                            "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                            "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                            "name": "Wrapped Flare",
                            "symbol": "WFLR",
                            "decimals": "18"
                        }
                    },
                    "trade": {
                        "tradeEvent": {
                            "__typename": "TradeEvent"
                        }
                    }
                },
                "outputVaultBalanceChange": {
                    "id": "output-change-1",
                    "__typename": "TradeVaultBalanceChange",
                    "amount": float100,
                    "newVaultBalance": "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "oldVaultBalance": float100,
                    "timestamp": "1632000000",
                    "transaction": {
                        "id": "0x0000000000000000000000000000000000000000000000000000000000000000",
                        "from": "0x0000000000000000000000000000000000000000",
                        "timestamp": "1632000000",
                        "blockNumber": "0"
                    },
                    "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                    "vault": {
                        "id": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                        "vaultId": "0x5678",
                        "token": {
                            "id": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                            "address": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                            "name": "Staked FLR",
                            "symbol": "sFLR",
                            "decimals": "18"
                        }
                    },
                    "trade": {
                        "tradeEvent": {
                            "__typename": "TradeEvent"
                        }
                    }
                }
            })]
        }

        #[tokio::test]
        async fn test_order_vaults_volume() {
            let sg_server = MockServer::start_async().await;

            sg_server.mock(|when, then| {
                when.path("/sg1").body_contains("orders");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [get_order1_json()]
                    }
                }));
            });

            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("trades")
                    .body_contains("\"first\":200")
                    .body_contains("\"skip\":0");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "trades": get_trades_json()
                    }
                }));
            });

            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("trades")
                    .body_contains("\"first\":200")
                    .body_contains("\"skip\":200");
                then.status(200).json_body_obj(&json!({
                    "data": { "trades": [] }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let order = raindex_client
                .get_order_by_hash(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    b256!("0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4"),
                )
                .await
                .unwrap();

            let res = order.get_vaults_volume(None, None).await.unwrap();

            assert_eq!(res.len(), 2);

            let volume1 = &res[0];
            assert_eq!(volume1.id(), U256::from_str("0x1234").unwrap());
            assert_eq!(
                volume1.token().address(),
                Address::from_str("0x1d80c49bbbcd1c0911346656b529df9e5c2f783d").unwrap()
            );
            assert_eq!(volume1.token().name(), Some("Wrapped Flare".to_string()));
            assert_eq!(volume1.token().symbol(), Some("WFLR".to_string()));
            assert_eq!(volume1.token().decimals(), 18u8);
            assert_eq!(volume1.details().formatted_total_in(), "50");
            assert_eq!(volume1.details().formatted_total_out(), "0");
            assert_eq!(volume1.details().formatted_total_vol(), "50");
            assert_eq!(volume1.details().formatted_net_vol(), "50");

            let volume2 = &res[1];
            assert_eq!(volume2.id(), U256::from_str("0x5678").unwrap());
            assert_eq!(
                volume2.token().address(),
                Address::from_str("0x12e605bc104e93b45e1ad99f9e555f659051c2bb").unwrap()
            );
            assert_eq!(volume2.token().name(), Some("Staked FLR".to_string()));
            assert_eq!(volume2.token().symbol(), Some("sFLR".to_string()));
            assert_eq!(volume2.token().decimals(), 18u8);
            assert_eq!(volume2.details().formatted_total_in(), "100");
            assert_eq!(volume2.details().formatted_total_out(), "0");
            assert_eq!(volume2.details().formatted_total_vol(), "100");
            assert_eq!(volume2.details().formatted_net_vol(), "100");
        }

        // TODO: Issue #1989
        // #[tokio::test]
        // async fn test_get_orders_with_token_filter() {
        //     let sg_server = MockServer::start_async().await;

        //     sg_server.mock(|when, then| {
        //         when.path("/sg1")
        //             .body_contains("\"token_in\":[\"0x1d80c49bbbcd1c0911346656b529df9e5c2f783d\"]");
        //         then.status(200).json_body_obj(&json!({
        //             "data": {
        //                 "orders": [get_order1_json()]
        //             }
        //         }));
        //     });
        //     sg_server.mock(|when, then| {
        //         when.path("/sg2")
        //             .body_contains("\"token_in\":[\"0x1d80c49bbbcd1c0911346656b529df9e5c2f783d\"]");
        //         then.status(200).json_body_obj(&json!({
        //             "data": {
        //                 "orders": []
        //             }
        //         }));
        //     });

        //     let raindex_client = RaindexClient::new(
        //         vec![get_test_yaml(
        //             &sg_server.url("/sg1"),
        //             &sg_server.url("/sg2"),
        //             &sg_server.url("/rpc1"),
        //             &sg_server.url("/rpc2"),
        //         )],
        //         None,
        //     )
        //     .unwrap();

        //     let filters = GetOrdersFilters {
        //         owners: vec![],
        //         active: None,
        //         order_hash: None,
        //         tokens: Some(vec![Address::from_str(
        //             "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
        //         )
        //         .unwrap()]),
        //     };

        //     let result = raindex_client
        //         .get_orders(None, Some(filters), None)
        //         .await
        //         .unwrap();

        //     assert_eq!(result.len(), 1);
        //     assert_eq!(
        //         result[0].id,
        //         Bytes::from_str(
        //             "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1"
        //         )
        //         .unwrap()
        //     );
        // }

        // TODO: Issue #1989
        // #[tokio::test]
        // async fn test_get_orders_with_multiple_token_filters() {
        //     let sg_server = MockServer::start_async().await;

        //     sg_server.mock(|when, then| {
        //         when.path("/sg1")
        //             .body_contains("\"token_in\":[\"0x1d80c49bbbcd1c0911346656b529df9e5c2f783d\",\"0x12e605bc104e93b45e1ad99f9e555f659051c2bb\"]");
        //         then.status(200).json_body_obj(&json!({
        //             "data": {
        //                 "orders": [get_order1_json()]
        //             }
        //         }));
        //     });
        //     sg_server.mock(|when, then| {
        //         when.path("/sg2");
        //         then.status(200).json_body_obj(&json!({
        //             "data": {
        //                 "orders": []
        //             }
        //         }));
        //     });

        //     let raindex_client = RaindexClient::new(
        //         vec![get_test_yaml(
        //             &sg_server.url("/sg1"),
        //             &sg_server.url("/sg2"),
        //             &sg_server.url("/rpc1"),
        //             &sg_server.url("/rpc2"),
        //         )],
        //         None,
        //     )
        //     .unwrap();

        //     let filters = GetOrdersFilters {
        //         owners: vec![],
        //         active: None,
        //         order_hash: None,
        //         tokens: Some(vec![
        //             Address::from_str("0x1d80c49bbbcd1c0911346656b529df9e5c2f783d").unwrap(),
        //             Address::from_str("0x12e605bc104e93b45e1ad99f9e555f659051c2bb").unwrap(),
        //         ]),
        //     };

        //     let result = raindex_client
        //         .get_orders(None, Some(filters), None)
        //         .await
        //         .unwrap();

        //     assert_eq!(result.len(), 1);
        // }

        #[tokio::test]
        async fn test_fetch_orders_for_pair_returns_orders() {
            let sg_server = MockServer::start_async().await;

            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [get_order1_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2");
                then.status(200).json_body_obj(&json!({
                    "data": { "orders": [] }
                }));
            });

            let client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let sell = address!("1d80c49bbbcd1c0911346656b529df9e5c2f783d");
            let buy = address!("12e605bc104e93b45e1ad99f9e555f659051c2bb");
            let result = client.fetch_orders_for_pair(1, sell, buy).await.unwrap();
            assert_eq!(result.len(), 1);
        }

        #[tokio::test]
        async fn test_fetch_orders_for_pair_no_liquidity() {
            let sg_server = MockServer::start_async().await;

            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": { "orders": [] }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2");
                then.status(200).json_body_obj(&json!({
                    "data": { "orders": [] }
                }));
            });

            let client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let sell = address!("1d80c49bbbcd1c0911346656b529df9e5c2f783d");
            let buy = address!("12e605bc104e93b45e1ad99f9e555f659051c2bb");
            let result = client.fetch_orders_for_pair(1, sell, buy).await;
            assert!(matches!(result, Err(RaindexError::NoLiquidity)));
        }
    }

    #[cfg(target_family = "wasm")]
    mod wasm_tests {
        use crate::raindex_client::order_quotes::{RaindexOrderQuote, RaindexOrderQuoteValue};
        use crate::raindex_client::take_orders::{
            estimate_take_order, TakeOrderEstimate, TakeOrdersCalldataResult,
        };
        use crate::take_orders::TakeOrdersMode;
        use alloy::primitives::{Address, Bytes};
        use rain_math_float::Float;
        use raindex_quote::Pair;
        use std::ops::Mul;
        use wasm_bindgen_test::wasm_bindgen_test;
        use wasm_bindgen_utils::prelude::{from_js_value, to_js_value};

        fn make_test_quote_value() -> RaindexOrderQuoteValue {
            let max_output = Float::parse("100".to_string()).unwrap();
            let ratio = Float::parse("2".to_string()).unwrap();
            let max_input = max_output.mul(ratio).unwrap();
            let inverse_ratio = Float::parse("0.5".to_string()).unwrap();
            RaindexOrderQuoteValue {
                max_output,
                formatted_max_output: "100".to_string(),
                max_input,
                formatted_max_input: "200".to_string(),
                ratio,
                formatted_ratio: "2".to_string(),
                inverse_ratio,
                formatted_inverse_ratio: "0.5".to_string(),
                formatted_max_output_as_percent_of_vault: None,
            }
        }

        fn make_test_quote() -> RaindexOrderQuote {
            RaindexOrderQuote {
                pair: Pair {
                    pair_name: "TOKEN_A/TOKEN_B".to_string(),
                    input_index: 0,
                    output_index: 0,
                },
                block_number: 12345678,
                data: Some(make_test_quote_value()),
                success: true,
                error: None,
                signed_context: vec![],
            }
        }

        fn make_test_calldata_result() -> TakeOrdersCalldataResult {
            use crate::raindex_client::take_orders::result::TakeOrdersInfoData;

            let raindex = Address::from([0x11u8; 20]);
            let calldata = Bytes::from(vec![0x01, 0x02, 0x03, 0x04]);
            let effective_price = Float::parse("1.5".to_string()).unwrap();
            let prices = vec![
                Float::parse("1.4".to_string()).unwrap(),
                Float::parse("1.6".to_string()).unwrap(),
            ];
            let expected_sell = Float::parse("150".to_string()).unwrap();
            let max_sell_cap = Float::parse("200".to_string()).unwrap();

            TakeOrdersCalldataResult::ready(TakeOrdersInfoData {
                raindex,
                calldata,
                effective_price,
                prices,
                expected_sell,
                max_sell_cap,
            })
        }

        #[wasm_bindgen_test]
        fn test_raindex_order_quote_wasm_serialization() {
            let quote = make_test_quote();

            let js_value = to_js_value(&quote).expect("Should serialize RaindexOrderQuote to JS");
            let roundtrip: RaindexOrderQuote =
                from_js_value(js_value).expect("Should deserialize RaindexOrderQuote from JS");

            assert_eq!(roundtrip.pair.pair_name, quote.pair.pair_name);
            assert_eq!(roundtrip.pair.input_index, quote.pair.input_index);
            assert_eq!(roundtrip.pair.output_index, quote.pair.output_index);
            assert_eq!(roundtrip.block_number, quote.block_number);
            assert_eq!(roundtrip.success, quote.success);
            assert!(roundtrip.error.is_none());

            let data = roundtrip.data.expect("Should have quote data");
            let original_data = quote.data.expect("Original should have data");
            assert!(data.max_output.eq(original_data.max_output).unwrap());
            assert!(data.ratio.eq(original_data.ratio).unwrap());
            assert!(roundtrip.signed_context.is_empty());
        }

        #[wasm_bindgen_test]
        fn test_raindex_order_quote_failed_wasm_serialization() {
            let quote = RaindexOrderQuote {
                pair: Pair {
                    pair_name: "FAIL/TEST".to_string(),
                    input_index: 1,
                    output_index: 2,
                },
                block_number: 99999,
                data: None,
                success: false,
                error: Some("Quote simulation failed".to_string()),
                signed_context: vec![],
            };

            let js_value = to_js_value(&quote).expect("Should serialize failed quote to JS");
            let roundtrip: RaindexOrderQuote =
                from_js_value(js_value).expect("Should deserialize failed quote from JS");

            assert_eq!(roundtrip.pair.pair_name, "FAIL/TEST");
            assert!(!roundtrip.success);
            assert!(roundtrip.data.is_none());
            assert_eq!(roundtrip.error, Some("Quote simulation failed".to_string()));
            assert!(roundtrip.signed_context.is_empty());
        }

        #[wasm_bindgen_test]
        fn test_take_orders_calldata_result_wasm_serialization() {
            let result = make_test_calldata_result();

            assert!(result.is_ready());
            let result_info = result.take_orders_info().unwrap();

            let js_value =
                to_js_value(&result_info).expect("Should serialize TakeOrdersInfo to JS");
            let roundtrip: crate::raindex_client::take_orders::TakeOrdersInfo =
                from_js_value(js_value).expect("Should deserialize TakeOrdersInfo from JS");

            assert_eq!(roundtrip.raindex(), result_info.raindex());
            assert_eq!(roundtrip.calldata(), result_info.calldata());
            assert!(roundtrip
                .effective_price()
                .eq(result_info.effective_price())
                .unwrap());
            assert_eq!(roundtrip.prices().len(), result_info.prices().len());
            for (rt_price, orig_price) in roundtrip.prices().iter().zip(result_info.prices().iter())
            {
                assert!(rt_price.eq(*orig_price).unwrap());
            }
            assert!(roundtrip
                .expected_sell()
                .eq(result_info.expected_sell())
                .unwrap());
            assert!(roundtrip
                .max_sell_cap()
                .eq(result_info.max_sell_cap())
                .unwrap());
        }

        #[wasm_bindgen_test]
        fn test_take_orders_mode_wasm_serialization_for_get_take_calldata() {
            let modes = vec![
                TakeOrdersMode::BuyUpTo,
                TakeOrdersMode::BuyExact,
                TakeOrdersMode::SpendUpTo,
                TakeOrdersMode::SpendExact,
            ];

            for mode in modes {
                let js_value = to_js_value(&mode).expect("Should serialize TakeOrdersMode to JS");
                let roundtrip: TakeOrdersMode =
                    from_js_value(js_value).expect("Should deserialize TakeOrdersMode from JS");
                assert_eq!(roundtrip, mode);
            }
        }

        #[wasm_bindgen_test]
        fn test_order_estimate_take_order_wasm() {
            let max_output = Float::parse("100".to_string()).unwrap();
            let ratio = Float::parse("2".to_string()).unwrap();
            let amount = Float::parse("50".to_string()).unwrap();

            let estimate = estimate_take_order(max_output, ratio, true, amount).unwrap();

            let js_value =
                to_js_value(&estimate).expect("Should serialize TakeOrderEstimate to JS");
            let roundtrip: TakeOrderEstimate =
                from_js_value(js_value).expect("Should deserialize TakeOrderEstimate from JS");

            assert!(roundtrip
                .expected_spend()
                .eq(estimate.expected_spend())
                .unwrap());
            assert!(roundtrip
                .expected_receive()
                .eq(estimate.expected_receive())
                .unwrap());
            assert_eq!(roundtrip.is_partial(), estimate.is_partial());
            assert!(!roundtrip.is_partial());

            let partial_amount = Float::parse("150".to_string()).unwrap();
            let partial_estimate =
                estimate_take_order(max_output, ratio, true, partial_amount).unwrap();

            let partial_js =
                to_js_value(&partial_estimate).expect("Should serialize partial estimate to JS");
            let partial_roundtrip: TakeOrderEstimate =
                from_js_value(partial_js).expect("Should deserialize partial estimate from JS");

            assert!(partial_roundtrip.is_partial());
            assert!(partial_roundtrip.expected_receive().eq(max_output).unwrap());

            let sell_estimate = estimate_take_order(max_output, ratio, false, amount).unwrap();

            let sell_js =
                to_js_value(&sell_estimate).expect("Should serialize sell estimate to JS");
            let sell_roundtrip: TakeOrderEstimate =
                from_js_value(sell_js).expect("Should deserialize sell estimate from JS");

            assert!(sell_roundtrip
                .expected_spend()
                .eq(sell_estimate.expected_spend())
                .unwrap());
            assert!(sell_roundtrip
                .expected_receive()
                .eq(sell_estimate.expected_receive())
                .unwrap());
        }
    }
}
