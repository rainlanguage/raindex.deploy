pub mod environment;
pub mod export;
pub mod manifest;

use crate::commands::local_db::executor::RusqliteExecutor;
use crate::commands::local_db::pipeline::{
    bootstrap::ProducerBootstrapAdapter,
    status::{DebugStatus, ProducerStatusBus},
};
use alloy::primitives::Address;
use environment::default_environment;
use export::export_dump;
pub use export::ExportMetadata;
use manifest::{build_manifest, write_manifest_to_path};
use raindex_app_settings::local_db_manifest::ManifestRaindex;
use raindex_common::local_db::pipeline::adapters::apply::ApplyPipeline;
use raindex_common::local_db::pipeline::runner::{
    environment::RunnerEnvironment,
    remotes::lookup_manifest_entry,
    utils::{build_runner_targets, parse_runner_settings, ParsedRunnerSettings, RunnerTarget},
    RunReport, TargetFailure, TargetStage, TargetSuccess,
};
use raindex_common::local_db::pipeline::{
    adapters::{
        apply::DefaultApplyPipeline, bootstrap::BootstrapPipeline, events::DefaultEventsPipeline,
        tokens::DefaultTokensPipeline, window::DefaultWindowPipeline,
    },
    engine::SyncInputs,
    EventsPipeline, StatusBus, TokensPipeline, WindowPipeline,
};
use raindex_common::local_db::{LocalDbError, RaindexIdentifier};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::task::{JoinSet, LocalSet};
use tracing::{error, warn};
use url::Url;

pub struct ProducerRunner<B, W, E, T, A, S> {
    settings: ParsedRunnerSettings,
    targets: Vec<RunnerTarget>,
    target_lookup: HashMap<RaindexIdentifier, RunnerTarget>,
    out_root: PathBuf,
    release_base_url: Url,
    manifest_output_path: PathBuf,
    environment: RunnerEnvironment<B, W, E, T, A, S>,
}

impl<B, W, E, T, A, S> ProducerRunner<B, W, E, T, A, S>
where
    B: BootstrapPipeline + 'static,
    W: WindowPipeline + 'static,
    E: EventsPipeline + 'static,
    T: TokensPipeline + 'static,
    A: ApplyPipeline + 'static,
    S: StatusBus + 'static,
{
    pub fn with_environment(
        settings_yaml: String,
        out_root: PathBuf,
        release_base_url: Url,
        environment: RunnerEnvironment<B, W, E, T, A, S>,
    ) -> Result<Self, LocalDbError> {
        let settings = parse_runner_settings(&settings_yaml)?;
        let targets = build_runner_targets(&settings.raindexes, &settings.syncs)?;

        let mut target_lookup = HashMap::with_capacity(targets.len());
        for target in &targets {
            target_lookup.insert(target.inputs.raindex_id.clone(), target.clone());
        }

        let manifest_output_path = out_root.join("manifest.yaml");

        Ok(Self {
            settings,
            targets,
            target_lookup,
            out_root,
            release_base_url,
            manifest_output_path,
            environment,
        })
    }

    pub async fn run(&self) -> Result<ProducerRunReport, LocalDbError> {
        let manifest_map = match self
            .environment
            .fetch_manifests(&self.settings.raindexes)
            .await
        {
            Ok(map) => Arc::new(map),
            Err(error) => {
                let report = RunReport {
                    successes: Vec::new(),
                    failures: vec![TargetFailure {
                        raindex_id: RaindexIdentifier::new(0, Address::ZERO),
                        raindex_key: None,
                        stage: TargetStage::ManifestFetch,
                        error,
                    }],
                };
                return Ok(ProducerRunReport::from_parts(report, HashMap::new()));
            }
        };
        let manifest_map_for_tasks = Arc::clone(&manifest_map);
        let targets = self.targets.clone();
        let out_root = self.out_root.clone();
        let environment = self.environment.clone();

        let local = LocalSet::new();
        let (report, exports) = local
            .run_until(async move {
                let mut tasks = JoinSet::new();
                for target in targets {
                    let manifest_map = Arc::clone(&manifest_map_for_tasks);
                    let out_root = out_root.clone();
                    let environment = environment.clone();
                    tasks.spawn_local(async move {
                        let manifest_entry = lookup_manifest_entry(manifest_map.as_ref(), &target);
                        run_raindex_job::<B, W, E, T, A, S>(
                            target,
                            manifest_entry,
                            environment,
                            out_root,
                        )
                        .await
                    });
                }

                let mut successes = Vec::new();
                let mut failures = Vec::new();
                let mut exports: HashMap<RaindexIdentifier, Option<ExportMetadata>> =
                    HashMap::new();
                while let Some(result) = tasks.join_next().await {
                    match result {
                        Ok(Ok((success, export))) => {
                            exports.insert(success.outcome.raindex_id.clone(), export);
                            successes.push(success);
                        }
                        Ok(Err(failure)) => {
                            error!(
                                address = ?failure.raindex_id.raindex_address,
                                stage = ?failure.stage,
                                error = %failure.error,
                                "producer job failed (chain_id={:?})",
                                failure.raindex_id.chain_id,
                            );
                            failures.push(failure);
                        }
                        Err(join_err) => {
                            let error = LocalDbError::from(join_err);
                            error!(
                                error = %error,
                                "producer job panicked or was cancelled before completion"
                            );
                            failures.push(TargetFailure {
                                raindex_id: RaindexIdentifier::new(0, Address::ZERO),
                                raindex_key: None,
                                stage: TargetStage::EngineRun,
                                error,
                            });
                        }
                    }
                }
                Ok::<
                    (
                        RunReport,
                        HashMap<RaindexIdentifier, Option<ExportMetadata>>,
                    ),
                    LocalDbError,
                >((
                    RunReport {
                        successes,
                        failures,
                    },
                    exports,
                ))
            })
            .await?;

        if !report.failures.is_empty() {
            warn!(
                failures = report.failures.len(),
                "one or more producer jobs failed; reusing previous manifest entries for those targets"
            );
        }

        let manifest = build_manifest(
            &report.successes,
            &exports,
            &self.target_lookup,
            &self.release_base_url,
            manifest_map.as_ref(),
        )?;
        if manifest.networks.is_empty() {
            return Ok(ProducerRunReport::from_parts(report, exports));
        }
        let manifest_path = self.manifest_output_path.clone();
        write_manifest_to_path(&manifest, manifest_path.as_path()).await?;

        Ok(ProducerRunReport::from_parts(report, exports))
    }
}

impl
    ProducerRunner<
        ProducerBootstrapAdapter,
        DefaultWindowPipeline,
        DefaultEventsPipeline,
        DefaultTokensPipeline,
        DefaultApplyPipeline,
        ProducerStatusBus,
    >
{
    pub fn new(
        settings_yaml: String,
        out_root: PathBuf,
        release_base_url: Url,
        hypersync_token: String,
        debug_status: DebugStatus,
    ) -> Result<Self, LocalDbError> {
        let environment = default_environment(hypersync_token, debug_status);
        Self::with_environment(settings_yaml, out_root, release_base_url, environment)
    }
}

#[derive(Debug, Default)]
pub struct ProducerRunReport {
    pub successes: Vec<TargetSuccess>,
    pub failures: Vec<TargetFailure>,
    pub exports: HashMap<RaindexIdentifier, Option<ExportMetadata>>,
}

impl ProducerRunReport {
    fn from_parts(
        report: RunReport,
        exports: HashMap<RaindexIdentifier, Option<ExportMetadata>>,
    ) -> Self {
        Self {
            successes: report.successes,
            failures: report.failures,
            exports,
        }
    }

    pub fn successes(&self) -> &[TargetSuccess] {
        &self.successes
    }

    pub fn failures(&self) -> &[TargetFailure] {
        &self.failures
    }

    pub fn export_for(&self, raindex_id: &RaindexIdentifier) -> Option<&ExportMetadata> {
        self.exports.get(raindex_id)?.as_ref()
    }
}

async fn run_raindex_job<B, W, E, T, A, S>(
    target: RunnerTarget,
    manifest_entry: Option<ManifestRaindex>,
    environment: RunnerEnvironment<B, W, E, T, A, S>,
    out_root: PathBuf,
) -> Result<(TargetSuccess, Option<ExportMetadata>), TargetFailure>
where
    B: BootstrapPipeline + 'static,
    W: WindowPipeline + 'static,
    E: EventsPipeline + 'static,
    T: TokensPipeline + 'static,
    A: ApplyPipeline + 'static,
    S: StatusBus + 'static,
{
    let RunnerTarget {
        raindex_key,
        manifest_url,
        network_key,
        inputs,
    } = target;

    let raindex_id = inputs.raindex_id.clone();
    let ob_key_for_failure = raindex_key.clone();
    let mk_failure = move |stage: TargetStage, error: LocalDbError| TargetFailure {
        raindex_id: raindex_id.clone(),
        raindex_key: Some(ob_key_for_failure.clone()),
        stage,
        error,
    };

    let inputs = match manifest_entry {
        Some(entry) => {
            let dump_sql = environment
                .download_dump(&entry.dump_url)
                .await
                .map_err(|error| mk_failure(TargetStage::DumpDownload, error))?;
            SyncInputs {
                dump_str: Some(std::sync::Arc::new(dump_sql)),
                ..inputs
            }
        }
        None => inputs,
    };

    let target = RunnerTarget {
        raindex_key,
        manifest_url,
        network_key,
        inputs,
    };

    let db_path = db_path_for_target(&out_root, &target)
        .map_err(|error| mk_failure(TargetStage::EngineRun, error))?;
    ensure_clean_db(&db_path).map_err(|error| mk_failure(TargetStage::EngineRun, error))?;
    let executor = RusqliteExecutor::new(&db_path);

    let engine = environment
        .build_engine(&target)
        .map_err(|error| mk_failure(TargetStage::EngineBuild, error))?
        .into_engine();
    let outcome = engine
        .run(&executor, &target.inputs)
        .await
        .map_err(|error| mk_failure(TargetStage::EngineRun, error))?;
    let exported_dump = export_dump(&executor, &target, &outcome, &out_root)
        .await
        .map_err(|error| mk_failure(TargetStage::Export, error))?;

    Ok((TargetSuccess { outcome }, exported_dump))
}

fn db_path_for_target(out_root: &Path, target: &RunnerTarget) -> Result<PathBuf, LocalDbError> {
    let chain_folder = out_root.join(target.inputs.raindex_id.chain_id.to_string());
    std::fs::create_dir_all(&chain_folder)?;
    let filename = format!("{}.db", target.inputs.raindex_id.raindex_address);
    Ok(chain_folder.join(filename))
}

fn ensure_clean_db(path: &Path) -> Result<(), LocalDbError> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{address, hex::encode_prefixed, Address, Bytes, B256};
    use async_trait::async_trait;
    use flate2::read::GzDecoder;
    use raindex_app_settings::local_db_manifest::{
        LocalDbManifest, ManifestNetwork, ManifestRaindex, DB_SCHEMA_VERSION, MANIFEST_VERSION,
    };
    use raindex_app_settings::raindex::RaindexCfg;
    use raindex_app_settings::remote::manifest::ManifestMap;
    use raindex_app_settings::spec_version::SpecVersion;
    use raindex_common::local_db::pipeline::adapters::apply::ApplyPipelineTargetInfo;
    use raindex_common::local_db::pipeline::adapters::bootstrap::{
        BootstrapConfig, BootstrapPipeline, BootstrapState,
    };
    use raindex_common::local_db::pipeline::runner::environment::{
        DumpFuture, EnginePipelines, ManifestFuture,
    };
    use raindex_common::local_db::pipeline::{
        EventsPipeline, StatusBus, SyncPhase, TokensPipeline, WindowPipeline,
    };
    use raindex_common::local_db::query::{LocalDbQueryExecutor, SqlStatement, SqlStatementBatch};
    use raindex_common::local_db::{FetchConfig, LocalDbError};
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Read;
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;
    use url::Url;

    #[derive(Clone, Default)]
    struct Telemetry {
        dump_requests: Arc<Mutex<Vec<Url>>>,
        bootstrap_dumps: Arc<Mutex<Vec<Option<String>>>>,
        status_messages: Arc<Mutex<Vec<String>>>,
    }

    impl Telemetry {
        fn record_dump(&self, url: Url) {
            self.dump_requests.lock().unwrap().push(url);
        }

        fn record_bootstrap_dump(&self, dump_stmt: Option<String>) {
            self.bootstrap_dumps.lock().unwrap().push(dump_stmt);
        }

        fn record_status(&self, message: &str) {
            self.status_messages
                .lock()
                .unwrap()
                .push(message.to_string());
        }
    }

    fn dump_sql(batch: &SqlStatementBatch) -> String {
        batch
            .statements()
            .iter()
            .map(|stmt| stmt.sql())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum EngineBehavior {
        Success,
        SuccessWithExport,
        Fail,
        Panic,
    }

    #[derive(Clone)]
    struct BootstrapNoWatermark {
        telemetry: Telemetry,
    }

    impl BootstrapNoWatermark {
        fn new(telemetry: Telemetry) -> Self {
            Self { telemetry }
        }

        async fn ensure_tables<DB>(&self, db: &DB) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            let mut batch = SqlStatementBatch::new();
            batch.add(SqlStatement::new(
                "CREATE TABLE IF NOT EXISTS target_watermarks (
                    chain_id INTEGER NOT NULL,
                    raindex_address TEXT NOT NULL,
                    last_block INTEGER NOT NULL DEFAULT 0,
                    last_hash TEXT,
                    updated_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER) * 1000),
                    PRIMARY KEY (chain_id, raindex_address)
                );",
            ));
            batch.add(SqlStatement::new(
                "CREATE TABLE IF NOT EXISTS deposits (
                    chain_id INTEGER NOT NULL,
                    raindex_address TEXT NOT NULL,
                    transaction_hash TEXT NOT NULL,
                    log_index INTEGER NOT NULL,
                    block_number INTEGER NOT NULL,
                    block_timestamp INTEGER NOT NULL,
                    sender TEXT NOT NULL,
                    token TEXT NOT NULL,
                    vault_id TEXT NOT NULL,
                    deposit_amount TEXT NOT NULL,
                    deposit_amount_uint256 TEXT NOT NULL,
                    PRIMARY KEY (chain_id, raindex_address, transaction_hash, log_index)
                );",
            ));
            batch.add(SqlStatement::new(
                "CREATE TABLE IF NOT EXISTS order_events (
                    chain_id INTEGER,
                    raindex_address TEXT,
                    store_address TEXT
                );",
            ));
            batch.add(SqlStatement::new(
                "CREATE TABLE IF NOT EXISTS interpreter_store_sets (
                    chain_id INTEGER,
                    raindex_address TEXT,
                    store_address TEXT
                );",
            ));

            db.execute_batch(&batch.ensure_transaction())
                .await
                .map_err(LocalDbError::from)
        }
    }

    #[async_trait(?Send)]
    impl BootstrapPipeline for BootstrapNoWatermark {
        async fn ensure_schema<DB>(
            &self,
            _db: &DB,
            _db_schema_version: Option<u32>,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(())
        }

        async fn inspect_state<DB>(
            &self,
            _db: &DB,
            _ob_id: &RaindexIdentifier,
        ) -> Result<BootstrapState, LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(BootstrapState {
                has_required_tables: true,
                last_synced_block: None,
            })
        }

        async fn reset_db<DB>(
            &self,
            _db: &DB,
            _db_schema_version: Option<u32>,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(())
        }

        async fn clear_raindex_data<DB>(
            &self,
            _db: &DB,
            _target: &RaindexIdentifier,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(())
        }

        async fn engine_run<DB>(
            &self,
            db: &DB,
            config: &BootstrapConfig,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            self.ensure_tables(db).await?;

            // Seed an exported row but skip the watermark to trigger export failure.
            let raindex_id = &config.raindex_id;
            let raindex_address = encode_prefixed(raindex_id.raindex_address);
            let mut batch = SqlStatementBatch::new();
            batch.add(SqlStatement::new(format!(
                "INSERT INTO deposits (chain_id, raindex_address, transaction_hash, log_index, block_number, block_timestamp, sender, token, vault_id, deposit_amount, deposit_amount_uint256) \
                 VALUES ({}, '{}', '0xseedtx', 0, {}, 1700000000, '{}', '{}', '1', '1', '1') \
                 ON CONFLICT(chain_id, raindex_address, transaction_hash, log_index) DO NOTHING;",
                raindex_id.chain_id, raindex_address, config.latest_block, raindex_address, raindex_address
            )));
            db.execute_batch(&batch.ensure_transaction()).await?;

            self.telemetry.record_bootstrap_dump(
                config
                    .dump_sql
                    .as_ref()
                    .map(|sql| sql.as_str().to_owned())
                    .or_else(|| config.dump_stmt.as_ref().map(dump_sql)),
            );
            Ok(())
        }

        async fn runner_run<DB>(
            &self,
            _db: &DB,
            _db_schema_version: Option<u32>,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct StubBootstrap {
        telemetry: Telemetry,
        panic_on_run: bool,
        seed_export: bool,
    }

    impl StubBootstrap {
        fn new(telemetry: Telemetry, behavior: EngineBehavior) -> Self {
            Self {
                telemetry,
                panic_on_run: behavior == EngineBehavior::Panic,
                seed_export: matches!(behavior, EngineBehavior::SuccessWithExport),
            }
        }

        async fn ensure_tables<DB>(&self, db: &DB) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            let mut batch = SqlStatementBatch::new();
            batch.add(SqlStatement::new(
                "CREATE TABLE IF NOT EXISTS target_watermarks (
                    chain_id INTEGER NOT NULL,
                    raindex_address TEXT NOT NULL,
                    last_block INTEGER NOT NULL DEFAULT 0,
                    last_hash TEXT,
                    updated_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER) * 1000),
                    PRIMARY KEY (chain_id, raindex_address)
                );",
            ));
            batch.add(SqlStatement::new(
                "CREATE TABLE IF NOT EXISTS deposits (
                    chain_id INTEGER NOT NULL,
                    raindex_address TEXT NOT NULL,
                    transaction_hash TEXT NOT NULL,
                    log_index INTEGER NOT NULL,
                    block_number INTEGER NOT NULL,
                    block_timestamp INTEGER NOT NULL,
                    sender TEXT NOT NULL,
                    token TEXT NOT NULL,
                    vault_id TEXT NOT NULL,
                    deposit_amount TEXT NOT NULL,
                    deposit_amount_uint256 TEXT NOT NULL,
                    PRIMARY KEY (chain_id, raindex_address, transaction_hash, log_index)
                );",
            ));
            batch.add(SqlStatement::new(
                "CREATE TABLE IF NOT EXISTS order_events (
                    chain_id INTEGER,
                    raindex_address TEXT,
                    store_address TEXT
                );",
            ));
            batch.add(SqlStatement::new(
                "CREATE TABLE IF NOT EXISTS interpreter_store_sets (
                    chain_id INTEGER,
                    raindex_address TEXT,
                    store_address TEXT
                );",
            ));

            db.execute_batch(&batch.ensure_transaction())
                .await
                .map_err(LocalDbError::from)
        }
    }

    #[async_trait(?Send)]
    impl BootstrapPipeline for StubBootstrap {
        async fn ensure_schema<DB>(
            &self,
            _db: &DB,
            _db_schema_version: Option<u32>,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(())
        }

        async fn inspect_state<DB>(
            &self,
            _db: &DB,
            _ob_id: &RaindexIdentifier,
        ) -> Result<BootstrapState, LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(BootstrapState {
                has_required_tables: true,
                last_synced_block: None,
            })
        }

        async fn reset_db<DB>(
            &self,
            _db: &DB,
            _db_schema_version: Option<u32>,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(())
        }

        async fn clear_raindex_data<DB>(
            &self,
            _db: &DB,
            _target: &RaindexIdentifier,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(())
        }

        async fn engine_run<DB>(
            &self,
            db: &DB,
            config: &BootstrapConfig,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            if self.panic_on_run {
                panic!("stub bootstrap panic");
            }

            let dump_stmt = config
                .dump_sql
                .as_ref()
                .map(|sql| sql.as_str().to_owned())
                .or_else(|| config.dump_stmt.as_ref().map(dump_sql));
            self.telemetry.record_bootstrap_dump(dump_stmt);
            self.ensure_tables(db).await?;

            if self.seed_export {
                let raindex_id = &config.raindex_id;
                let raindex_address = encode_prefixed(raindex_id.raindex_address);

                let mut batch = SqlStatementBatch::new();
                batch.add(SqlStatement::new(format!(
                    "INSERT INTO deposits (chain_id, raindex_address, transaction_hash, log_index, block_number, block_timestamp, sender, token, vault_id, deposit_amount, deposit_amount_uint256) \
                     VALUES ({}, '{}', '0xseedtx', 0, {}, 1700000000, '{}', '{}', '1', '1', '1') \
                     ON CONFLICT(chain_id, raindex_address, transaction_hash, log_index) DO NOTHING;",
                    raindex_id.chain_id, raindex_address, config.latest_block, raindex_address, raindex_address
                )));
                batch.add(SqlStatement::new(format!(
                    "INSERT INTO target_watermarks (chain_id, raindex_address, last_block, last_hash, updated_at) \
                     VALUES ({}, '{}', {}, '0xfeedface', 1700000000000) \
                     ON CONFLICT(chain_id, raindex_address) DO UPDATE \
                     SET last_block = excluded.last_block, \
                         last_hash = excluded.last_hash, \
                         updated_at = excluded.updated_at;",
                    raindex_id.chain_id, raindex_address, config.latest_block
                )));

                db.execute_batch(&batch.ensure_transaction()).await?;
            }

            Ok(())
        }

        async fn runner_run<DB>(
            &self,
            _db: &DB,
            _db_schema_version: Option<u32>,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct StubWindow {
        start_block: u64,
        target_block: u64,
    }

    impl StubWindow {
        fn new(start_block: u64, target_block: u64) -> Self {
            Self {
                start_block,
                target_block,
            }
        }
    }

    #[async_trait(?Send)]
    impl WindowPipeline for StubWindow {
        async fn compute<DB>(
            &self,
            _db: &DB,
            _target: &RaindexIdentifier,
            _cfg: &raindex_common::local_db::pipeline::SyncConfig,
            _latest_block: u64,
        ) -> Result<(u64, u64), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok((self.start_block, self.target_block))
        }
    }

    #[derive(Clone)]
    struct StubEvents {
        latest_block: u64,
    }

    #[async_trait(?Send)]
    impl EventsPipeline for StubEvents {
        async fn latest_block(&self) -> Result<u64, LocalDbError> {
            Ok(self.latest_block)
        }

        async fn fetch_raindex(
            &self,
            _raindex_address: Address,
            _from_block: u64,
            _to_block: u64,
            _cfg: &FetchConfig,
        ) -> Result<Vec<raindex_common::rpc_client::LogEntryResponse>, LocalDbError> {
            Ok(Vec::new())
        }

        async fn fetch_stores(
            &self,
            _store_addresses: &[Address],
            _from_block: u64,
            _to_block: u64,
            _cfg: &FetchConfig,
        ) -> Result<Vec<raindex_common::rpc_client::LogEntryResponse>, LocalDbError> {
            Ok(Vec::new())
        }

        fn decode(
            &self,
            _logs: &[raindex_common::rpc_client::LogEntryResponse],
        ) -> Result<
            Vec<
                raindex_common::local_db::decode::DecodedEventData<
                    raindex_common::local_db::decode::DecodedEvent,
                >,
            >,
            LocalDbError,
        > {
            Ok(Vec::new())
        }

        async fn block_hash(&self, _block_number: u64) -> Result<B256, LocalDbError> {
            Ok(B256::ZERO)
        }
    }

    #[derive(Clone, Default)]
    struct StubTokens;

    #[async_trait(?Send)]
    impl TokensPipeline for StubTokens {
        async fn load_existing<DB>(
            &self,
            _db: &DB,
            _ob_id: &RaindexIdentifier,
            _token_addrs_lower: &[Address],
        ) -> Result<
            Vec<raindex_common::local_db::query::fetch_erc20_tokens_by_addresses::Erc20TokenRow>,
            LocalDbError,
        >
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(Vec::new())
        }

        async fn fetch_missing(
            &self,
            _missing: Vec<Address>,
            _cfg: &FetchConfig,
        ) -> Result<Vec<(Address, raindex_common::erc20::TokenInfo)>, LocalDbError> {
            Ok(Vec::new())
        }
    }

    #[derive(Clone)]
    struct StubApply {
        fail_persist: bool,
    }

    impl StubApply {
        fn new(fail_persist: bool) -> Self {
            Self { fail_persist }
        }
    }

    #[async_trait(?Send)]
    impl ApplyPipeline for StubApply {
        fn build_batch(
            &self,
            _target_info: &ApplyPipelineTargetInfo,
            _raw_logs: &[raindex_common::rpc_client::LogEntryResponse],
            _decoded_events: &[raindex_common::local_db::decode::DecodedEventData<
                raindex_common::local_db::decode::DecodedEvent,
            >],
            _existing_tokens: &[
                raindex_common::local_db::query::fetch_erc20_tokens_by_addresses::Erc20TokenRow
            ],
            _tokens_to_upsert: &[(Address, raindex_common::erc20::TokenInfo)],
        ) -> Result<SqlStatementBatch, LocalDbError> {
            Ok(SqlStatementBatch::new())
        }

        async fn persist<DB>(
            &self,
            _db: &DB,
            _batch: &SqlStatementBatch,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            if self.fail_persist {
                Err(LocalDbError::CustomError(
                    "apply pipeline failure".to_string(),
                ))
            } else {
                Ok(())
            }
        }

        async fn export_dump<DB>(
            &self,
            _db: &DB,
            _target: &RaindexIdentifier,
            _end_block: u64,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct StubStatusBus {
        telemetry: Telemetry,
    }

    impl StubStatusBus {
        fn new(telemetry: Telemetry) -> Self {
            Self { telemetry }
        }
    }

    #[async_trait(?Send)]
    impl StatusBus for StubStatusBus {
        async fn send(&self, phase: SyncPhase) -> Result<(), LocalDbError> {
            self.telemetry.record_status(phase.to_message());
            Ok(())
        }
    }

    fn sample_settings_yaml() -> String {
        format!(
            r#"
version: {version}
networks:
  anvil:
    rpcs:
      - https://rpc.example/anvil
    chain-id: 42161
subgraphs:
  anvil: https://subgraph.example/anvil
local-db-remotes:
  remote-a: https://manifests.example/a.yaml
  remote-b: https://manifests.example/b.yaml
  remote-c: https://manifests.example/c.yaml
local-db-sync:
  anvil:
    batch-size: 10
    max-concurrent-batches: 2
    retry-attempts: 3
    retry-delay-ms: 100
    rate-limit-delay-ms: 1
    finality-depth: 12
    bootstrap-block-threshold: 10000
    sync-interval-ms: 5000
raindexes:
  ok:
    address: 0x00000000000000000000000000000000000000a1
    network: anvil
    subgraph: anvil
    local-db-remote: remote-a
    deployment-block: 123
  fail:
    address: 0x00000000000000000000000000000000000000b2
    network: anvil
    subgraph: anvil
    local-db-remote: remote-b
    deployment-block: 456
  panic:
    address: 0x00000000000000000000000000000000000000c3
    network: anvil
    subgraph: anvil
    local-db-remote: remote-c
    deployment-block: 789
"#,
            version = SpecVersion::current()
        )
    }

    fn settings_yaml_ok_fail() -> String {
        format!(
            r#"
version: {version}
networks:
  anvil:
    rpcs:
      - https://rpc.example/anvil
    chain-id: 42161
subgraphs:
  anvil: https://subgraph.example/anvil
local-db-remotes:
  remote-a: https://manifests.example/a.yaml
  remote-b: https://manifests.example/b.yaml
local-db-sync:
  anvil:
    batch-size: 10
    max-concurrent-batches: 2
    retry-attempts: 3
    retry-delay-ms: 100
    rate-limit-delay-ms: 1
    finality-depth: 12
    bootstrap-block-threshold: 10000
    sync-interval-ms: 5000
raindexes:
  ok:
    address: 0x00000000000000000000000000000000000000a1
    network: anvil
    subgraph: anvil
    local-db-remote: remote-a
    deployment-block: 123
  fail:
    address: 0x00000000000000000000000000000000000000b2
    network: anvil
    subgraph: anvil
    local-db-remote: remote-b
    deployment-block: 456
"#,
            version = SpecVersion::current()
        )
    }

    fn settings_yaml_panic_only() -> String {
        format!(
            r#"
version: {version}
networks:
  anvil:
    rpcs:
      - https://rpc.example/anvil
    chain-id: 42161
subgraphs:
  anvil: https://subgraph.example/anvil
local-db-remotes:
  remote-c: https://manifests.example/c.yaml
local-db-sync:
  anvil:
    batch-size: 10
    max-concurrent-batches: 2
    retry-attempts: 3
    retry-delay-ms: 100
    rate-limit-delay-ms: 1
    finality-depth: 12
    bootstrap-block-threshold: 10000
    sync-interval-ms: 5000
raindexes:
  panic:
    address: 0x00000000000000000000000000000000000000c3
    network: anvil
    subgraph: anvil
    local-db-remote: remote-c
    deployment-block: 789
"#,
            version = SpecVersion::current()
        )
    }

    fn settings_yaml_ok_only() -> String {
        format!(
            r#"
version: {version}
networks:
  anvil:
    rpcs:
      - https://rpc.example/anvil
    chain-id: 42161
subgraphs:
  anvil: https://subgraph.example/anvil
local-db-remotes:
  remote-a: https://manifests.example/a.yaml
local-db-sync:
  anvil:
    batch-size: 10
    max-concurrent-batches: 2
    retry-attempts: 3
    retry-delay-ms: 100
    rate-limit-delay-ms: 1
    finality-depth: 12
    bootstrap-block-threshold: 10000
    sync-interval-ms: 5000
raindexes:
  ok:
    address: 0x00000000000000000000000000000000000000a1
    network: anvil
    subgraph: anvil
    local-db-remote: remote-a
    deployment-block: 123
"#,
            version = SpecVersion::current()
        )
    }

    fn settings_yaml_two_success() -> String {
        format!(
            r#"
version: {version}
networks:
  anvil:
    rpcs:
      - https://rpc.example/anvil
    chain-id: 42161
subgraphs:
  anvil: https://subgraph.example/anvil
local-db-remotes:
  remote-a: https://manifests.example/a.yaml
  remote-d: https://manifests.example/d.yaml
local-db-sync:
  anvil:
    batch-size: 10
    max-concurrent-batches: 2
    retry-attempts: 3
    retry-delay-ms: 100
    rate-limit-delay-ms: 1
    finality-depth: 12
    bootstrap-block-threshold: 10000
    sync-interval-ms: 5000
raindexes:
  ok:
    address: 0x00000000000000000000000000000000000000a1
    network: anvil
    subgraph: anvil
    local-db-remote: remote-a
    deployment-block: 123
  ok-second:
    address: 0x00000000000000000000000000000000000000d4
    network: anvil
    subgraph: anvil
    local-db-remote: remote-d
    deployment-block: 321
"#,
            version = SpecVersion::current()
        )
    }

    fn manifest_for_ok_raindex() -> ManifestMap {
        let url = Url::parse("https://manifests.example/a.yaml").unwrap();
        let dump_url = Url::parse("https://dumps.example/ok.dump.sql").unwrap();
        let raindex_address = address!("00000000000000000000000000000000000000a1");
        let manifest = LocalDbManifest {
            manifest_version: MANIFEST_VERSION,
            db_schema_version: DB_SCHEMA_VERSION,
            networks: HashMap::from([(
                "anvil".to_string(),
                ManifestNetwork {
                    chain_id: 42161,
                    raindexes: vec![ManifestRaindex {
                        address: raindex_address,
                        dump_url,
                        end_block: 111,
                        end_block_hash: Bytes::from_str("0xdead").unwrap(),
                        end_block_time_ms: 1000,
                    }],
                },
            )]),
        };
        HashMap::from([(url, manifest)])
    }

    fn manifest_for_two_ok_raindexes() -> ManifestMap {
        let mut map = manifest_for_ok_raindex();
        let second_url = Url::parse("https://manifests.example/d.yaml").unwrap();
        let second_dump_url = Url::parse("https://dumps.example/ok-second.dump.sql").unwrap();
        let second_address = address!("00000000000000000000000000000000000000d4");
        let manifest = LocalDbManifest {
            manifest_version: MANIFEST_VERSION,
            db_schema_version: DB_SCHEMA_VERSION,
            networks: HashMap::from([(
                "anvil".to_string(),
                ManifestNetwork {
                    chain_id: 42161,
                    raindexes: vec![ManifestRaindex {
                        address: second_address,
                        dump_url: second_dump_url,
                        end_block: 222,
                        end_block_hash: Bytes::from_str("0xbeef").unwrap(),
                        end_block_time_ms: 2000,
                    }],
                },
            )]),
        };
        map.insert(second_url, manifest);
        map
    }

    type StubEngineBuilder = Arc<
        dyn Fn(
                &RunnerTarget,
            ) -> Result<
                EnginePipelines<
                    StubBootstrap,
                    StubWindow,
                    StubEvents,
                    StubTokens,
                    StubApply,
                    StubStatusBus,
                >,
                LocalDbError,
            > + Send
            + Sync,
    >;

    type NoWatermarkEngineBuilder = Arc<
        dyn Fn(
                &RunnerTarget,
            ) -> Result<
                EnginePipelines<
                    BootstrapNoWatermark,
                    StubWindow,
                    StubEvents,
                    StubTokens,
                    StubApply,
                    StubStatusBus,
                >,
                LocalDbError,
            > + Send
            + Sync,
    >;

    fn engine_builder_for_behaviors(
        telemetry: Telemetry,
        behaviors: Arc<HashMap<String, EngineBehavior>>,
    ) -> StubEngineBuilder {
        let telemetry_for_builder = telemetry.clone();
        Arc::new(move |target: &RunnerTarget| {
            let behavior = behaviors
                .get(&target.raindex_key)
                .copied()
                .unwrap_or(EngineBehavior::Success);
            let telemetry = telemetry_for_builder.clone();
            let bootstrap = StubBootstrap::new(telemetry.clone(), behavior);
            let window = StubWindow::new(0, target.inputs.cfg.deployment_block);
            let events = StubEvents {
                latest_block: target.inputs.cfg.deployment_block,
            };
            let tokens = StubTokens;
            let apply = StubApply::new(matches!(behavior, EngineBehavior::Fail));
            let status = StubStatusBus::new(telemetry);
            Ok(EnginePipelines::new(
                bootstrap, window, events, tokens, apply, status,
            ))
        })
    }

    fn build_environment(
        manifest_map: ManifestMap,
        behaviors: HashMap<String, EngineBehavior>,
    ) -> (
        RunnerEnvironment<
            StubBootstrap,
            StubWindow,
            StubEvents,
            StubTokens,
            StubApply,
            StubStatusBus,
        >,
        Telemetry,
    ) {
        let telemetry = Telemetry::default();
        let manifests = Arc::new(manifest_map);
        let behaviors = Arc::new(behaviors);
        let manifest_fetcher = {
            let manifests = Arc::clone(&manifests);
            Arc::new(move |_raindexes: &HashMap<String, RaindexCfg>| {
                let manifests = Arc::clone(&manifests);
                Box::pin(async move { Ok((*manifests).clone()) }) as ManifestFuture
            })
        };

        let telemetry_for_downloader = telemetry.clone();
        let dump_downloader = Arc::new(move |url: &Url| {
            let url = url.clone();
            let telemetry = telemetry_for_downloader.clone();
            Box::pin(async move {
                telemetry.record_dump(url.clone());
                Ok(format!("-- dump for {}", url))
            }) as DumpFuture
        });

        let engine_builder: StubEngineBuilder =
            engine_builder_for_behaviors(telemetry.clone(), behaviors);

        let environment = RunnerEnvironment::new(manifest_fetcher, dump_downloader, engine_builder);
        (environment, telemetry)
    }

    fn parse_settings(yaml: &str) -> ParsedRunnerSettings {
        parse_runner_settings(yaml).expect("valid YAML")
    }

    fn build_targets(yaml: &str) -> Vec<RunnerTarget> {
        let parsed = parse_settings(yaml);
        build_runner_targets(&parsed.raindexes, &parsed.syncs).expect("targets")
    }

    #[test]
    fn with_environment_parses_settings() {
        let yaml = sample_settings_yaml();
        let behaviors = HashMap::new();
        let (environment, _telemetry) = build_environment(HashMap::new(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let runner = ProducerRunner::with_environment(
            yaml.clone(),
            temp_dir.path().to_path_buf(),
            Url::parse("https://releases.example.com").unwrap(),
            environment,
        )
        .expect("runner to be constructed");
        assert_eq!(runner.targets.len(), 3);
        assert_eq!(runner.settings.raindexes.len(), 3);
        assert!(runner.settings.raindexes.contains_key("ok"));
    }

    #[test]
    fn with_environment_propagates_parse_errors() {
        let yaml = "networks: {";
        let behaviors = HashMap::new();
        let (environment, _telemetry) = build_environment(HashMap::new(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let result = ProducerRunner::with_environment(
            yaml.to_string(),
            temp_dir.path().to_path_buf(),
            Url::parse("https://releases.example.com").unwrap(),
            environment,
        );
        match result {
            Err(LocalDbError::YamlScan(_) | LocalDbError::SettingsYaml(_)) => {}
            Err(other) => panic!("unexpected error variant: {other:?}"),
            Ok(_) => panic!("expected invalid YAML to produce an error"),
        }
    }

    #[test]
    fn with_environment_requires_valid_release_base_url() {
        let invalid = Url::parse("ht%tp://bad-url");
        assert!(invalid.is_err(), "expected invalid URL to fail parsing");
    }

    #[tokio::test]
    async fn run_collects_success_and_failures() {
        let yaml = settings_yaml_ok_fail();
        let mut behaviors = HashMap::new();
        behaviors.insert("ok".to_string(), EngineBehavior::Success);
        behaviors.insert("fail".to_string(), EngineBehavior::Fail);
        let (environment, telemetry) = build_environment(manifest_for_ok_raindex(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let report = runner.run().await.expect("run succeeds overall");

        assert_eq!(report.successes.len(), 1);
        assert_eq!(report.failures.len(), 1);

        let success = &report.successes[0];
        assert_eq!(success.outcome.start_block, 0);
        assert_eq!(
            success.outcome.raindex_id.raindex_address,
            address!("00000000000000000000000000000000000000a1")
        );

        let failure = &report.failures[0];
        assert_eq!(failure.raindex_id.chain_id, 42161);
        assert_eq!(
            failure.raindex_id.raindex_address,
            address!("00000000000000000000000000000000000000b2")
        );
        assert_eq!(failure.raindex_key.as_deref(), Some("fail"));
        assert!(matches!(failure.error, LocalDbError::CustomError(_)));

        let dumps: Vec<Option<String>> = telemetry.bootstrap_dumps.lock().unwrap().clone();
        assert!(dumps.contains(&Some(
            "-- dump for https://dumps.example/ok.dump.sql".to_string()
        )));

        let dump_requests = telemetry.dump_requests.lock().unwrap().clone();
        assert_eq!(dump_requests.len(), 1);
        assert_eq!(
            dump_requests[0],
            Url::parse("https://dumps.example/ok.dump.sql").unwrap()
        );
    }

    #[tokio::test]
    async fn run_records_join_errors_as_failures() {
        let yaml = settings_yaml_panic_only();
        let mut behaviors = HashMap::new();
        behaviors.insert("panic".to_string(), EngineBehavior::Panic);
        let (environment, _telemetry) = build_environment(HashMap::new(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let report = runner.run().await.expect("run completes");
        assert_eq!(report.successes.len(), 0);
        assert_eq!(report.failures.len(), 1);
        let failure = &report.failures[0];
        assert_eq!(failure.raindex_id.chain_id, 0);
        assert!(failure.raindex_key.is_none());
        assert!(matches!(failure.error, LocalDbError::TaskJoin(_)));
    }

    #[tokio::test]
    async fn run_handles_success_fail_and_panic_jobs() {
        let yaml = sample_settings_yaml();
        let mut behaviors = HashMap::new();
        behaviors.insert("ok".to_string(), EngineBehavior::Success);
        behaviors.insert("fail".to_string(), EngineBehavior::Fail);
        behaviors.insert("panic".to_string(), EngineBehavior::Panic);
        let (environment, telemetry) = build_environment(manifest_for_ok_raindex(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let report = runner.run().await.expect("run completes");

        assert_eq!(report.successes.len(), 1);
        assert_eq!(report.failures.len(), 2);

        let success_addresses: Vec<Address> = report
            .successes
            .iter()
            .map(|outcome| outcome.outcome.raindex_id.raindex_address)
            .collect();
        assert_eq!(
            success_addresses,
            vec![address!("00000000000000000000000000000000000000a1")]
        );

        let mut custom_failure = None;
        let mut join_failure = None;
        for failure in &report.failures {
            match (
                failure.raindex_id.chain_id,
                failure.raindex_id.raindex_address,
            ) {
                (42161, addr) if addr == address!("00000000000000000000000000000000000000b2") => {
                    custom_failure = Some(failure);
                }
                (0, addr) if addr.is_zero() => join_failure = Some(failure),
                _ => {}
            }
        }

        let custom_failure = custom_failure.expect("expected a custom failure");
        assert!(matches!(custom_failure.error, LocalDbError::CustomError(_)));

        let join_failure = join_failure.expect("expected a join failure");
        assert!(matches!(join_failure.error, LocalDbError::TaskJoin(_)));

        let dumps = telemetry.dump_requests.lock().unwrap();
        assert_eq!(dumps.len(), 1);
    }

    #[tokio::test]
    async fn manifest_written_with_failures_reuses_previous_entries() {
        let yaml = settings_yaml_two_success();
        let mut behaviors = HashMap::new();
        behaviors.insert("ok".to_string(), EngineBehavior::SuccessWithExport);
        behaviors.insert("ok-second".to_string(), EngineBehavior::Fail);
        let (environment, _telemetry) =
            build_environment(manifest_for_two_ok_raindexes(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            temp_dir.path().to_path_buf(),
            release_base.clone(),
            environment,
        )
        .expect("runner");

        let report = runner
            .run()
            .await
            .expect("run completes with mixed outcomes");
        assert_eq!(report.successes.len(), 1);
        assert_eq!(report.failures.len(), 1);

        let manifest_path = temp_dir.path().join("manifest.yaml");
        let manifest_contents =
            std::fs::read_to_string(&manifest_path).expect("manifest should be written");
        let manifest_lower = manifest_contents.to_lowercase();

        let trimmed_base = release_base.as_str().trim_end_matches('/');
        let success_dump = format!(
            "{}/{}-{}.sql.gz",
            trimmed_base, 42161, "0x00000000000000000000000000000000000000a1"
        );
        assert!(
            manifest_lower.contains(&success_dump),
            "expected manifest to contain updated dump URL for succeeded job"
        );
        assert!(
            manifest_lower.contains("https://dumps.example/ok-second.dump.sql"),
            "expected manifest to retain previous dump URL for failed job"
        );
    }

    #[tokio::test]
    async fn run_returns_error_when_manifest_fetch_fails() {
        let yaml = settings_yaml_ok_only();
        let telemetry = Telemetry::default();
        let manifest_fetcher = Arc::new(
            |_raindexes: &HashMap<String, RaindexCfg>| -> ManifestFuture {
                Box::pin(async {
                    Err(LocalDbError::CustomError(
                        "manifest fetch failure".to_string(),
                    ))
                })
            },
        );
        let dump_downloader =
            Arc::new(|_url: &Url| -> DumpFuture { Box::pin(async { Ok("-- dump".to_string()) }) });
        let behaviors = Arc::new(HashMap::new());
        let engine_builder = engine_builder_for_behaviors(telemetry.clone(), behaviors);
        let environment = RunnerEnvironment::new(manifest_fetcher, dump_downloader, engine_builder);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let report = runner
            .run()
            .await
            .expect("manifest fetch failure should be reported");
        assert!(report.successes.is_empty());
        assert_eq!(report.failures.len(), 1);
        let failure = &report.failures[0];
        assert_eq!(failure.stage, TargetStage::ManifestFetch);
        matches!(&failure.error, LocalDbError::CustomError(message) if message == "manifest fetch failure");
    }

    #[tokio::test]
    async fn run_records_download_failure() {
        let yaml = settings_yaml_ok_only();
        let manifest_map = manifest_for_ok_raindex();
        let telemetry = Telemetry::default();
        let manifests = Arc::new(manifest_map);
        let manifest_fetcher = {
            let manifests = Arc::clone(&manifests);
            Arc::new(move |_raindexes: &HashMap<String, RaindexCfg>| {
                let manifests = Arc::clone(&manifests);
                Box::pin(async move { Ok((*manifests).clone()) }) as ManifestFuture
            })
        };
        let telemetry_for_downloader = telemetry.clone();
        let dump_downloader = Arc::new(move |url: &Url| {
            let telemetry = telemetry_for_downloader.clone();
            let url = url.clone();
            Box::pin(async move {
                telemetry.record_dump(url.clone());
                Err(LocalDbError::CustomError(format!(
                    "download failed for {}",
                    url
                )))
            }) as DumpFuture
        });
        let behaviors = Arc::new(HashMap::new());
        let engine_builder = engine_builder_for_behaviors(telemetry.clone(), behaviors);
        let environment = RunnerEnvironment::new(manifest_fetcher, dump_downloader, engine_builder);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let report = runner.run().await.expect("run completes");
        assert!(report.successes.is_empty());
        assert_eq!(report.failures.len(), 1);
        let failure = &report.failures[0];
        assert_eq!(failure.raindex_id.chain_id, 42161);
        assert_eq!(
            failure.raindex_id.raindex_address,
            address!("00000000000000000000000000000000000000a1")
        );
        assert_eq!(failure.raindex_key.as_deref(), Some("ok"));
        assert!(matches!(
            &failure.error,
            LocalDbError::CustomError(message) if message.contains("download failed")
        ));

        let dumps = telemetry.dump_requests.lock().unwrap();
        assert_eq!(dumps.len(), 1);
    }

    #[tokio::test]
    async fn run_records_engine_build_error() {
        let yaml = settings_yaml_ok_only();
        let manifest_fetcher = Arc::new(
            |_raindexes: &HashMap<String, RaindexCfg>| -> ManifestFuture {
                Box::pin(async { Ok(HashMap::new()) })
            },
        );
        let dump_downloader =
            Arc::new(|_url: &Url| -> DumpFuture { Box::pin(async { Ok(String::new()) }) });
        let engine_builder = Arc::new(
            |_target: &RunnerTarget| -> Result<
                EnginePipelines<
                    StubBootstrap,
                    StubWindow,
                    StubEvents,
                    StubTokens,
                    StubApply,
                    StubStatusBus,
                >,
                LocalDbError,
            > { Err(LocalDbError::CustomError("engine build error".to_string())) },
        );
        let environment = RunnerEnvironment::new(manifest_fetcher, dump_downloader, engine_builder);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let report = runner.run().await.expect("run completes");
        assert!(report.successes.is_empty());
        assert_eq!(report.failures.len(), 1);
        let failure = &report.failures[0];
        assert_eq!(failure.raindex_id.chain_id, 42161);
        assert_eq!(
            failure.raindex_id.raindex_address,
            address!("00000000000000000000000000000000000000a1")
        );
        assert_eq!(failure.raindex_key.as_deref(), Some("ok"));
        assert!(matches!(
            &failure.error,
            LocalDbError::CustomError(message) if message == "engine build error"
        ));
    }

    #[tokio::test]
    async fn run_records_export_failure_with_stage() {
        let yaml = settings_yaml_ok_only();
        let manifest_map = manifest_for_ok_raindex();
        let manifest_fetcher = Arc::new(
            move |_raindexes: &HashMap<String, RaindexCfg>| -> ManifestFuture {
                let manifest_map = manifest_map.clone();
                Box::pin(async move { Ok(manifest_map) })
            },
        );
        let dump_downloader =
            Arc::new(|_url: &Url| -> DumpFuture { Box::pin(async { Ok(String::new()) }) });

        let telemetry = Telemetry::default();
        let engine_builder: NoWatermarkEngineBuilder = Arc::new(move |target: &RunnerTarget| {
            let bootstrap = BootstrapNoWatermark::new(telemetry.clone());
            let window = StubWindow::new(0, target.inputs.cfg.deployment_block);
            let events = StubEvents {
                latest_block: target.inputs.cfg.deployment_block,
            };
            let apply = StubApply::new(false);
            let status = StubStatusBus::new(telemetry.clone());
            Ok(EnginePipelines::new(
                bootstrap, window, events, StubTokens, apply, status,
            ))
        });

        let environment: RunnerEnvironment<
            BootstrapNoWatermark,
            StubWindow,
            StubEvents,
            StubTokens,
            StubApply,
            StubStatusBus,
        > = RunnerEnvironment::new(manifest_fetcher, dump_downloader, engine_builder);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let report = runner.run().await.expect("run completes with failures");
        assert_eq!(report.successes.len(), 0);
        assert_eq!(report.failures.len(), 1);
        let failure = &report.failures[0];
        assert_eq!(failure.stage, TargetStage::Export);
        assert_eq!(
            failure.raindex_id.raindex_address,
            address!("00000000000000000000000000000000000000a1")
        );
    }

    #[tokio::test]
    async fn run_succeeds_without_manifest_entry() {
        let yaml = settings_yaml_ok_only();
        let mut behaviors = HashMap::new();
        behaviors.insert("ok".to_string(), EngineBehavior::Success);
        let (environment, telemetry) = build_environment(HashMap::new(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let report = runner.run().await.expect("run completes");
        assert_eq!(report.successes.len(), 1);
        assert!(report.failures.is_empty());

        let dumps = telemetry.dump_requests.lock().unwrap();
        assert!(dumps.is_empty());
    }

    #[tokio::test]
    async fn run_success_reports_export_metadata() {
        let yaml = settings_yaml_ok_only();
        let mut behaviors = HashMap::new();
        behaviors.insert("ok".to_string(), EngineBehavior::Success);
        let (environment, _telemetry) = build_environment(manifest_for_ok_raindex(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml.clone(),
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let report = runner.run().await.expect("run completes");
        if report.successes.is_empty() {
            panic!("expected success, got failures: {:?}", report.failures);
        }
        let outcome = &report.successes[0];
        assert_eq!(outcome.outcome.raindex_id.chain_id, 42161);
        assert_eq!(outcome.outcome.start_block, 0);

        assert!(
            report.export_for(&outcome.outcome.raindex_id).is_none(),
            "stub environment should not emit dumps"
        );
    }

    #[tokio::test]
    async fn run_success_with_export_produces_dump() {
        let yaml = settings_yaml_ok_only();
        let mut behaviors = HashMap::new();
        behaviors.insert("ok".to_string(), EngineBehavior::SuccessWithExport);
        let (environment, _telemetry) = build_environment(HashMap::new(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let report = runner.run().await.expect("run completes");
        if report.successes.is_empty() {
            panic!("expected success, got failures: {:?}", report.failures);
        }
        let outcome = &report.successes[0];

        let metadata = report
            .export_for(&outcome.outcome.raindex_id)
            .expect("export metadata to be present");
        assert!(
            metadata.dump_path.exists(),
            "exported dump file should exist"
        );
        let file_name = metadata
            .dump_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("dump file name");
        let expected_file = format!(
            "{}-{}.sql.gz",
            outcome.outcome.raindex_id.chain_id, outcome.outcome.raindex_id.raindex_address
        );
        assert_eq!(file_name, expected_file);
        assert_eq!(metadata.end_block, outcome.outcome.target_block);
        assert_eq!(metadata.end_block_hash, "0xfeedface");

        let dump_bytes = std::fs::read(&metadata.dump_path).expect("read dump");
        let mut decoder = GzDecoder::new(&dump_bytes[..]);
        let mut dump_sql = String::new();
        decoder
            .read_to_string(&mut dump_sql)
            .expect("decompress dump contents");

        assert!(
            dump_sql.contains("INSERT INTO \"target_watermarks\""),
            "dump should include target_watermarks insert statements"
        );
        assert!(
            dump_sql.contains("0xfeedface"),
            "dump should preserve the watermark hash"
        );
        assert!(
            dump_sql.contains("1700000000000"),
            "dump should capture the watermark update timestamp as milliseconds"
        );
    }

    #[tokio::test]
    async fn run_successes_write_manifest_file() {
        let yaml = settings_yaml_two_success();
        let mut behaviors = HashMap::new();
        behaviors.insert("ok".to_string(), EngineBehavior::SuccessWithExport);
        behaviors.insert("ok-second".to_string(), EngineBehavior::SuccessWithExport);
        let (environment, _telemetry) = build_environment(HashMap::new(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let out_root = temp_dir.path().join("artifacts/manifests");
        let release_base = Url::parse("https://cdn.example/releases/").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            out_root.clone(),
            release_base.clone(),
            environment,
        )
        .expect("runner");

        let report = runner.run().await.expect("run completes");
        assert!(report.failures.is_empty());
        assert_eq!(report.successes.len(), 2);

        let manifest_path = out_root.join("manifest.yaml");
        assert!(
            manifest_path.exists(),
            "expected manifest file to be written"
        );
        let manifest_contents = std::fs::read_to_string(&manifest_path).expect("manifest readable");
        let manifest_lower = manifest_contents.to_lowercase();
        assert!(
            manifest_lower.contains("manifest-version: \"1\""),
            "manifest header missing"
        );
        let expected_base = "https://cdn.example/releases";
        let first_address = "0x00000000000000000000000000000000000000a1";
        let second_address = "0x00000000000000000000000000000000000000d4";
        let first_dump = format!(
            "dump-url: \"{}/42161-{}.sql.gz\"",
            expected_base, first_address
        );
        let second_dump = format!(
            "dump-url: \"{}/42161-{}.sql.gz\"",
            expected_base, second_address
        );
        assert!(
            manifest_lower.contains(&first_dump),
            "first dump url missing: {manifest_contents}"
        );
        assert!(
            manifest_lower.contains(&second_dump),
            "second dump url missing: {manifest_contents}"
        );
        let first_index = manifest_lower
            .find(first_address)
            .expect("first address present");
        let second_index = manifest_lower
            .find(second_address)
            .expect("second address present");
        assert!(
            first_index < second_index,
            "expected raindexes to be sorted by address"
        );
    }

    #[tokio::test]
    async fn run_all_successes_report_no_failures() {
        let yaml = settings_yaml_two_success();
        let mut behaviors = HashMap::new();
        behaviors.insert("ok".to_string(), EngineBehavior::Success);
        behaviors.insert("ok-second".to_string(), EngineBehavior::Success);
        let (environment, telemetry) =
            build_environment(manifest_for_two_ok_raindexes(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml,
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let report = runner.run().await.expect("run completes");
        assert_eq!(report.successes.len(), 2);
        assert!(report.failures.is_empty());

        let dumps = telemetry.dump_requests.lock().unwrap();
        assert_eq!(dumps.len(), 2);
    }

    #[tokio::test]
    async fn run_recreates_db_between_runs() {
        let yaml = settings_yaml_ok_only();
        let mut behaviors = HashMap::new();
        behaviors.insert("ok".to_string(), EngineBehavior::Success);
        let (environment, _telemetry) = build_environment(HashMap::new(), behaviors);
        let temp_dir = TempDir::new().unwrap();
        let release_base = Url::parse("https://releases.example.com").unwrap();
        let runner = ProducerRunner::with_environment(
            yaml.clone(),
            temp_dir.path().to_path_buf(),
            release_base,
            environment,
        )
        .expect("runner");

        let targets = build_targets(&yaml);
        let db_path = db_path_for_target(temp_dir.path(), &targets[0]).expect("db path");

        runner.run().await.expect("first run succeeds");

        let mut header = [0u8; 16];
        {
            let mut file = File::open(&db_path).expect("db file exists after first run");
            file.read_exact(&mut header).expect("read header");
        }
        assert_eq!(&header[..13], b"SQLite format");

        std::fs::write(&db_path, b"not sqlite!").expect("overwrite db with junk");

        runner.run().await.expect("second run succeeds");

        let mut second_header = [0u8; 16];
        {
            let mut file = File::open(&db_path).expect("db file exists after second run");
            file.read_exact(&mut second_header).expect("read header");
        }
        assert_eq!(&second_header[..13], b"SQLite format");
    }

    #[test]
    fn db_path_for_target_builds_folder_structure() {
        let yaml = settings_yaml_ok_fail();
        let targets = build_targets(&yaml);
        let temp_dir = TempDir::new().unwrap();
        let db_path = db_path_for_target(temp_dir.path(), &targets[0]).expect("path is computed");
        assert!(db_path.parent().unwrap().ends_with(PathBuf::from("42161")));
        let file = db_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase();
        let expected = format!(
            "{}.db",
            targets[0]
                .inputs
                .raindex_id
                .raindex_address
                .to_string()
                .to_lowercase()
        );
        assert!(
            file.contains(&expected),
            "expected file name '{file}' to contain '{expected}'"
        );
    }

    #[test]
    fn ensure_clean_db_removes_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("existing.db");
        std::fs::write(&db_path, b"old data").unwrap();
        ensure_clean_db(&db_path).expect("removal succeeds");
        assert!(!db_path.exists());
    }

    #[test]
    fn ensure_clean_db_noop_when_missing() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("missing.db");
        ensure_clean_db(&db_path).expect("no-op succeeds");
    }
}
