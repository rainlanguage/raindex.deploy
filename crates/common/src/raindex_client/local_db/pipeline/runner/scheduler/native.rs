use super::super::config::NetworkRunnerConfig;
use super::super::environment::default_environment;
use super::super::leadership::DefaultLeadership;
use super::super::ClientRunner;
use crate::local_db::pipeline::adapters::bootstrap::BootstrapPipeline;
use crate::local_db::pipeline::adapters::{
    apply::DefaultApplyPipeline, events::DefaultEventsPipeline, tokens::DefaultTokensPipeline,
    window::DefaultWindowPipeline,
};
use crate::local_db::pipeline::runner::utils::configured_sync_networks;
use crate::local_db::pipeline::runner::utils::ParsedRunnerSettings;
use crate::local_db::pipeline::runner::RunOutcome;
use crate::local_db::query::LocalDbQueryExecutor;
use crate::local_db::LocalDbError;
use crate::raindex_client::local_db::pipeline::bootstrap::ClientBootstrapAdapter;
use crate::raindex_client::local_db::pipeline::status::TracingStatusBus;
use crate::raindex_client::local_db::{
    LocalDb, LocalDbStatus, LocalDbSyncStatusStore, NetworkSyncStatus, RaindexSyncStatus,
    SchedulerState, SyncReadiness,
};
use raindex_app_settings::local_db_manifest::DB_SCHEMA_VERSION;
#[cfg(test)]
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

type NativeClientRunner = ClientRunner<
    ClientBootstrapAdapter,
    DefaultWindowPipeline,
    DefaultEventsPipeline,
    DefaultTokensPipeline,
    DefaultApplyPipeline,
    TracingStatusBus,
    DefaultLeadership,
>;

trait NativeRunner {
    fn run_once<'a, DB: LocalDbQueryExecutor + ?Sized>(
        &'a mut self,
        db: &'a DB,
    ) -> Pin<Box<dyn Future<Output = Result<RunOutcome, LocalDbError>> + 'a>>;
}

impl NativeRunner for NativeClientRunner {
    fn run_once<'a, DB: LocalDbQueryExecutor + ?Sized>(
        &'a mut self,
        db: &'a DB,
    ) -> Pin<Box<dyn Future<Output = Result<RunOutcome, LocalDbError>> + 'a>> {
        Box::pin(async move { self.run(db).await })
    }
}

#[derive(Clone)]
struct NativeNetworkLoopContext {
    stop_flag: Arc<AtomicBool>,
    interval_ms: u64,
    network_key: String,
    chain_id: u32,
    sync_readiness: SyncReadiness,
    status_store: LocalDbSyncStatusStore,
}

pub struct NativeSyncHandle {
    stop_flag: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for NativeSyncHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeSyncHandle")
            .field("stopped", &self.stop_flag.load(Ordering::SeqCst))
            .finish()
    }
}

impl NativeSyncHandle {
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    pub fn stop_and_join(mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn start(
    settings: ParsedRunnerSettings,
    db_path: PathBuf,
    sync_readiness: SyncReadiness,
    status_store: LocalDbSyncStatusStore,
) -> Result<NativeSyncHandle, LocalDbError> {
    let networks = configured_sync_networks(&settings);

    if networks.is_empty() {
        return Err(LocalDbError::CustomError(
            "no networks found in settings".to_string(),
        ));
    }

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = Arc::clone(&stop_flag);
    status_store.seed(&settings);

    let thread_handle = thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime for native sync");

        rt.block_on(async move {
            let local_set = tokio::task::LocalSet::new();
            local_set
                .run_until(async move {
                    let executor = crate::local_db::executor::RusqliteExecutor::new(&db_path);
                    let db = LocalDb::new(executor);

                    let bootstrap = ClientBootstrapAdapter::new();
                    if let Err(err) = bootstrap.runner_run(&db, Some(DB_SCHEMA_VERSION)).await {
                        tracing::error!(error = %err, "native sync bootstrap failed");
                        return;
                    }

                    for network in &networks {
                        if stop_flag_clone.load(Ordering::SeqCst) {
                            return;
                        }

                        let config = match NetworkRunnerConfig::from_global_settings(
                            &settings,
                            &network.key,
                        ) {
                            Ok(config) => config,
                            Err(err) => {
                                tracing::error!(
                                    network = %network.key,
                                    chain_id = network.chain_id,
                                    error = %err,
                                    "failed to create network runner config"
                                );
                                continue;
                            }
                        };

                        let interval_ms = match settings.syncs.get(&network.key) {
                            Some(sync_cfg) => sync_cfg.sync_interval_ms,
                            None => {
                                tracing::error!(
                                    network = %network.key,
                                    chain_id = network.chain_id,
                                    "missing local-db-sync settings for network"
                                );
                                continue;
                            }
                        };

                        let leadership = DefaultLeadership::with_network_key(network.key.clone());
                        let environment = default_environment(status_store.clone());

                        let runner = match NativeClientRunner::from_config(
                            config,
                            environment,
                            leadership,
                        ) {
                            Ok(r) => r,
                            Err(err) => {
                                tracing::error!(
                                    network = %network.key,
                                    chain_id = network.chain_id,
                                    error = %err,
                                    "failed to create native runner"
                                );
                                continue;
                            }
                        };

                        let db_clone = db.clone();
                        let stop = Arc::clone(&stop_flag_clone);

                        tokio::task::spawn_local(run_network_loop(
                            runner,
                            db_clone,
                            NativeNetworkLoopContext {
                                stop_flag: stop,
                                interval_ms,
                                network_key: network.key.clone(),
                                chain_id: network.chain_id,
                                sync_readiness: sync_readiness.clone(),
                                status_store: status_store.clone(),
                            },
                        ));
                    }

                    loop {
                        if stop_flag_clone.load(Ordering::SeqCst) {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                })
                .await;
        });
    });

    Ok(NativeSyncHandle {
        stop_flag,
        thread_handle: Some(thread_handle),
    })
}

async fn run_network_loop<R: NativeRunner>(
    mut runner: R,
    db: LocalDb,
    ctx: NativeNetworkLoopContext,
) {
    let NativeNetworkLoopContext {
        stop_flag,
        interval_ms,
        network_key,
        chain_id,
        sync_readiness,
        status_store,
    } = ctx;

    tracing::info!(network = %network_key, chain_id, "starting native sync loop");
    status_store.record_network_status(NetworkSyncStatus::syncing(chain_id));

    let mut backoff = super::CycleBackoff::default();
    loop {
        if stop_flag.load(Ordering::SeqCst) {
            break;
        }

        let result = runner.run_once(&db).await;
        let cycle = super::CycleResult::of(&result);
        match result {
            Ok(outcome) => match outcome {
                RunOutcome::Report(report) => {
                    if report.failures.is_empty() && cycle == super::CycleResult::CaughtUp {
                        sync_readiness.mark_ready(chain_id);
                        status_store.record_chain_ready(chain_id);
                        status_store.record_network_status(NetworkSyncStatus::active(
                            chain_id,
                            SchedulerState::Leader,
                        ));
                        tracing::debug!(
                            network = %network_key,
                            chain_id,
                            successes = report.successes.len(),
                            "sync cycle completed"
                        );
                    } else if report.failures.is_empty() {
                        status_store.record_network_status(NetworkSyncStatus::syncing(chain_id));
                        tracing::debug!(
                            network = %network_key,
                            chain_id,
                            "sync cycle stopped short of the head, continuing"
                        );
                    } else {
                        for failure in &report.failures {
                            let msg = failure.error.to_readable_msg();
                            status_store.record_raindex_status(RaindexSyncStatus::failure(
                                failure.raindex_id.clone(),
                                msg.clone(),
                            ));
                            status_store
                                .record_network_status(NetworkSyncStatus::failure(chain_id, msg));
                            tracing::warn!(
                                network = %network_key,
                                chain_id,
                                raindex = %format!("{:#x}", failure.raindex_id.raindex_address),
                                stage = ?failure.stage,
                                error = %failure.error,
                                "sync target failed"
                            );
                        }
                    }
                }
                RunOutcome::NotLeader => {
                    sync_readiness.mark_ready(chain_id);
                    status_store.record_chain_ready(chain_id);
                    status_store.record_network_status(NetworkSyncStatus::active(
                        chain_id,
                        SchedulerState::NotLeader,
                    ));
                    tracing::debug!(
                        network = %network_key,
                        chain_id,
                        "not leader, skipping sync cycle"
                    );
                }
            },
            Err(err) => {
                status_store.record_network_status(NetworkSyncStatus::new(
                    chain_id,
                    LocalDbStatus::Failure,
                    SchedulerState::Leader,
                    Some(err.to_readable_msg()),
                ));
                tracing::error!(
                    network = %network_key,
                    chain_id,
                    error = %err,
                    "sync cycle error"
                );
            }
        }

        let delay_ms = backoff.next_delay_ms(interval_ms, cycle);
        if backoff.consecutive_failures() > 0 {
            tracing::warn!(
                network = %network_key,
                chain_id,
                consecutive_failures = backoff.consecutive_failures(),
                delay_ms,
                "backing off before the next sync cycle"
            );
        }
        sleep_unless_stopped(&stop_flag, delay_ms).await;
    }

    tracing::info!(network = %network_key, chain_id, "native sync loop stopped");
}

/// Sleeps `delay_ms` in short steps, returning early once `stop_flag` is set
/// so a long backoff never delays shutdown.
async fn sleep_unless_stopped(stop_flag: &AtomicBool, delay_ms: u64) {
    let mut remaining = delay_ms;
    while remaining > 0 && !stop_flag.load(Ordering::SeqCst) {
        let step = remaining.min(super::STOP_POLL_MS);
        tokio::time::sleep(std::time::Duration::from_millis(step)).await;
        remaining -= step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_db::pipeline::runner::{RunReport, TargetFailure, TargetStage};
    use crate::local_db::query::{FromDbJson, LocalDbQueryError, SqlStatement, SqlStatementBatch};
    use crate::local_db::RaindexIdentifier;
    use alloy::primitives::Address;
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;

    struct NoopExecutor;

    #[cfg_attr(target_family = "wasm", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait::async_trait)]
    impl LocalDbQueryExecutor for NoopExecutor {
        async fn execute_batch(&self, _: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            Ok(())
        }
        async fn query_json<T: FromDbJson>(
            &self,
            _: &SqlStatement,
        ) -> Result<T, LocalDbQueryError> {
            Err(LocalDbQueryError::database("noop"))
        }
        async fn query_text(&self, _: &SqlStatement) -> Result<String, LocalDbQueryError> {
            Ok(String::new())
        }
        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Ok(())
        }
    }

    fn noop_local_db() -> LocalDb {
        LocalDb::new(NoopExecutor)
    }

    struct RecordingRunner {
        calls: Arc<AtomicUsize>,
        failures: Arc<AtomicUsize>,
        outcomes: Arc<Mutex<VecDeque<Option<bool>>>>,
    }

    impl RecordingRunner {
        fn new(
            calls: Arc<AtomicUsize>,
            failures: Arc<AtomicUsize>,
            outcomes: Vec<Option<bool>>,
        ) -> Self {
            Self {
                calls,
                failures,
                outcomes: Arc::new(Mutex::new(VecDeque::from(outcomes))),
            }
        }
    }

    impl NativeRunner for RecordingRunner {
        fn run_once<'a, DB: LocalDbQueryExecutor + ?Sized>(
            &'a mut self,
            _db: &'a DB,
        ) -> Pin<Box<dyn Future<Output = Result<RunOutcome, LocalDbError>> + 'a>> {
            let calls = Arc::clone(&self.calls);
            let failures = Arc::clone(&self.failures);
            let outcomes = Arc::clone(&self.outcomes);

            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let outcome = outcomes.lock().unwrap().pop_front().unwrap_or(Some(false));
                match outcome {
                    Some(should_fail) => {
                        if should_fail {
                            failures.fetch_add(1, Ordering::SeqCst);
                            let failure = TargetFailure {
                                raindex_id: RaindexIdentifier::new(1, Address::ZERO),
                                raindex_key: None,
                                stage: TargetStage::EngineRun,
                                error: LocalDbError::CustomError("runner failure".to_string()),
                            };
                            Ok(RunOutcome::Report(RunReport {
                                successes: vec![],
                                failures: vec![failure],
                            }))
                        } else {
                            Ok(RunOutcome::Report(RunReport {
                                successes: vec![],
                                failures: vec![],
                            }))
                        }
                    }
                    None => Ok(RunOutcome::NotLeader),
                }
            })
        }
    }

    struct ErrorRunner {
        calls: Arc<AtomicUsize>,
    }

    impl NativeRunner for ErrorRunner {
        fn run_once<'a, DB: LocalDbQueryExecutor + ?Sized>(
            &'a mut self,
            _db: &'a DB,
        ) -> Pin<Box<dyn Future<Output = Result<RunOutcome, LocalDbError>> + 'a>> {
            let calls = Arc::clone(&self.calls);
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(LocalDbError::CustomError("sync error".to_string()))
            })
        }
    }

    #[test]
    fn start_returns_error_for_empty_settings() {
        let settings = ParsedRunnerSettings {
            raindexes: HashMap::new(),
            syncs: HashMap::new(),
        };
        let result = start(
            settings,
            PathBuf::from("/tmp/test.db"),
            SyncReadiness::new(),
            LocalDbSyncStatusStore::new(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn stop_flag_works() {
        let handle = NativeSyncHandle {
            stop_flag: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
        };
        assert!(!handle.stop_flag.load(Ordering::SeqCst));
        handle.stop();
        assert!(handle.stop_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn stop_and_join_with_completed_thread() {
        let handle = NativeSyncHandle {
            stop_flag: Arc::new(AtomicBool::new(false)),
            thread_handle: Some(thread::spawn(|| {})),
        };
        handle.stop_and_join();
    }

    #[test]
    fn stop_and_join_without_thread_handle() {
        let handle = NativeSyncHandle {
            stop_flag: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
        };
        handle.stop_and_join();
    }

    #[test]
    fn debug_impl_shows_stopped_state() {
        let handle = NativeSyncHandle {
            stop_flag: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
        };
        let debug = format!("{:?}", handle);
        assert!(debug.contains("NativeSyncHandle"));
        assert!(debug.contains("stopped: false"));

        handle.stop();
        let debug = format!("{:?}", handle);
        assert!(debug.contains("stopped: true"));
    }

    #[tokio::test]
    async fn network_loop_runs_until_stopped() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let calls = Arc::new(AtomicUsize::new(0));
                let failures = Arc::new(AtomicUsize::new(0));
                let runner = RecordingRunner::new(
                    Arc::clone(&calls),
                    Arc::clone(&failures),
                    vec![Some(false)],
                );
                let stop_flag = Arc::new(AtomicBool::new(false));
                let readiness = SyncReadiness::new();

                tokio::task::spawn_local(run_network_loop(
                    runner,
                    noop_local_db(),
                    NativeNetworkLoopContext {
                        stop_flag: Arc::clone(&stop_flag),
                        interval_ms: 1,
                        network_key: "test".to_string(),
                        chain_id: 1,
                        sync_readiness: readiness.clone(),
                        status_store: LocalDbSyncStatusStore::new(),
                    },
                ));

                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                stop_flag.store(true, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;

                assert!(calls.load(Ordering::SeqCst) >= 1);
                assert_eq!(failures.load(Ordering::SeqCst), 0);
                assert!(
                    readiness.is_ready(1),
                    "chain should be marked ready after successful run"
                );
            })
            .await;
    }

    #[tokio::test]
    async fn network_loop_continues_after_failure() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let calls = Arc::new(AtomicUsize::new(0));
                let failures = Arc::new(AtomicUsize::new(0));
                let runner = RecordingRunner::new(
                    Arc::clone(&calls),
                    Arc::clone(&failures),
                    vec![Some(true), Some(false)],
                );
                let stop_flag = Arc::new(AtomicBool::new(false));
                let readiness = SyncReadiness::new();

                tokio::task::spawn_local(run_network_loop(
                    runner,
                    noop_local_db(),
                    NativeNetworkLoopContext {
                        stop_flag: Arc::clone(&stop_flag),
                        interval_ms: 1,
                        network_key: "test".to_string(),
                        chain_id: 1,
                        sync_readiness: readiness.clone(),
                        status_store: LocalDbSyncStatusStore::new(),
                    },
                ));

                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                stop_flag.store(true, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;

                assert!(failures.load(Ordering::SeqCst) >= 1);
                assert!(calls.load(Ordering::SeqCst) >= 2);
                assert!(
                    readiness.is_ready(1),
                    "chain should be marked ready after recovery"
                );
            })
            .await;
    }

    #[tokio::test]
    async fn network_loop_handles_not_leader() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let calls = Arc::new(AtomicUsize::new(0));
                let failures = Arc::new(AtomicUsize::new(0));
                let runner =
                    RecordingRunner::new(Arc::clone(&calls), Arc::clone(&failures), vec![None]);
                let stop_flag = Arc::new(AtomicBool::new(false));
                let readiness = SyncReadiness::new();

                tokio::task::spawn_local(run_network_loop(
                    runner,
                    noop_local_db(),
                    NativeNetworkLoopContext {
                        stop_flag: Arc::clone(&stop_flag),
                        interval_ms: 1,
                        network_key: "test".to_string(),
                        chain_id: 1,
                        sync_readiness: readiness.clone(),
                        status_store: LocalDbSyncStatusStore::new(),
                    },
                ));

                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                stop_flag.store(true, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;

                assert!(calls.load(Ordering::SeqCst) >= 1);
                assert_eq!(failures.load(Ordering::SeqCst), 0);
                assert!(
                    readiness.is_ready(1),
                    "observer should be able to read local DB"
                );
            })
            .await;
    }

    #[tokio::test]
    async fn network_loop_handles_runner_error() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let calls = Arc::new(AtomicUsize::new(0));
                let runner = ErrorRunner {
                    calls: Arc::clone(&calls),
                };
                let stop_flag = Arc::new(AtomicBool::new(false));
                let readiness = SyncReadiness::new();

                tokio::task::spawn_local(run_network_loop(
                    runner,
                    noop_local_db(),
                    NativeNetworkLoopContext {
                        stop_flag: Arc::clone(&stop_flag),
                        interval_ms: 1,
                        network_key: "test".to_string(),
                        chain_id: 1,
                        sync_readiness: readiness.clone(),
                        status_store: LocalDbSyncStatusStore::new(),
                    },
                ));

                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                stop_flag.store(true, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;

                assert!(
                    calls.load(Ordering::SeqCst) >= 2,
                    "loop should continue after errors, got {} calls",
                    calls.load(Ordering::SeqCst)
                );
                assert!(
                    !readiness.is_ready(1),
                    "chain should not be marked ready after errors"
                );
            })
            .await;
    }

    #[tokio::test]
    async fn network_loop_respects_pre_run_stop_flag() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let calls = Arc::new(AtomicUsize::new(0));
                let failures = Arc::new(AtomicUsize::new(0));
                let runner = RecordingRunner::new(
                    Arc::clone(&calls),
                    Arc::clone(&failures),
                    vec![Some(false)],
                );
                let stop_flag = Arc::new(AtomicBool::new(true));
                let readiness = SyncReadiness::new();

                tokio::task::spawn_local(run_network_loop(
                    runner,
                    noop_local_db(),
                    NativeNetworkLoopContext {
                        stop_flag: Arc::clone(&stop_flag),
                        interval_ms: 1,
                        network_key: "test".to_string(),
                        chain_id: 1,
                        sync_readiness: readiness.clone(),
                        status_store: LocalDbSyncStatusStore::new(),
                    },
                ));

                tokio::time::sleep(std::time::Duration::from_millis(20)).await;

                assert_eq!(calls.load(Ordering::SeqCst), 0);
                assert!(
                    !readiness.is_ready(1),
                    "chain should not be marked ready when stopped before running"
                );
            })
            .await;
    }

    #[tokio::test]
    async fn network_loop_respects_post_run_stop_flag() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let calls = Arc::new(AtomicUsize::new(0));
                let failures = Arc::new(AtomicUsize::new(0));
                let runner = RecordingRunner::new(
                    Arc::clone(&calls),
                    Arc::clone(&failures),
                    vec![Some(false)],
                );
                let stop_flag = Arc::new(AtomicBool::new(false));
                let stop_clone = Arc::clone(&stop_flag);
                let calls_clone = Arc::clone(&calls);
                let readiness = SyncReadiness::new();

                tokio::task::spawn_local(run_network_loop(
                    runner,
                    noop_local_db(),
                    NativeNetworkLoopContext {
                        stop_flag: stop_clone,
                        interval_ms: 10000,
                        network_key: "test".to_string(),
                        chain_id: 1,
                        sync_readiness: readiness.clone(),
                        status_store: LocalDbSyncStatusStore::new(),
                    },
                ));

                for _ in 0..200 {
                    if calls_clone.load(Ordering::SeqCst) >= 1 {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }

                stop_flag.store(true, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;

                assert_eq!(
                    calls.load(Ordering::SeqCst),
                    1,
                    "loop should stop after first run without waiting for interval"
                );
                assert!(
                    readiness.is_ready(1),
                    "chain should be marked ready after successful run"
                );
            })
            .await;
    }
}
