#[cfg(target_family = "wasm")]
use crate::local_db::{
    pipeline::adapters::bootstrap::BootstrapPipeline,
    query::fetch_target_watermark::{fetch_target_watermark_stmt, TargetWatermarkRow},
};
use crate::local_db::{
    query::{
        fetch_last_synced_block::{fetch_last_synced_block_stmt, SyncStatusResponse},
        LocalDbQueryError, LocalDbQueryExecutor,
    },
    LocalDbError,
};
use crate::raindex_client::local_db::{
    LocalDb, LocalDbSyncSnapshot, LocalDbSyncStatusStore, SyncReadiness,
};
use crate::{
    add_order::AddOrderArgsError, deposit::DepositError, dotrain_order::DotrainOrderError,
    meta::TryDecodeRainlangSourceError, transaction::WritableTransactionExecuteError,
    utils::amount_formatter::AmountFormatterError,
};
use alloy::{
    hex::FromHexError,
    primitives::{
        ruint::{FromUintError, ParseError},
        Address, ParseSignedError, B256,
    },
};
pub(crate) use local_db::{ClassifiedChains, LocalDbState, QuerySource};
use rain_math_float::FloatError;
use raindex_app_settings::{
    network::NetworkCfg,
    remote_networks::ParseRemoteNetworksError,
    remote_tokens::ParseRemoteTokensError,
    yaml::{
        raindex::{RaindexYaml, RaindexYamlValidation},
        YamlError, YamlParsable,
    },
};
use raindex_subgraph_client::{
    types::order_detail_traits::OrderDetailError, MultiSubgraphArgs, RaindexSubgraphClient,
    RaindexSubgraphClientError,
};
use serde::{Deserialize, Serialize};
#[cfg(target_family = "wasm")]
use std::collections::HashMap;
#[cfg(not(target_family = "wasm"))]
use std::sync::Arc;
#[cfg(target_family = "wasm")]
use std::{cell::RefCell, rc::Rc};
use std::{collections::BTreeMap, fmt, num::ParseIntError, str::FromStr};
#[cfg(target_family = "wasm")]
use wasm_bindgen_utils::prelude::js_sys::Reflect;

#[cfg(target_family = "wasm")]
pub(crate) type ClientRef = std::rc::Rc<RaindexClient>;
#[cfg(not(target_family = "wasm"))]
pub(crate) type ClientRef = std::sync::Arc<RaindexClient>;

use thiserror::Error;
use url::Url;
use wasm_bindgen_utils::{prelude::*, wasm_export};

pub mod add_orders;
pub mod local_db;
pub mod markets;
pub mod order_quotes;
pub mod orders;
pub mod orders_list;
pub mod raindex_yaml;
pub mod remove_orders;
pub mod take_orders;
pub mod trades;
pub mod transactions;
pub mod types;
pub mod vaults;
pub mod vaults_list;

pub use types::*;

/// RaindexClient provides a simplified interface for querying raindex data across
/// multiple networks with automatic configuration management.
///
/// This client abstracts away complex network-specific configurations by parsing YAML
/// configuration files that define networks, tokens, raindexes, and subgraph endpoints.
/// It enables querying raindex data either from specific chains or across all
/// configured networks with automatic fallback mechanisms.
///
/// The client handles:
/// - YAML configuration parsing and validation
/// - Network-to-subgraph URL mapping
/// - Multi-network query coordination
/// - Chain ID resolution to network configurations
///
/// ## Examples
///
/// ```javascript
/// const result = await RaindexClient.new([yamlConfig]);
/// if (result.error) {
///   console.error('Failed to create client:', result.error.readableMsg);
/// } else {
///   const client = result.value;
///   // Query orders across all networks or specific chains
/// }
///
/// // Create client with multiple YAML files for modular configuration
/// const result = await RaindexClient.new([
///   yamlConfig1,
///   yamlConfig2,
///   yamlConfig3,
/// ]);
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
#[wasm_bindgen]
pub struct RaindexClient {
    raindex_yaml: RaindexYaml,
    #[serde(skip_serializing, skip_deserializing)]
    local_db_state: LocalDbState,
}

#[cfg(target_family = "wasm")]
#[wasm_export]
impl RaindexClient {
    /// Creates a RaindexClient from YAML config, optionally setting up local DB
    /// sync automatically when the YAML declares `local-db-sync`.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// // Subgraph-only (no local-db-sync in YAML)
    /// const result = await RaindexClient.new([yamlConfig]);
    ///
    /// // With local DB (YAML has local-db-sync)
    /// const result = await RaindexClient.new([yamlConfig], undefined, {
    ///   localDb,
    ///   statusCallback: updateStatus,
    /// });
    /// ```
    #[wasm_export(
        js_name = "new",
        return_description = "Initialized client instance for further operations",
        preserve_js_class
    )]
    pub async fn new(
        #[wasm_export(
            js_name = "raindexYamls",
            param_description = "List of YAML configuration strings. \
            The YAML files must match the [raindex yaml spec](https://github.com/rainlanguage/specs/blob/main/raindex-yaml.md)
            "
        )]
        raindex_yamls: Vec<String>,
        validate: Option<bool>,
        #[wasm_export(
            js_name = "options",
            param_description = "Optional setup object with localDb and statusCallback"
        )]
        options: Option<JsValue>,
    ) -> Result<RaindexClient, RaindexError> {
        let options = LocalDbClientOptions::parse(options)?;
        let mut raindex_yaml = RaindexYaml::new(
            raindex_yamls,
            match validate {
                Some(true) => RaindexYamlValidation::full(),
                _ => RaindexYamlValidation::default(),
            },
        )?;
        raindex_yaml.fetch_remote_data().await?;

        let sync_configured_chains = LocalDbState::compute_chain_ids(&raindex_yaml);
        let sync_readiness = SyncReadiness::new();
        let sync_status_store = LocalDbSyncStatusStore::new();
        let has_syncs = !sync_configured_chains.is_empty();
        if !has_syncs {
            sync_status_store.reset();
        }

        let local_db = if has_syncs {
            let local_db = options
                .local_db
                .ok_or_else(|| RaindexError::LocalDbSetupMissing("options.localDb".to_string()))?;
            Some(LocalDb::from_js_local_db(local_db)?)
        } else {
            None
        };

        let settings = if has_syncs {
            Some(
                crate::local_db::pipeline::runner::utils::parse_runner_settings_from_yaml(
                    &raindex_yaml,
                )?,
            )
        } else {
            None
        };

        if let (Some(db), Some(settings)) = (local_db.as_ref(), settings.as_ref()) {
            initialize_local_db_readiness(db, settings, &sync_readiness, &sync_status_store)
                .await?;
        }

        let scheduler = if has_syncs {
            let (db, settings) = local_db.clone().zip(settings.clone()).ok_or_else(|| {
                RaindexError::LocalDbSetupMissing("local_db settings".to_string())
            })?;
            let handle = crate::raindex_client::local_db::pipeline::runner::scheduler::start(
                settings,
                db,
                options.status_callback,
                sync_readiness.clone(),
                sync_status_store.clone(),
                true,
            )?;
            Rc::new(RefCell::new(Some(handle)))
        } else {
            Rc::new(RefCell::new(None))
        };
        #[cfg(not(target_family = "wasm"))]
        let scheduler = Arc::new(std::sync::Mutex::new(None));

        Ok(RaindexClient {
            raindex_yaml,
            local_db_state: LocalDbState::new(
                local_db,
                scheduler,
                sync_readiness,
                sync_configured_chains,
                sync_status_store,
            ),
        })
    }
}

#[cfg(target_family = "wasm")]
#[derive(Default)]
pub struct LocalDbClientOptions {
    pub local_db: Option<JsValue>,
    pub status_callback: Option<js_sys::Function>,
}

#[cfg(target_family = "wasm")]
impl LocalDbClientOptions {
    pub fn parse(options: Option<JsValue>) -> Result<Self, RaindexError> {
        let Some(options) = options else {
            return Ok(Self::default());
        };

        let local_db = optional_field(&options, "localDb")?;
        let status_callback = optional_function_field(&options, "statusCallback")?;

        Ok(Self {
            local_db,
            status_callback,
        })
    }
}

#[cfg(target_family = "wasm")]
fn optional_field(options: &JsValue, name: &str) -> Result<Option<JsValue>, RaindexError> {
    let value = Reflect::get(options, &JsValue::from_str(name)).map_err(|e| {
        RaindexError::LocalDbQueryError(LocalDbQueryError::database(format!(
            "Failed to read options.{name}: {e:?}"
        )))
    })?;
    Ok((!value.is_undefined() && !value.is_null()).then_some(value))
}

#[cfg(target_family = "wasm")]
fn optional_function_field(
    options: &JsValue,
    name: &str,
) -> Result<Option<js_sys::Function>, RaindexError> {
    optional_field(options, name)?
        .map(|value| {
            value.dyn_into::<js_sys::Function>().map_err(|_| {
                RaindexError::LocalDbQueryError(LocalDbQueryError::database(format!(
                    "options.{name} must be a function"
                )))
            })
        })
        .transpose()
}

#[wasm_export]
impl RaindexClient {
    fn resolve_networks(
        &self,
        chain_ids: Option<Vec<u32>>,
    ) -> Result<Vec<NetworkCfg>, RaindexError> {
        match chain_ids {
            Some(ids) if !ids.is_empty() => {
                let mut networks = Vec::with_capacity(ids.len());
                for id in ids {
                    networks.push(self.raindex_yaml.get_network_by_chain_id(id)?);
                }
                Ok(networks)
            }
            Some(_) | None => {
                let all_nets = self.raindex_yaml.get_networks()?;
                let networks = all_nets.values().cloned().collect();
                Ok(networks)
            }
        }
    }

    fn get_multi_subgraph_args(
        &self,
        chain_ids: Option<Vec<u32>>,
    ) -> Result<BTreeMap<u32, Vec<MultiSubgraphArgs>>, RaindexError> {
        let networks = self.resolve_networks(chain_ids)?;
        let mut result: BTreeMap<u32, Vec<MultiSubgraphArgs>> = BTreeMap::new();
        for network in networks {
            let raindexes = self
                .raindex_yaml
                .get_raindexes_by_network_key(&network.key)?;
            for raindex in raindexes {
                result
                    .entry(network.chain_id)
                    .or_default()
                    .push(MultiSubgraphArgs {
                        url: raindex.subgraph.url.clone(),
                        name: network.label.clone().unwrap_or(network.key.clone()),
                    });
            }
        }

        if result.is_empty() {
            return Err(RaindexError::NoNetworksConfigured);
        }
        Ok(result)
    }

    #[wasm_export(skip)]
    pub fn get_raindex_subgraph_client(
        &self,
        raindex_address: Address,
    ) -> Result<RaindexSubgraphClient, RaindexError> {
        let raindex = self.raindex_yaml.get_raindex_by_address(raindex_address)?;
        Ok(RaindexSubgraphClient::new(raindex.subgraph.url.clone()))
    }

    fn get_rpc_urls_for_chain(&self, chain_id: u32) -> Result<Vec<Url>, RaindexError> {
        let network = self.raindex_yaml.get_network_by_chain_id(chain_id)?;
        Ok(network.rpcs.clone())
    }

    pub(crate) fn query_source(&self, chain_id: u32) -> QuerySource {
        self.local_db_state.query_source(chain_id)
    }

    pub(crate) fn classify_chains(
        &self,
        chain_ids: Option<Vec<u32>>,
    ) -> Result<ClassifiedChains, RaindexError> {
        let networks = self.resolve_networks(chain_ids)?;
        Ok(self.local_db_state.classify_chains(&networks))
    }

    /// Returns the latest known local DB sync status snapshot.
    ///
    /// The live status fields are maintained by the sync scheduler as it emits
    /// status updates. When a local DB handle is available, persisted
    /// `lastSyncedBlock` watermark information is added per raindex.
    #[wasm_export(
        js_name = "getLocalDbSyncSnapshot",
        return_description = "Latest local DB sync status snapshot"
    )]
    pub async fn get_local_db_sync_snapshot(&self) -> Result<LocalDbSyncSnapshot, RaindexError> {
        let mut snapshot = self.local_db_state.sync_status_store.snapshot();
        let Some(db) = self.local_db_state.local_db() else {
            return Ok(snapshot);
        };

        for raindex in &mut snapshot.raindexes {
            let stmt = fetch_last_synced_block_stmt(&raindex.raindex_id);
            match db
                .query_json_retryable::<Vec<SyncStatusResponse>>(&stmt)
                .await
            {
                Ok(rows) => {
                    if let Some(row) = rows.into_iter().next() {
                        raindex.last_synced_block = Some(row.last_synced_block);
                        raindex.updated_at = row.updated_at;
                    }
                }
                Err(err) => {
                    tracing::debug!(
                        chain_id = raindex.raindex_id.chain_id,
                        raindex = %format!("{:#x}", raindex.raindex_id.raindex_address),
                        error = %err,
                        "failed to read local DB sync watermark for snapshot"
                    );
                }
            }
        }

        Ok(snapshot)
    }
}

#[cfg(not(target_family = "wasm"))]
impl RaindexClient {
    pub async fn new(
        raindex_yamls: Vec<String>,
        validate: Option<bool>,
        db_path: Option<std::path::PathBuf>,
    ) -> Result<RaindexClient, RaindexError> {
        let mut raindex_yaml = RaindexYaml::new(
            raindex_yamls,
            match validate {
                Some(true) => RaindexYamlValidation::full(),
                _ => RaindexYamlValidation::default(),
            },
        )?;
        raindex_yaml.fetch_remote_data().await?;

        let sync_configured_chains = LocalDbState::compute_chain_ids(&raindex_yaml);
        let sync_readiness = SyncReadiness::new();
        let sync_status_store = LocalDbSyncStatusStore::new();
        let has_syncs = !sync_configured_chains.is_empty();
        if !has_syncs {
            sync_status_store.reset();
        }

        let (local_db, scheduler) = if has_syncs {
            let path =
                db_path.ok_or_else(|| RaindexError::LocalDbSetupMissing("db_path".to_string()))?;
            let settings =
                crate::local_db::pipeline::runner::utils::parse_runner_settings_from_yaml(
                    &raindex_yaml,
                )?;
            let handle = crate::raindex_client::local_db::pipeline::runner::scheduler::start(
                settings,
                path.clone(),
                sync_readiness.clone(),
                sync_status_store.clone(),
            )?;
            let executor = crate::local_db::executor::RusqliteExecutor::new(&path);
            (Some(LocalDb::new(executor)), Some(handle))
        } else {
            (None, None)
        };

        Ok(RaindexClient {
            raindex_yaml,
            local_db_state: LocalDbState::new(
                local_db,
                Arc::new(std::sync::Mutex::new(scheduler)),
                sync_readiness,
                sync_configured_chains,
                sync_status_store,
            ),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SerdeWasmBindgenErrorWrapper {
    message: String,
}

impl fmt::Display for SerdeWasmBindgenErrorWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SerdeWasmBindgenErrorWrapper {}

impl From<serde_wasm_bindgen::Error> for SerdeWasmBindgenErrorWrapper {
    fn from(error: serde_wasm_bindgen::Error) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

#[derive(Error, Debug)]
pub enum RaindexError {
    #[error("Invalid yaml configuration")]
    InvalidYamlConfig,
    #[error("{0}")]
    OracleFetchError(String),
    #[error("Chain ID not found: {0}")]
    ChainIdNotFound(u32),
    #[error("No networks configured")]
    NoNetworksConfigured,
    #[error("Subgraph not configured for chain ID: {0}")]
    SubgraphNotConfigured(String),
    #[error("Transaction {tx_hash:#x} was not indexed after {attempts} attempts")]
    TransactionIndexingTimeout { tx_hash: B256, attempts: usize },
    #[error(transparent)]
    YamlError(#[from] YamlError),
    #[error(transparent)]
    ParseRemoteNetworksError(#[from] ParseRemoteNetworksError),
    #[error(transparent)]
    ParseRemoteTokensError(#[from] ParseRemoteTokensError),
    #[error(transparent)]
    SerdeError(#[from] SerdeWasmBindgenErrorWrapper),
    #[error(transparent)]
    DotrainOrderError(Box<DotrainOrderError>),
    #[error(transparent)]
    FromHexError(#[from] FromHexError),
    #[error(transparent)]
    RaindexSubgraphClientError(#[from] RaindexSubgraphClientError),
    #[error(transparent)]
    TryDecodeRainlangSourceError(#[from] TryDecodeRainlangSourceError),
    #[error(transparent)]
    U256ParseError(#[from] ParseError),
    #[error(transparent)]
    I256ParseError(#[from] ParseSignedError),
    #[error("JavaScript error: {0}")]
    JsError(String),
    #[error("Failed to acquire read lock")]
    ReadLockError,
    #[error("Failed to acquire write lock")]
    WriteLockError,
    #[error("Zero amount")]
    ZeroAmount,
    #[error("Negative amount")]
    NegativeAmount,
    #[error(transparent)]
    WritableTransactionExecuteError(#[from] WritableTransactionExecuteError),
    #[error(transparent)]
    TransactionArgsError(#[from] crate::transaction::TransactionArgsError),
    #[error(transparent)]
    DepositArgsError(#[from] DepositError),
    #[error("Raindex not found for address: {0} on chain ID: {1}")]
    RaindexNotFound(String, u32),
    #[error("Order not found for address: {0} on chain ID: {1} with hash: {2}")]
    OrderNotFound(String, u32, B256),
    #[error("Vault not found for address: {0} on chain ID: {1} with id: {2}")]
    VaultNotFound(String, u32, String),
    #[error(transparent)]
    OrderDetailError(#[from] OrderDetailError),
    #[error(transparent)]
    AddOrderArgsError(#[from] AddOrderArgsError),
    #[error(transparent)]
    RaindexQuoteError(#[from] raindex_quote::error::Error),
    #[error("Missing subgraph {0} for order {1}")]
    SubgraphNotFound(String, String),
    #[error("Invalid vault balance change type: {0}")]
    InvalidVaultBalanceChangeType(String),
    #[error(transparent)]
    Erc20(Box<crate::erc20::Error>),
    #[error("Float error: {0}")]
    Float(#[from] FloatError),
    #[error("Failed to parse an integer: {0}")]
    ParseInt(#[from] ParseIntError),
    #[error("Failed to convert to u8: {0}")]
    TryFromUint(#[from] FromUintError<u8>),
    #[error("Missing decimals for token {0}")]
    MissingErc20Decimals(String),
    #[error(transparent)]
    AmountFormatterError(#[from] AmountFormatterError),
    #[error(transparent)]
    LocalDbError(#[from] Box<LocalDbError>),
    #[error(transparent)]
    LocalDbQueryError(#[from] LocalDbQueryError),
    #[error("Chain id: {0} is not supported for local database")]
    LocalDbUnsupportedNetwork(u32),
    #[error("YAML has local-db-sync but no {0} was provided")]
    LocalDbSetupMissing(String),
    #[error("No liquidity available for the requested token pair")]
    NoLiquidity,
    #[error("Insufficient liquidity: requested {requested}, available {available}")]
    InsufficientLiquidity {
        requested: String,
        available: String,
    },
    #[error("Sell token and buy token cannot be the same")]
    SameTokenPair,
    #[error("Amount must be positive")]
    NonPositiveAmount,
    #[error("Price cap cannot be negative")]
    NegativePriceCap,
    #[error(transparent)]
    RpcClientError(#[from] crate::rpc_client::RpcClientError),
    #[error("Preflight check failed: {0}")]
    PreflightError(String),
    #[error("Quote data is missing")]
    QuoteDataMissing,
    #[error("Invalid input index: {0}")]
    InvalidInputIndex(u32),
    #[error("Invalid output index: {0}")]
    InvalidOutputIndex(u32),
    #[error("Cannot parse metadata: {0}")]
    ParseMetaError(#[from] rain_metadata::Error),
    #[error("No metaboards configured for any chain")]
    NoMetaboardsConfigured,
    #[error("Metaboard not configured for chain ID: {0}")]
    MetaboardNotConfigured(u32),
    #[error("Metaboard subgraph error: {0}")]
    MetaboardSubgraphError(String),
    #[error("Invalid dotrain source metadata found")]
    InvalidDotrainSourceMetadata,
}

impl From<DotrainOrderError> for RaindexError {
    fn from(err: DotrainOrderError) -> Self {
        Self::DotrainOrderError(Box::new(err))
    }
}

impl From<LocalDbError> for RaindexError {
    fn from(err: LocalDbError) -> Self {
        Self::LocalDbError(Box::new(err))
    }
}

impl From<crate::erc20::Error> for RaindexError {
    fn from(err: crate::erc20::Error) -> Self {
        Self::Erc20(Box::new(err))
    }
}

impl From<serde_wasm_bindgen::Error> for RaindexError {
    fn from(err: serde_wasm_bindgen::Error) -> Self {
        RaindexError::SerdeError(err.into())
    }
}

#[cfg(target_family = "wasm")]
async fn initialize_local_db_readiness(
    db: &LocalDb,
    settings: &crate::local_db::pipeline::runner::utils::ParsedRunnerSettings,
    sync_readiness: &SyncReadiness,
    sync_status_store: &LocalDbSyncStatusStore,
) -> Result<(), RaindexError> {
    crate::raindex_client::local_db::pipeline::bootstrap::ClientBootstrapAdapter::new()
        .runner_run(
            db,
            Some(raindex_app_settings::local_db_manifest::DB_SCHEMA_VERSION),
        )
        .await?;

    sync_status_store.seed(settings);

    let mut chain_raindexes = settings
        .raindexes
        .values()
        .filter(|raindex| settings.syncs.contains_key(&raindex.network.key))
        .fold(
            HashMap::<u32, Vec<crate::local_db::RaindexIdentifier>>::new(),
            |mut chain_raindexes, raindex| {
                chain_raindexes
                    .entry(raindex.network.chain_id)
                    .or_default()
                    .push(crate::local_db::RaindexIdentifier::new(
                        raindex.network.chain_id,
                        raindex.address,
                    ));
                chain_raindexes
            },
        );

    for (chain_id, raindex_ids) in chain_raindexes.drain() {
        let mut chain_ready = true;

        for raindex_id in raindex_ids {
            let rows: Vec<TargetWatermarkRow> = db
                .query_json_retryable(&fetch_target_watermark_stmt(&raindex_id))
                .await?;

            if rows.is_empty() {
                chain_ready = false;
                break;
            }
        }

        if chain_ready {
            sync_readiness.mark_ready(chain_id);
            sync_status_store.record_chain_ready(chain_id);
        }
    }

    Ok(())
}

impl RaindexError {
    pub fn to_readable_msg(&self) -> String {
        match self {
            RaindexError::InvalidYamlConfig => {
                "The YAML configuration is invalid. Please check your configuration.".to_string()
            }
            RaindexError::ChainIdNotFound(chain_id) => format!(
                "The chain ID '{}' was not found in the configuration.",
                chain_id
            ),
            RaindexError::NoNetworksConfigured => {
                "No networks configured. Please check your configuration.".to_string()
            }
            RaindexError::SubgraphNotConfigured(chain_id) => {
                format!("No subgraph is configured for chain ID '{}'.", chain_id)
            }
            RaindexError::TransactionIndexingTimeout { tx_hash, attempts } => {
                format!(
                    "Timeout waiting for transaction {tx_hash:#x} to be indexed after {attempts} attempts."
                )
            }
            RaindexError::YamlError(err) => format!(
                "YAML configuration parsing failed: {}. Check file syntax and structure.",
                err
            ),
            RaindexError::ParseRemoteNetworksError(e) => {
                format!("Error parsing the remote networks configuration: {e}")
            }
            RaindexError::ParseRemoteTokensError(e) => {
                format!("Error parsing the remote tokens configuration: {e}")
            }
            RaindexError::SerdeError(err) => format!(
                "Data conversion failed: {}. The data format may be incompatible.",
                err
            ),
            RaindexError::DotrainOrderError(err) => format!(
                "Order configuration is invalid: {}. Please check the order parameters and format.",
                err
            ),
            RaindexError::FromHexError(err) => {
                format!(
                    "Invalid address format: {}. Please provide a valid hexadecimal address.",
                    err
                )
            }
            RaindexError::RaindexSubgraphClientError(err) => {
                format!("Failed to query subgraph: {}. Check network connection and subgraph availability.", err)
            }
            RaindexError::TryDecodeRainlangSourceError(err) => {
                format!("Failed to decode Rainlang source: {}. The source code may be corrupted or incompatible.", err)
            }
            RaindexError::U256ParseError(err) => {
                format!(
                    "Invalid number format: {}. Please provide a valid numeric value.",
                    err
                )
            }
            RaindexError::I256ParseError(err) => {
                format!(
                    "Invalid number format: {}. Please provide a valid numeric value.",
                    err
                )
            }
            RaindexError::JsError(err) => format!("JavaScript error: {}", err),
            RaindexError::ReadLockError => {
                "Failed to read the YAML configuration due to a lock error".to_string()
            }
            RaindexError::WriteLockError => {
                "Failed to modify the YAML configuration due to a lock error".to_string()
            }
            RaindexError::ZeroAmount => "Amount cannot be zero".to_string(),
            RaindexError::NegativeAmount => "Amount cannot be negative".to_string(),
            RaindexError::WritableTransactionExecuteError(err) => {
                format!("Failed to execute transaction: {}", err)
            }
            RaindexError::TransactionArgsError(err) => {
                format!("Invalid transaction arguments: {}", err)
            }
            RaindexError::DepositArgsError(err) => {
                format!("Failed to create deposit arguments: {}", err)
            }
            RaindexError::RaindexNotFound(address, chain_id) => {
                format!(
                    "Raindex not found for address: {} on chain ID: {}",
                    address, chain_id
                )
            }
            RaindexError::OrderNotFound(address, chain_id, order_hash) => {
                format!(
                    "Order not found for address: {} on chain ID: {} with hash: {}",
                    address, chain_id, order_hash
                )
            }
            RaindexError::VaultNotFound(address, chain_id, vault_id) => format!(
                "Vault not found for address: {} on chain ID: {} with id: {}",
                address, chain_id, vault_id
            ),
            RaindexError::OrderDetailError(err) => {
                format!("Failed to decode order detail: {}", err)
            }
            RaindexError::AddOrderArgsError(e) => {
                format!("Failed to prepare the add order calldata: {}", e)
            }
            RaindexError::RaindexQuoteError(err) => {
                format!("Failed to get order quote: {}", err)
            }
            RaindexError::SubgraphNotFound(subgraph, order) => {
                format!(
                    "Subgraph with name '{}' not found for the order with hash '{}'",
                    subgraph, order
                )
            }
            RaindexError::InvalidVaultBalanceChangeType(typ) => {
                format!("Invalid vault balance change type: {}", typ)
            }
            RaindexError::Erc20(err) => format!("Failed to get ERC20 info: {err}"),
            RaindexError::Float(err) => format!("Float error: {err}"),
            RaindexError::ParseInt(err) => format!("Failed to parse an integer: {err}"),
            RaindexError::TryFromUint(err) => format!("Failed to convert to u8: {err}"),
            RaindexError::MissingErc20Decimals(token) => {
                format!("Missing decimal information for the token address: {token}")
            }
            RaindexError::AmountFormatterError(err) => format!("Amount formatter error: {err}"),
            RaindexError::LocalDbError(err) => {
                format!("There was an error with the local database: {err}")
            }
            RaindexError::LocalDbQueryError(err) => {
                format!("There was an error querying the local database: {err}")
            }
            RaindexError::LocalDbUnsupportedNetwork(chain_id) => {
                format!("The chain ID: {chain_id} is not supported for local database operations.")
            }
            RaindexError::LocalDbSetupMissing(field) => {
                format!("YAML has local-db-sync configured but no {field} was provided.")
            }
            RaindexError::NoLiquidity => {
                "No liquidity available for the requested token pair".to_string()
            }
            RaindexError::InsufficientLiquidity {
                requested,
                available,
            } => {
                format!(
                    "Insufficient liquidity: requested {}, but only {} available",
                    requested, available
                )
            }
            RaindexError::SameTokenPair => {
                "Sell token and buy token cannot be the same".to_string()
            }
            RaindexError::NonPositiveAmount => "Amount must be positive".to_string(),
            RaindexError::NegativePriceCap => "Price cap cannot be negative".to_string(),
            RaindexError::RpcClientError(err) => format!("RPC client error: {}", err),
            RaindexError::PreflightError(err) => {
                format!("Preflight check failed: {err}")
            }
            RaindexError::QuoteDataMissing => {
                "Quote data is missing. Please ensure the quote was successful.".to_string()
            }
            RaindexError::InvalidInputIndex(index) => {
                format!(
                    "Invalid input index: {}. The order does not have an input at this index.",
                    index
                )
            }
            RaindexError::InvalidOutputIndex(index) => {
                format!(
                    "Invalid output index: {}. The order does not have an output at this index.",
                    index
                )
            }
            RaindexError::ParseMetaError(err) => format!("Cannot parse metadata: {err}"),
            RaindexError::NoMetaboardsConfigured => {
                "No metaboards configured for any chain. Please check your configuration."
                    .to_string()
            }
            RaindexError::MetaboardNotConfigured(chain_id) => {
                format!("Metaboard is not configured for chain ID: {chain_id}")
            }
            RaindexError::MetaboardSubgraphError(err) => {
                format!("Failed to query metaboard subgraph: {err}")
            }
            RaindexError::InvalidDotrainSourceMetadata => {
                "Found metadata but it could not be parsed as valid dotrain source".to_string()
            }
            RaindexError::OracleFetchError(err) => err.clone(),
        }
    }
}

impl From<RaindexError> for JsValue {
    fn from(value: RaindexError) -> Self {
        JsError::new(&value.to_string()).into()
    }
}

impl From<RaindexError> for WasmEncodedError {
    fn from(value: RaindexError) -> Self {
        WasmEncodedError {
            msg: value.to_string(),
            readable_msg: value.to_readable_msg(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use raindex_app_settings::spec_version::SpecVersion;

    #[cfg(not(target_family = "wasm"))]
    pub const CHAIN_ID_1_RAINDEX_ADDRESS: &str = "0x1234567890123456789012345678901234567890";
    pub fn get_test_yaml(subgraph1: &str, subgraph2: &str, rpc1: &str, rpc2: &str) -> String {
        format!(
            r#"
version: {spec_version}
networks:
    mainnet:
        rpcs:
            - {rpc1}
        chain-id: 1
        label: Ethereum Mainnet
        network-id: 1
        currency: ETH
    polygon:
        rpcs:
            - {rpc2}
        chain-id: 137
        label: Polygon Mainnet
        network-id: 137
        currency: MATIC
subgraphs:
    mainnet: {subgraph1}
    polygon: {subgraph2}
metaboards:
    mainnet: https://api.thegraph.com/subgraphs/name/xyz
    polygon: https://api.thegraph.com/subgraphs/name/polygon
raindexes:
    mainnet-raindex:
        address: 0x1234567890123456789012345678901234567890
        network: mainnet
        subgraph: mainnet
        local-db-remote: remote
        label: Primary Raindex
        deployment-block: 12345
    polygon-raindex:
        address: 0x0987654321098765432109876543210987654321
        network: polygon
        subgraph: polygon
        local-db-remote: remote
        deployment-block: 12345
        label: Polygon Raindex
tokens:
    weth:
        network: mainnet
        address: 0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2
        decimals: 18
        label: Wrapped Ether
        symbol: WETH
    usdc:
        network: polygon
        address: 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174
        decimals: 6
        label: USD Coin
        symbol: USDC
rainlangs:
    mainnet-rainlang:
        address: 0xF14E09601A47552De6aBd3A0B165607FaFd2B5Ba
        network: mainnet
accounts:
    alice: 0x742d35Cc6634C0532925a3b8D4Fd2d3dB2d4D7fA
    bob: 0x8ba1f109551bD432803012645aac136c0c8D2e80
    charlie: 0x95222290DD7278Aa3Ddd389Cc1E1d165CC4BAfe5
"#,
            spec_version = SpecVersion::current()
        )
    }

    #[cfg(not(target_family = "wasm"))]
    pub async fn new_with_local_db(
        yamls: Vec<String>,
        local_db: super::local_db::LocalDb,
        chain_ids: Vec<u32>,
    ) -> RaindexClient {
        let mut client = RaindexClient::new(yamls, None, None)
            .await
            .expect("test yaml should be valid");
        client.local_db_state.db = Arc::new(std::sync::Mutex::new(Some(local_db)));
        for &id in &chain_ids {
            client.local_db_state.sync_readiness.mark_ready(id);
            client.local_db_state.sync_configured_chains.insert(id);
        }
        client
    }

    #[cfg(target_family = "wasm")]
    pub fn local_db_object_from_query_callback(query_callback: js_sys::Function) -> JsValue {
        let local_db = js_sys::Object::new();
        js_sys::Reflect::set(&local_db, &JsValue::from_str("query"), &query_callback).unwrap();
        js_sys::Reflect::set(
            &local_db,
            &JsValue::from_str("wipeAndRecreate"),
            &js_sys::Function::new_no_args(
                "return Promise.resolve({ value: undefined, error: null });",
            ),
        )
        .unwrap();
        js_sys::Reflect::set(
            &local_db,
            &JsValue::from_str("transaction"),
            &js_sys::Function::new_no_args("return Promise.resolve({ value: '', error: null });"),
        )
        .unwrap();
        for name in [
            "beginSqlDumpImport",
            "appendSqlDumpChunk",
            "finishSqlDumpImport",
            "cancelSqlDumpImport",
        ] {
            js_sys::Reflect::set(
                &local_db,
                &JsValue::from_str(name),
                &js_sys::Function::new_no_args(
                    "return Promise.resolve({ value: '', error: null });",
                ),
            )
            .unwrap();
        }
        local_db.into()
    }

    #[cfg(target_family = "wasm")]
    pub fn new_test_client_with_local_db(
        yamls: Vec<String>,
        local_db: JsValue,
        chain_ids: Vec<u32>,
    ) -> RaindexClient {
        let raindex_yaml = RaindexYaml::new(yamls, RaindexYamlValidation::default())
            .expect("test yaml should be valid");
        let sync_readiness = SyncReadiness::new();
        let mut db_chain_ids = std::collections::HashSet::new();
        for &id in &chain_ids {
            sync_readiness.mark_ready(id);
            db_chain_ids.insert(id);
        }
        RaindexClient {
            raindex_yaml,
            local_db_state: LocalDbState::new(
                Some(
                    super::local_db::LocalDb::from_js_local_db(local_db)
                        .expect("valid test local DB object"),
                ),
                Rc::new(RefCell::new(None)),
                sync_readiness,
                db_chain_ids,
                LocalDbSyncStatusStore::new(),
            ),
        }
    }

    #[cfg(not(target_family = "wasm"))]
    mod native_tests {
        use super::*;
        use crate::local_db::pipeline::runner::utils::parse_runner_settings;
        use crate::local_db::query::{FromDbJson, SqlStatement, SqlStatementBatch};
        use crate::raindex_client::local_db::LocalDbStatus;
        use httpmock::MockServer;
        use std::str::FromStr;

        #[derive(Clone)]
        struct SnapshotDbExec {
            rows: Vec<SyncStatusResponse>,
        }

        #[async_trait::async_trait]
        impl crate::local_db::query::LocalDbQueryExecutor for SnapshotDbExec {
            async fn execute_batch(&self, _: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
                Ok(())
            }

            async fn query_json<T>(&self, _: &SqlStatement) -> Result<T, LocalDbQueryError>
            where
                T: FromDbJson,
            {
                let value = serde_json::to_value(&self.rows)
                    .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))?;
                serde_json::from_value(value)
                    .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))
            }

            async fn query_text(&self, _: &SqlStatement) -> Result<String, LocalDbQueryError> {
                Ok(String::new())
            }

            async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
                Ok(())
            }
        }

        fn local_db_snapshot_yaml() -> String {
            format!(
                r#"
version: {spec_version}
networks:
    arbitrum:
        rpcs:
            - https://arb.example/rpc
        chain-id: 42161
        label: Arbitrum
        network-id: 42161
        currency: ETH
subgraphs:
    arbitrum: https://arb.example/subgraph
local-db-remotes:
    remote: https://remote.example/manifest
local-db-sync:
    arbitrum:
        batch-size: 10
        max-concurrent-batches: 2
        retry-attempts: 1
        retry-delay-ms: 1
        rate-limit-delay-ms: 1
        finality-depth: 12
        bootstrap-block-threshold: 100
        sync-interval-ms: 5000
raindexes:
    arbitrum-raindex:
        address: 0x2f209e5b67A33B8fE96E28f24628dF6Da301c8eB
        network: arbitrum
        subgraph: arbitrum
        local-db-remote: remote
        deployment-block: 1
"#,
                spec_version = SpecVersion::current()
            )
        }

        fn client_with_snapshot_db(
            yaml: String,
            rows: Vec<SyncStatusResponse>,
            sync_status_store: LocalDbSyncStatusStore,
        ) -> RaindexClient {
            let raindex_yaml = RaindexYaml::new(vec![yaml], RaindexYamlValidation::default())
                .expect("test yaml should parse");
            let sync_readiness = SyncReadiness::new();
            sync_readiness.mark_ready(42161);
            let mut configured_chains = std::collections::HashSet::new();
            configured_chains.insert(42161);
            RaindexClient {
                raindex_yaml,
                local_db_state: LocalDbState::new(
                    Some(LocalDb::new(SnapshotDbExec { rows })),
                    Arc::new(std::sync::Mutex::new(None)),
                    sync_readiness,
                    configured_chains,
                    sync_status_store,
                ),
            }
        }

        #[tokio::test]
        async fn test_create_fetches_remote_networks() {
            let server = MockServer::start_async().await;
            let response = r#"[
                {
                    "name": "Remote Network",
                    "chain": "remote-network",
                    "chainId": 999,
                    "rpc": ["http://localhost:8085/rpc-url"],
                    "networkId": 999,
                    "nativeCurrency": {
                        "name": "Remote",
                        "symbol": "REM",
                        "decimals": 18
                    },
                    "infoURL": "http://localhost:8085/info-url",
                    "shortName": "remote-network"
                }
            ]"#;
            server
                .mock_async(|when, then| {
                    when.method("GET").path("/");
                    then.status(200)
                        .header("content-type", "application/json")
                        .body(response);
                })
                .await;

            let yaml = format!(
                r#"
version: {spec_version}
using-networks-from:
    remote-source:
        url: {url}
        format: chainid
"#,
                spec_version = SpecVersion::current(),
                url = server.base_url()
            );

            let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

            let networks = client.get_all_networks().unwrap();
            assert!(networks.contains_key("remote-network"));
            let network = networks.get("remote-network").unwrap();
            assert_eq!(network.chain_id, 999);
            assert_eq!(network.key, "remote-network");
            assert_eq!(
                network.rpcs,
                vec![Url::parse("http://localhost:8085/rpc-url").unwrap()]
            );
        }

        #[tokio::test]
        async fn test_create_fetches_remote_tokens() {
            let server = MockServer::start_async().await;
            let response = r#"{
                "name": "Remote Tokens",
                "timestamp": "2021-01-01T00:00:00.000Z",
                "keywords": [],
                "version": {"major": 1, "minor": 0, "patch": 0},
                "tokens": [
                    {
                        "chainId": 123,
                        "address": "0x0000000000000000000000000000000000000001",
                        "name": "RemoteToken",
                        "symbol": "REM",
                        "decimals": 18
                    }
                ],
                "logoURI": "http://localhost.com"
            }"#;
            server
                .mock_async(|when, then| {
                    when.method("GET").path("/");
                    then.status(200)
                        .header("content-type", "application/json")
                        .body(response);
                })
                .await;

            let yaml = format!(
                r#"
version: {spec_version}
networks:
    test-network:
        rpcs:
            - http://localhost:8085/rpc
        chain-id: 123
using-tokens-from:
    - {url}
"#,
                spec_version = SpecVersion::current(),
                url = server.base_url()
            );

            let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

            let tokens = client.raindex_yaml.get_tokens().unwrap();
            let expected_key =
                "test-network-RemoteToken-0x0000000000000000000000000000000000000001";
            assert!(tokens.contains_key(expected_key));
            let token = tokens.get(expected_key).unwrap();
            assert_eq!(token.key, expected_key);
            assert_eq!(
                token.address,
                Address::from_str("0x0000000000000000000000000000000000000001").unwrap()
            );
            assert_eq!(token.network.chain_id, 123);
            assert_eq!(token.network.key, "test-network");
            assert_eq!(token.decimals, Some(18));
        }

        #[tokio::test]
        async fn get_local_db_sync_snapshot_returns_not_configured_without_syncs() {
            let client = RaindexClient::new(
                vec![get_test_yaml(
                    "https://mainnet.example/subgraph",
                    "https://polygon.example/subgraph",
                    "https://mainnet.example/rpc",
                    "https://polygon.example/rpc",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let snapshot = client.get_local_db_sync_snapshot().await.unwrap();

            assert!(!snapshot.configured);
            assert!(snapshot.healthy);
            assert_eq!(snapshot.status, LocalDbStatus::Active);
            assert!(snapshot.networks.is_empty());
            assert!(snapshot.raindexes.is_empty());
        }

        #[tokio::test]
        async fn get_local_db_sync_snapshot_includes_config_and_db_watermark() {
            let yaml = local_db_snapshot_yaml();
            let settings = parse_runner_settings(&yaml).expect("settings should parse");
            let sync_status_store = LocalDbSyncStatusStore::new();
            sync_status_store.seed(&settings);

            let client = client_with_snapshot_db(
                yaml,
                vec![SyncStatusResponse {
                    chain_id: 42161,
                    raindex_address: "0x2f209e5b67a33b8fe96e28f24628df6da301c8eb".to_string(),
                    last_synced_block: 123_456,
                    updated_at: Some("2026-05-01 12:00:00".to_string()),
                }],
                sync_status_store,
            );

            let snapshot = client.get_local_db_sync_snapshot().await.unwrap();

            assert!(snapshot.configured);
            assert_eq!(snapshot.networks.len(), 1);
            assert_eq!(snapshot.networks[0].chain_id, 42161);
            assert_eq!(
                snapshot.networks[0].network_key,
                Some("arbitrum".to_string())
            );
            assert_eq!(snapshot.networks[0].raindex_count, 1);
            assert_eq!(snapshot.raindexes.len(), 1);
            assert_eq!(
                snapshot.raindexes[0].raindex_key,
                Some("arbitrum-raindex".to_string())
            );
            assert_eq!(snapshot.raindexes[0].last_synced_block, Some(123_456));
            assert_eq!(
                snapshot.raindexes[0].updated_at,
                Some("2026-05-01 12:00:00".to_string())
            );
        }
    }

    #[cfg(target_family = "wasm")]
    pub fn get_local_db_test_yaml() -> String {
        format!(
            r#"
version: {spec_version}
networks:
    arbitrum:
        rpcs:
            - https://arb1.example
        chain-id: 42161
        label: Arbitrum
        network-id: 42161
        currency: ETH
subgraphs:
    arbitrum: https://arb.subgraph
metaboards:
    arbitrum: https://arb.metaboard
raindexes:
    arbitrum-raindex:
        address: 0x2f209e5b67A33B8fE96E28f24628dF6Da301c8eB
        network: arbitrum
        subgraph: arbitrum
        local-db-remote: remote
        deployment-block: 1
tokens:
    tokena:
        network: arbitrum
        address: 0x00000000000000000000000000000000000000aa
        decimals: 18
        label: Token A
        symbol: TKNA
    tokenb:
        network: arbitrum
        address: 0x00000000000000000000000000000000000000bb
        decimals: 6
        label: Token B
        symbol: TKNB
rainlangs:
    arb-rainlang:
        address: 0x1111111111111111111111111111111111111111
        network: arbitrum
accounts:
    test: 0x2222222222222222222222222222222222222222
"#,
            spec_version = SpecVersion::current()
        )
    }

    #[cfg(target_family = "wasm")]
    mod wasm_tests {
        use super::*;
        use raindex_app_settings::yaml::YamlError;
        use url::Url;
        use wasm_bindgen_test::wasm_bindgen_test;

        fn get_invalid_yaml() -> String {
            format!(
                r#"
    version: {spec_version}
    networks:
        mainnet:
            rpcs:
                - https://mainnet.infura.io
            chain-id: 1
    raindexes:
        invalid-raindex:
            address: 0x1234567890123456789012345678901234567890
            network: nonexistent-network
            subgraph: nonexistent-subgraph
    "#,
                spec_version = SpecVersion::current()
            )
        }

        fn local_db_sync_yaml() -> String {
            format!(
                r#"
version: {spec_version}
networks:
    arbitrum:
        rpcs:
            - https://arb.example/rpc
        chain-id: 42161
        label: Arbitrum
        network-id: 42161
        currency: ETH
subgraphs:
    arbitrum: https://arb.example/subgraph
local-db-remotes:
    remote: https://remote.example/manifest
local-db-sync:
    arbitrum:
        batch-size: 10
        max-concurrent-batches: 2
        retry-attempts: 1
        retry-delay-ms: 1
        rate-limit-delay-ms: 1
        finality-depth: 12
        bootstrap-block-threshold: 100
        sync-interval-ms: 5000
raindexes:
    arbitrum-raindex:
        address: 0x2f209e5b67A33B8fE96E28f24628dF6Da301c8eB
        network: arbitrum
        subgraph: arbitrum
        local-db-remote: remote
        deployment-block: 1
"#,
                spec_version = SpecVersion::current()
            )
        }

        fn local_db_options(include_transaction: bool) -> JsValue {
            let query = js_sys::Function::new_with_args(
                "sql",
                r#"
                var value = '[]';
                if (sql && sql.toLowerCase().includes('quick_check')) {
                    value = '[{"quick_check":"ok"}]';
                }
                return Promise.resolve({ value: value, error: null });
                "#,
            );
            let local_db = js_sys::Object::new();
            js_sys::Reflect::set(&local_db, &JsValue::from_str("query"), &query).unwrap();
            js_sys::Reflect::set(
                &local_db,
                &JsValue::from_str("wipeAndRecreate"),
                &js_sys::Function::new_no_args(
                    "return Promise.resolve({ value: undefined, error: null });",
                ),
            )
            .unwrap();
            if include_transaction {
                js_sys::Reflect::set(
                    &local_db,
                    &JsValue::from_str("transaction"),
                    &js_sys::Function::new_no_args(
                        "return Promise.resolve({ value: '', error: null });",
                    ),
                )
                .unwrap();
            }
            for name in [
                "beginSqlDumpImport",
                "appendSqlDumpChunk",
                "finishSqlDumpImport",
                "cancelSqlDumpImport",
            ] {
                js_sys::Reflect::set(
                    &local_db,
                    &JsValue::from_str(name),
                    &js_sys::Function::new_no_args(
                        "return Promise.resolve({ value: '', error: null });",
                    ),
                )
                .unwrap();
            }

            let options = js_sys::Object::new();
            js_sys::Reflect::set(&options, &JsValue::from_str("localDb"), &local_db).unwrap();
            options.into()
        }

        #[wasm_bindgen_test]
        async fn test_raindex_client_new_success() {
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
            assert!(!client.raindex_yaml.documents.is_empty());
        }

        #[wasm_bindgen_test]
        async fn test_raindex_client_new_invalid_yaml() {
            let err = RaindexClient::new(vec![get_invalid_yaml()], Some(true), None)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                RaindexError::YamlError(YamlError::Field { .. })
            ));
            assert!(err
                .to_readable_msg()
                .contains("YAML configuration parsing failed"));
        }

        #[wasm_bindgen_test]
        async fn test_raindex_client_new_empty_yaml() {
            let err = RaindexClient::new(vec!["".to_string()], None, None)
                .await
                .unwrap_err();
            assert!(matches!(err, RaindexError::YamlError(YamlError::EmptyFile)));
        }

        #[wasm_bindgen_test]
        async fn test_raindex_client_new_with_local_db_options() {
            let client = RaindexClient::new(
                vec![local_db_sync_yaml()],
                None,
                Some(local_db_options(true)),
            )
            .await
            .unwrap();

            assert!(client.local_db_state.local_db().is_some());
        }

        #[wasm_bindgen_test]
        async fn test_raindex_client_new_requires_transaction_method() {
            let err = RaindexClient::new(
                vec![local_db_sync_yaml()],
                None,
                Some(local_db_options(false)),
            )
            .await
            .unwrap_err();

            assert!(err
                .to_string()
                .contains("localDb.transaction must be a function"));
        }

        #[wasm_bindgen_test]
        async fn test_get_multi_subgraph_args_single_chain() {
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

            let args = client.get_multi_subgraph_args(Some(vec![1])).unwrap();
            assert_eq!(args.len(), 1);
            assert_eq!(
                args.get(&1).unwrap()[0].url,
                Url::parse("http://localhost:3000/sg1").unwrap()
            );
            assert_eq!(args.get(&1).unwrap()[0].name, "Ethereum Mainnet");
        }

        #[wasm_bindgen_test]
        async fn test_get_multi_subgraph_args_all_chains() {
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

            let args = client.get_multi_subgraph_args(None).unwrap();
            assert_eq!(args.len(), 2);

            let urls: Vec<&str> = args.iter().map(|(_, arg)| arg[0].url.as_str()).collect();
            assert!(urls.contains(&"http://localhost:3000/sg1"));
            assert!(urls.contains(&"http://localhost:3000/sg2"));

            let names: Vec<&str> = args.iter().map(|(_, arg)| arg[0].name.as_str()).collect();
            assert!(names.contains(&"Ethereum Mainnet"));
            assert!(names.contains(&"Polygon Mainnet"));
        }

        #[wasm_bindgen_test]
        async fn test_get_multi_subgraph_args_empty_chain_ids_defaults_to_all() {
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

            let args = client.get_multi_subgraph_args(Some(vec![])).unwrap();
            assert_eq!(args.len(), 2);

            let urls: Vec<&str> = args.iter().map(|(_, arg)| arg[0].url.as_str()).collect();
            assert!(urls.contains(&"http://localhost:3000/sg1"));
            assert!(urls.contains(&"http://localhost:3000/sg2"));
        }

        #[wasm_bindgen_test]
        async fn test_get_multi_subgraph_args_multiple_chains() {
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

            let args = client.get_multi_subgraph_args(Some(vec![1, 137])).unwrap();
            assert_eq!(args.len(), 2);

            let args1 = args.get(&1).unwrap();
            assert_eq!(args1.len(), 1);
            assert_eq!(
                args1[0].url,
                Url::parse("http://localhost:3000/sg1").unwrap()
            );
            assert_eq!(args1[0].name, "Ethereum Mainnet");

            let args2 = args.get(&137).unwrap();
            assert_eq!(args2.len(), 1);
            assert_eq!(
                args2[0].url,
                Url::parse("http://localhost:3000/sg2").unwrap()
            );
            assert_eq!(args2[0].name, "Polygon Mainnet");
        }

        #[wasm_bindgen_test]
        async fn test_get_multi_subgraph_args_invalid_chain() {
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

            let err = client.get_multi_subgraph_args(Some(vec![999])).unwrap_err();
            assert!(
                matches!(err, RaindexError::YamlError(YamlError::NotFound(ref msg)) if msg.contains("network with chain-id: 999"))
            );
        }

        #[wasm_bindgen_test]
        async fn test_get_multi_subgraph_args_no_networks() {
            let yaml = format!(
                r#"
    version: {spec_version}
    networks:
        isolated:
            rpcs:
                - https://isolated.rpc
            chain-id: 999
        some-network:
            rpcs:
                - https://some-network.rpc
            chain-id: 1000
    subgraphs:
        test: https://test.subgraph
    metaboards:
        test: https://test.metaboard
    tokens:
        test-token:
            network: isolated
            address: 0x1111111111111111111111111111111111111111
            decimals: 18
    rainlangs:
        test-rainlang:
            address: 0x2222222222222222222222222222222222222222
            network: isolated
    raindexes:
        test-raindex:
            address: 0x1111111111111111111111111111111111111111
            network: some-network
            subgraph: test
            local-db-remote: remote
            label: Test Raindex
            deployment-block: 12345
    "#,
                spec_version = SpecVersion::current()
            );

            let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

            let err = client.get_multi_subgraph_args(None).unwrap_err();
            assert!(matches!(
                err,
                RaindexError::YamlError(YamlError::NotFound(ref msg)) if msg.contains("raindex with network key: isolated")
            ));
            assert!(err
                .to_readable_msg()
                .contains("raindex with network key: isolated not found"));
        }
    }
}
