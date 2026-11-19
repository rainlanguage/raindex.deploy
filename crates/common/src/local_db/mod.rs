pub mod address_collectors;
pub mod decode;
#[cfg(not(target_family = "wasm"))]
pub mod executor;
pub mod export;
pub mod fetch;
#[cfg(not(target_family = "wasm"))]
pub mod functions;
pub mod insert;
pub mod pipeline;
pub mod query;
pub mod token_fetch;

use crate::erc20::Error as TokenError;
use crate::rpc_client::RpcClientError;
use alloy::primitives::ruint::ParseError;
use alloy::primitives::{hex::FromHexError, Address};
use alloy::rpc::types::FilterBlockError;
use decode::DecodeError;
pub use fetch::{FetchConfig, FetchConfigError};
use insert::InsertError;
use query::{LocalDbQueryError, SqlBuildError};
use raindex_app_settings::remote::manifest::FetchManifestError;
use raindex_app_settings::yaml::YamlError;
use serde::{Deserialize, Serialize};
use std::array::TryFromSliceError;
use std::num::ParseIntError;
use strict_yaml_rust::ScanError;
use tokio::task::JoinError;
#[cfg(target_family = "wasm")]
use wasm_bindgen_utils::{impl_wasm_traits, prelude::*};

#[derive(Debug, thiserror::Error)]
pub enum LocalDbError {
    #[error("{0}")]
    CustomError(String),

    #[error(transparent)]
    SettingsYaml(#[from] YamlError),

    #[error(transparent)]
    YamlScan(#[from] ScanError),

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("RPC error: {0}")]
    Rpc(#[from] RpcClientError),

    #[error("JSON parsing failed")]
    JsonParse(#[from] serde_json::Error),

    #[error("Missing field: {field}")]
    MissingField { field: String },

    #[error("Missing local-db sync config for network '{network}'")]
    MissingLocalDbSyncForNetwork { network: String },

    #[error("Missing local-db remote for raindex '{raindex_key}'")]
    MissingLocalDbRemote { raindex_key: String },

    #[error("Invalid block number '{value}'")]
    InvalidBlockNumber {
        value: String,
        #[source]
        source: ParseError,
    },
    #[error("Invalid block number '{value}'")]
    InvalidBlockNumberString {
        value: String,
        #[source]
        source: ParseIntError,
    },
    #[error("Block {block_number} not found when fetching block hash")]
    BlockHashNotFound { block_number: u64 },

    #[error("Events is not in expected array format")]
    InvalidEventsFormat,

    #[error("Network request timed out")]
    Timeout,

    #[error("Configuration error: {message}")]
    Config { message: String },

    #[error("Missing runner target for chain {chain_id} raindex {raindex_address}")]
    MissingRunnerTarget {
        chain_id: u32,
        raindex_address: Address,
    },

    #[error(
        "Network '{network_key}' has mismatched chain ids (expected {expected}, found {found})"
    )]
    RunnerNetworkChainIdMismatch {
        network_key: String,
        expected: u32,
        found: u32,
    },

    #[error("Failed to build dump url '{url}'")]
    DumpUrlConstructionFailed {
        url: String,
        #[source]
        source: url::ParseError,
    },

    #[error("Failed to fetch token metadata for {address} after {attempts} attempts")]
    TokenMetadataFetchFailed {
        address: Address,
        attempts: usize,
        #[source]
        source: Box<TokenError>,
    },

    #[error("Failed to check required tables")]
    TableCheckFailed(#[source] LocalDbQueryError),

    #[error("Failed to read sync status")]
    SyncStatusReadFailed(#[source] LocalDbQueryError),

    #[error("Failed to fetch events")]
    FetchEventsFailed(#[source] Box<LocalDbError>),

    #[error("Failed to decode events")]
    DecodeEventsFailed(#[source] Box<LocalDbError>),

    #[error("Failed to generate SQL from events")]
    SqlGenerationFailed(#[source] Box<LocalDbError>),

    #[error("HTTP request failed with status: {status}")]
    HttpStatus { status: u16 },

    #[error(transparent)]
    ManifestFetch(#[from] FetchManifestError),

    #[error("Task join error: {0}")]
    TaskJoin(#[from] JoinError),

    #[error(transparent)]
    LocalDbQueryError(#[from] LocalDbQueryError),

    #[error("SQL dump import failed: {0}")]
    DumpImportFailed(#[source] LocalDbQueryError),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    FromHexError(#[from] FromHexError),

    #[error(transparent)]
    SqlBuildError(#[from] SqlBuildError),

    #[error(transparent)]
    InsertError(#[from] InsertError),

    #[error("Overflow when incrementing last_synced_block: {last_synced_block}")]
    LastSyncedBlockOverflow { last_synced_block: u64 },
    #[error("There are no rows in the db_metadata table")]
    MissingDbMetadataRow,

    #[error("Database schema version mismatch: expected {expected}, found {found}")]
    SchemaVersionMismatch { expected: u32, found: u32 },

    #[error("Invalid bootstrap implementation")]
    InvalidBootstrapImplementation,

    #[error("Block sync threshold exceeded: latest block {latest_block}, last indexed block {last_indexed_block}, threshold {threshold}")]
    BlockSyncThresholdExceeded {
        latest_block: u64,
        last_indexed_block: u64,
        threshold: u64,
    },

    #[error("Invalid log index '{value}'")]
    InvalidLogIndex {
        value: String,
        #[source]
        source: ParseIntError,
    },

    #[error("Missing topics filter")]
    MissingTopicsFilter,

    #[error("Missing block filter: {0}")]
    MissingBlockFilter(String),

    #[error("Block number is not number: {0}")]
    NonNumberBlockNumber(String),

    #[error(transparent)]
    FilterBlockError(#[from] FilterBlockError),

    #[error("Invalid retry max attempts")]
    InvalidRetryMaxAttemps,

    #[error(transparent)]
    ERC20Error(#[from] crate::erc20::Error),

    #[error(transparent)]
    FetchConfigError(#[from] FetchConfigError),

    #[error(transparent)]
    Export(#[from] export::ExportError),

    #[error(transparent)]
    DecodeError(#[from] DecodeError),

    #[error(transparent)]
    TryFromSliceError(#[from] TryFromSliceError),
}

impl LocalDbError {
    pub fn to_readable_msg(&self) -> String {
        match self {
            LocalDbError::CustomError(msg) => msg.clone(),
            LocalDbError::SettingsYaml(err) => format!("Settings parsing failed: {}", err),
            LocalDbError::YamlScan(err) => format!("Settings YAML scan failed: {}", err),
            LocalDbError::Http(err) => format!("HTTP request failed: {}", err),
            LocalDbError::Rpc(err) => format!("RPC error: {}", err),
            LocalDbError::JsonParse(err) => format!("Failed to parse JSON response: {}", err),
            LocalDbError::MissingField { field } => format!("Missing expected field: {}", field),
            LocalDbError::MissingLocalDbRemote { raindex_key } => format!(
                "Missing local-db remote configuration for raindex '{}'",
                raindex_key
            ),
            LocalDbError::InvalidBlockNumber { value, .. } => {
                format!("Invalid block number provided: {}", value)
            }
            LocalDbError::InvalidBlockNumberString { value, .. } => {
                format!("Invalid block number provided: {}", value)
            }
            LocalDbError::BlockHashNotFound { block_number } => format!(
                "Block {} not found when fetching block hash",
                block_number
            ),
            LocalDbError::InvalidEventsFormat => {
                "Events data is not in the expected array format".to_string()
            }
            LocalDbError::Timeout => "Network request timed out".to_string(),
            LocalDbError::TokenMetadataFetchFailed {
                address,
                attempts,
                source,
            } => format!(
                "Failed to fetch token metadata for {} after {} attempts: {}",
                address, attempts, source
            ),
            LocalDbError::MissingLocalDbSyncForNetwork { network } => format!(
                "Missing local-db sync configuration for network '{}'",
                network
            ),
            LocalDbError::Config { message } => format!("Configuration error: {}", message),
            LocalDbError::MissingRunnerTarget {
                chain_id,
                raindex_address,
            } => format!(
                "Missing runner target for chain {} raindex {:#x}",
                chain_id, raindex_address
            ),
            LocalDbError::RunnerNetworkChainIdMismatch {
                network_key,
                expected,
                found,
            } => format!(
                "Network '{}' has mismatched chain ids (expected {}, found {})",
                network_key, expected, found
            ),
            LocalDbError::DumpUrlConstructionFailed { url, source } => {
                format!("Failed to build dump url '{}': {}", url, source)
            }
            LocalDbError::DecodeError(err) => format!("Event decoding error: {}", err),
            LocalDbError::TableCheckFailed(err) => {
                format!("Failed to check required tables: {}", err)
            }
            LocalDbError::SyncStatusReadFailed(err) => {
                format!("Failed to read sync status: {}", err)
            }
            LocalDbError::FetchEventsFailed(err) => {
                format!("Failed to fetch events: {}", err.to_readable_msg())
            }
            LocalDbError::DecodeEventsFailed(err) => {
                format!("Failed to decode events: {}", err.to_readable_msg())
            }
            LocalDbError::SqlGenerationFailed(err) => {
                format!(
                    "Failed to generate SQL from events: {}",
                    err.to_readable_msg()
                )
            }
            LocalDbError::HttpStatus { status } => {
                format!("HTTP request failed with status code: {}", status)
            }
            LocalDbError::ManifestFetch(err) => format!("Failed to fetch manifest: {}", err),
            LocalDbError::TaskJoin(err) => format!("Task join error: {}", err),
            LocalDbError::LocalDbQueryError(err) => format!("Database query error: {}", err),
            LocalDbError::DumpImportFailed(err) => format!("SQL dump import failed: {}", err),
            LocalDbError::IoError(err) => format!("I/O error: {}", err),
            LocalDbError::FromHexError(err) => format!("Hex decoding error: {}", err),
            LocalDbError::SqlBuildError(err) => format!("SQL build error: {}", err),
            LocalDbError::InsertError(err) => format!("Data insertion error: {}", err),
            LocalDbError::LastSyncedBlockOverflow { last_synced_block } => format!(
                "Overflow when incrementing last_synced_block {}",
                last_synced_block
            ),
            LocalDbError::MissingDbMetadataRow => {
                "There are no rows in the db_metadata table".to_string()
            }
            LocalDbError::SchemaVersionMismatch { expected, found } => format!(
                "Database schema version mismatch: expected {}, found {}",
                expected, found
            ),
            LocalDbError::InvalidBootstrapImplementation => {
                "This bootstrap implementation is invalid.".to_string()
            }
            LocalDbError::BlockSyncThresholdExceeded {
                latest_block,
                last_indexed_block,
                threshold,
            } => format!(
                "Block sync threshold exceeded: latest block {}, last indexed block {}, threshold {}",
                latest_block, last_indexed_block, threshold
            ),
            LocalDbError::InvalidLogIndex { value, .. } => {
                format!("Invalid log index provided: {}", value)
            }
            LocalDbError::MissingTopicsFilter => "Topics are missing from the filter".to_string(),
            LocalDbError::MissingBlockFilter(value) => {
                format!("Missing block filter: {}", value)
            }
            LocalDbError::FilterBlockError(err) => format!("Filter block error: {}", err),
            LocalDbError::NonNumberBlockNumber(value) => {
                format!("Block number is not a valid number: {}", value)
            }
            LocalDbError::InvalidRetryMaxAttemps => {
                "Invalid retry configuration for max attemps".to_string()
            }
            LocalDbError::ERC20Error(err) => format!("ERC20 error: {}", err),
            LocalDbError::FetchConfigError(err) => format!("Fetch configuration error: {}", err),
            LocalDbError::Export(err) => format!("Export error: {}", err),
            LocalDbError::TryFromSliceError(err) => format!("TryFromSlice error: {}", err),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub struct RaindexIdentifier {
    pub chain_id: u32,
    pub raindex_address: Address,
}
#[cfg(target_family = "wasm")]
impl_wasm_traits!(RaindexIdentifier);

impl RaindexIdentifier {
    pub fn new(chain_id: u32, raindex_address: Address) -> Self {
        Self {
            chain_id,
            raindex_address,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_error_display_includes_message() {
        let inner = RpcClientError::RpcError {
            message: "boom".to_string(),
        };
        let err = LocalDbError::Rpc(inner);
        let display = err.to_string();
        assert!(
            display.contains("boom"),
            "expected display to include inner message, got {display}"
        );
        let readable = err.to_readable_msg();
        assert!(
            readable.contains("boom"),
            "expected readable message to include inner message, got {readable}"
        );
    }
}
