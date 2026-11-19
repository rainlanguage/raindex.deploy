//! Split-traits API for the local DB sync engine.
//!
//! This module defines minimal, focused traits that partition sync
//! responsibilities into composable units. Environment-specific behavior
//! (browser vs producer) is provided by implementing these traits; the core
//! orchestrator depends only on the traits and does not
//! branch on environment.
//!

pub mod adapters;
pub mod engine;
pub mod runner;

use super::RaindexIdentifier;
use crate::erc20::TokenInfo;
use crate::local_db::decode::{DecodedEvent, DecodedEventData};
use crate::local_db::query::{
    fetch_erc20_tokens_by_addresses::Erc20TokenRow, LocalDbQueryExecutor,
};
use crate::local_db::{FetchConfig, LocalDbError};
use crate::rpc_client::LogEntryResponse;
use alloy::primitives::{Address, B256};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
#[cfg(target_family = "wasm")]
use wasm_bindgen_utils::{impl_wasm_traits, prelude::*};

/// Optional manual window overrides usually supplied by CLI/producer.
///
/// Orchestrators apply these after computing a finality-clamped safe head.
#[derive(Debug, Clone, Default)]
pub struct WindowOverrides {
    /// Override the start block (inclusive). When omitted the watermark or
    /// deployment block is used per policy.
    pub start_block: Option<u64>,
    /// Override the end/target block (inclusive) before finality clamping.
    pub end_block: Option<u64>,
}

/// Finality policy for windowing.
///
/// The safe head is computed as `max(deployment_block, latest - depth)` and
/// used as the upper bound for the target block, preventing tailing too close
/// to the chain head.
#[derive(Debug, Clone)]
pub struct FinalityConfig {
    /// Safe head depth; 0 means “no finality buffer”.
    pub depth: u32,
}

/// Static configuration supplied to a sync cycle.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Block where the raindex was deployed; the start block never goes
    /// below this.
    pub deployment_block: u64,
    /// Fetch configuration (batch sizes, concurrency, etc.).
    pub fetch: FetchConfig,
    /// Finality policy.
    pub finality: FinalityConfig,
    /// Optional manual window overrides (typically CLI only).
    pub window_overrides: WindowOverrides,
    /// Most blocks a single cycle may cover (inclusive span); `None` means no
    /// cap. Long-running clients set it so a cycle that keeps failing before
    /// it persists cannot re-read an ever-growing range every time; a
    /// successful cycle moves the watermark and the next one continues.
    pub max_blocks_per_cycle: Option<u64>,
}

/// Coarse execution summary for a single sync cycle.
#[derive(Debug, Clone)]
pub struct SyncOutcome {
    /// Target that was synced.
    pub raindex_id: RaindexIdentifier,
    /// Start block (inclusive) that was used.
    pub start_block: u64,
    /// Target block (inclusive) that was used.
    pub target_block: u64,
    /// Count of raw logs fetched across raindex and stores.
    pub fetched_logs: usize,
    /// Whether the cycle reached the block it was asked to sync to (the
    /// finality-clamped safe head, or the end override). False when
    /// `max_blocks_per_cycle` stopped it short, so the caller knows the
    /// target is still behind.
    pub caught_up: bool,
    /// Count of decoded events materialized during the cycle.
    pub decoded_events: usize,
}

/// Typed sync phases for status reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    PreparingLocalDatabase,
    FetchingSyncManifest,
    DownloadingInitialDump,
    FetchingLatestBlock,
    RunningBootstrap,
    ComputingSyncWindow,
    FetchingRaindexLogs,
    DecodingRaindexLogs,
    FetchingStoreLogs,
    DecodingStoreLogs,
    FetchingTokenMetadata,
    BuildingSqlBatch,
    PersistingToDatabase,
    RunningPostSyncExport,
    Idle,
}
#[cfg(target_family = "wasm")]
impl_wasm_traits!(SyncPhase);

impl SyncPhase {
    pub fn to_message(&self) -> &'static str {
        match self {
            Self::PreparingLocalDatabase => "Preparing local database",
            Self::FetchingSyncManifest => "Fetching sync manifest",
            Self::DownloadingInitialDump => "Downloading initial dump",
            Self::FetchingLatestBlock => "Fetching latest block",
            Self::RunningBootstrap => "Running bootstrap",
            Self::ComputingSyncWindow => "Computing sync window",
            Self::FetchingRaindexLogs => "Fetching raindex logs",
            Self::DecodingRaindexLogs => "Decoding raindex logs",
            Self::FetchingStoreLogs => "Fetching interpreter store logs",
            Self::DecodingStoreLogs => "Decoding interpreter store logs",
            Self::FetchingTokenMetadata => "Fetching missing token metadata",
            Self::BuildingSqlBatch => "Building SQL batch",
            Self::PersistingToDatabase => "Persisting to database",
            Self::RunningPostSyncExport => "Running post-sync export",
            Self::Idle => "No work for current window",
        }
    }
}

/// Coarse‑grained progress/status publishing.
///
/// Keep messages short and stable; a richer typed snapshot can be layered on
/// top without changing the pipeline contracts.
#[async_trait(?Send)]
pub trait StatusBus {
    /// Publishes a typed sync phase.
    async fn send(&self, phase: SyncPhase) -> Result<(), LocalDbError>;
}

/// Computes the inclusive `[start_block, target_block]` for a cycle.
///
/// Responsibilities (concrete):
/// - Read watermark for the target (last synced block, and optionally last
///   block hash).
/// - Compute `safe_head = max(deployment_block, latest - finality.depth)` and
///   apply overrides from `cfg.window_overrides` (subject to clamp).
///
/// Policy (environment-specific):
/// - Continuity check: producer verifies parent hash continuity vs stored
///   watermark hash; browser may skip or apply a light check.
///
/// Invariants:
/// - If `start_block > target_block`, the sync cycle is a no-op.
#[async_trait(?Send)]
pub trait WindowPipeline {
    /// Returns `(start_block, target_block)` for the cycle.
    async fn compute<DB>(
        &self,
        db: &DB,
        raindex_id: &RaindexIdentifier,
        cfg: &SyncConfig,
        latest_block: u64,
    ) -> Result<(u64, u64), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized;
}

/// Facade over the event fetch + decode steps.
///
/// Responsibilities (concrete):
/// - Decode via shared ABI into stable `DecodedEventData<DecodedEvent>`.
/// - Provide a uniform surface for fetching raindex/store logs.
///
/// Policy (environment-specific):
/// - Backend selection: browser uses regular/public RPCs; producer uses
///   HyperRPC for logs and regular RPCs for tokens.
#[async_trait(?Send)]
pub trait EventsPipeline {
    /// Returns the latest chain block number according to the backend.
    async fn latest_block(&self) -> Result<u64, LocalDbError>;

    /// Fetches the canonical block hash for the provided block number.
    async fn block_hash(&self, block_number: u64) -> Result<B256, LocalDbError>;

    /// Fetches raindex logs within the inclusive block range.
    async fn fetch_raindex(
        &self,
        raindex_address: Address,
        from_block: u64,
        to_block: u64,
        cfg: &FetchConfig,
    ) -> Result<Vec<LogEntryResponse>, LocalDbError>;

    /// Fetches interpreter store logs for the supplied addresses.
    async fn fetch_stores(
        &self,
        store_addresses: &[Address],
        from_block: u64,
        to_block: u64,
        cfg: &FetchConfig,
    ) -> Result<Vec<LogEntryResponse>, LocalDbError>;

    /// Decodes raw logs into typed events. Decoding must be deterministic
    /// for identical input logs.
    fn decode(
        &self,
        logs: &[LogEntryResponse],
    ) -> Result<Vec<DecodedEventData<DecodedEvent>>, LocalDbError>;
}

/// ERC-20 token metadata lookup pipeline.
///
/// Responsibilities (concrete):
/// - Read existing token rows for a chain and compute the missing set.
/// - Fetch metadata for missing tokens and return typed values; SQL generation
///   is handled by the Apply pipeline.
///
/// Invariants:
/// - Upserts must be idempotent and keyed by `(chain_id, raindex_address, token_address)`.
#[async_trait(?Send)]
pub trait TokensPipeline {
    /// Loads existing token rows for the provided lowercase addresses.
    async fn load_existing<DB>(
        &self,
        db: &DB,
        raindex_id: &RaindexIdentifier,
        token_addrs: &[Address],
    ) -> Result<Vec<Erc20TokenRow>, LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized;

    /// Fetches metadata for missing tokens using the supplied RPC endpoints.
    async fn fetch_missing(
        &self,
        missing: Vec<Address>,
        cfg: &FetchConfig,
    ) -> Result<Vec<(Address, TokenInfo)>, LocalDbError>;
}
