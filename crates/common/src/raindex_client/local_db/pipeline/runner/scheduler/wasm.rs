use super::super::bootstrap::{has_persisted_targets, provision_missing_targets};
use super::super::config::NetworkRunnerConfig;
use super::super::environment::default_environment;
use super::super::leadership::{DefaultLeadership, LeadershipGuard};
use super::super::ClientRunner;
use crate::local_db::pipeline::adapters::bootstrap::BootstrapPipeline;
use crate::local_db::pipeline::adapters::{
    apply::DefaultApplyPipeline, events::DefaultEventsPipeline, tokens::DefaultTokensPipeline,
    window::DefaultWindowPipeline,
};
use crate::local_db::pipeline::runner::environment::{
    default_dump_downloader, default_manifest_fetcher,
};
use crate::local_db::pipeline::runner::utils::configured_sync_networks;
use crate::local_db::pipeline::runner::utils::ParsedRunnerSettings;
use crate::local_db::pipeline::runner::RunOutcome;
use crate::local_db::pipeline::SyncPhase;
use crate::local_db::LocalDbError;
use crate::local_db::RaindexIdentifier;
use crate::raindex_client::local_db::pipeline::bootstrap::ClientBootstrapAdapter;
use crate::raindex_client::local_db::pipeline::status::{
    set_scheduler_state, set_status_callback, ClientStatusBus,
};
use crate::raindex_client::local_db::{
    LocalDb, LocalDbSyncStatusStore, NetworkSyncStatus, RaindexSyncStatus, SchedulerState,
    SyncReadiness,
};
use futures::future::{select, Either};
use gloo_timers::future::TimeoutFuture;
use js_sys::Function;
use raindex_app_settings::local_db_manifest::DB_SCHEMA_VERSION;
use raindex_app_settings::network::NetworkCfg;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use wasm_bindgen_utils::prelude::wasm_bindgen_futures::spawn_local;
use wasm_bindgen_utils::prelude::*;

type DefaultClientRunner = ClientRunner<
    ClientBootstrapAdapter,
    DefaultWindowPipeline,
    DefaultEventsPipeline,
    DefaultTokensPipeline,
    DefaultApplyPipeline,
    ClientStatusBus,
    DefaultLeadership,
>;

trait SchedulerRunner {
    fn run_once<'a>(
        &'a mut self,
        db_executor: &'a LocalDb,
        on_phase: Rc<dyn Fn(SyncPhase) + 'a>,
    ) -> Pin<Box<dyn Future<Output = Result<RunOutcome, LocalDbError>> + 'a>>;

    fn chain_id(&self) -> Option<u32>;

    fn raindex_ids(&self) -> Vec<RaindexIdentifier>;
}

impl SchedulerRunner for DefaultClientRunner {
    fn run_once<'a>(
        &'a mut self,
        db_executor: &'a LocalDb,
        on_phase: Rc<dyn Fn(SyncPhase) + 'a>,
    ) -> Pin<Box<dyn Future<Output = Result<RunOutcome, LocalDbError>> + 'a>> {
        Box::pin(async move {
            self.run_with_phase_callback(db_executor, |phase| on_phase(phase))
                .await
        })
    }

    fn chain_id(&self) -> Option<u32> {
        self.chain_id()
    }

    fn raindex_ids(&self) -> Vec<RaindexIdentifier> {
        self.raindex_ids()
    }
}

#[derive(Debug)]
pub struct SchedulerHandle {
    stop_flag: Rc<Cell<bool>>,
    networks: Vec<NetworkCfg>,
}

impl SchedulerHandle {
    pub fn stop(&self) {
        self.stop_flag.set(true);
    }

    pub fn networks(&self) -> &[NetworkCfg] {
        &self.networks
    }

    pub fn network_keys(&self) -> Vec<String> {
        self.networks.iter().map(|n| n.key.clone()).collect()
    }
}

pub(crate) fn start(
    settings: ParsedRunnerSettings,
    db: LocalDb,
    status_callback: Option<Function>,
    sync_readiness: SyncReadiness,
    status_store: LocalDbSyncStatusStore,
    schema_initialized: bool,
) -> Result<SchedulerHandle, LocalDbError> {
    let networks = configured_sync_networks(&settings);

    if networks.is_empty() {
        return Err(LocalDbError::CustomError(
            "no networks found in settings".to_string(),
        ));
    }

    let stop_flag = Rc::new(Cell::new(false));
    let callback = status_callback.map(Rc::new);
    status_store.seed(&settings);

    let bootstrap = ClientBootstrapAdapter::new();
    let db_clone = db.clone();
    let stop_flag_init = Rc::clone(&stop_flag);
    let networks_for_spawn = networks.clone();
    let settings_clone = settings.clone();

    spawn_local(async move {
        if stop_flag_init.get() {
            return;
        }

        set_status_callback(callback.clone());
        emit_initial_sync_statuses(&settings_clone, &status_store, callback.as_deref());

        if !schema_initialized {
            if let Err(err) = bootstrap
                .runner_run(&db_clone, Some(DB_SCHEMA_VERSION))
                .await
            {
                for network in &networks_for_spawn {
                    emit_network_status(
                        &status_store,
                        callback.as_deref(),
                        NetworkSyncStatus::failure(network.chain_id, err.to_readable_msg()),
                    );
                }
                return;
            }
        }

        let spawn_network = |network: &NetworkCfg, ownership: Option<LeadershipGuard>| {
            let config =
                match NetworkRunnerConfig::from_global_settings(&settings_clone, &network.key) {
                    Ok(config) => config,
                    Err(err) => {
                        emit_network_status(
                            &status_store,
                            callback.as_deref(),
                            NetworkSyncStatus::failure(network.chain_id, err.to_readable_msg()),
                        );
                        return;
                    }
                };

            let leadership = DefaultLeadership::with_network_key(network.key.clone());
            let environment = default_environment(status_store.clone());

            let runner = match ClientRunner::from_config(config.clone(), environment, leadership) {
                Ok(r) => r.with_initial_leadership(ownership),
                Err(err) => {
                    emit_network_status(
                        &status_store,
                        callback.as_deref(),
                        NetworkSyncStatus::failure(network.chain_id, err.to_readable_msg()),
                    );
                    return;
                }
            };

            let interval_ms = match settings_clone.syncs.get(&network.key) {
                Some(sync_cfg) => sync_cfg.sync_interval_ms as u32,
                None => {
                    emit_network_status(
                        &status_store,
                        callback.as_deref(),
                        NetworkSyncStatus::failure(
                            network.chain_id,
                            format!(
                                "missing local-db-sync settings for network '{}'",
                                network.key
                            ),
                        ),
                    );
                    return;
                }
            };

            spawn_network_loop(
                runner,
                db_clone.clone(),
                callback.clone(),
                Rc::clone(&stop_flag_init),
                interval_ms,
                sync_readiness.clone(),
                status_store.clone(),
            );
        };
        let populated = match has_persisted_targets(&db_clone).await {
            Ok(populated) => populated,
            Err(error) => {
                tracing::warn!(%error, "Database preflight failed; using network provisioning");
                true
            }
        };
        if stop_flag_init.get() {
            return;
        }
        if populated {
            for network in &networks_for_spawn {
                spawn_network(network, None);
            }
            return;
        }

        // Retain guards even on provisioning failure, and hand them directly to
        // the runners. Dropping/reacquiring Web Locks has an asynchronous gap.
        let mut network_ownership = HashMap::new();
        let provisioning = async {
            let _ownership = loop {
                if let Some(guard) = super::super::leadership::acquire_bootstrap().await? {
                    break guard;
                }
                TimeoutFuture::new(250).await;
            };
            // Another coordinator may have populated the DB while we waited.
            if has_persisted_targets(&db_clone).await? {
                return Ok(());
            }
            // The shared lock serializes coordinators; network locks exclude
            // fallback/RPC leaders from writing the same target in another tab.
            let mut cohort_settings = settings_clone.clone();
            cohort_settings.syncs.clear();
            for network in &networks_for_spawn {
                if let Some(guard) =
                    super::super::leadership::acquire_network_bootstrap(&network.key).await?
                {
                    if let Some(sync) = settings_clone.syncs.get(&network.key) {
                        cohort_settings
                            .syncs
                            .insert(network.key.clone(), sync.clone());
                        network_ownership.insert(network.key.clone(), guard);
                    }
                }
            }
            // A single runner already defers indexes on a fresh DB. Keep its
            // ownership, but avoid cohort downloads and their timeout caps.
            if cohort_settings.syncs.len() < 2 {
                return Ok(());
            }
            provision_missing_targets(
                &db_clone,
                &cohort_settings,
                &default_manifest_fetcher(),
                &default_dump_downloader(),
                |id, phase| {
                    emit_raindex_sync_statuses(
                        &status_store,
                        callback.as_deref(),
                        std::slice::from_ref(id),
                        phase,
                    );
                },
            )
            .await
        };
        match provision_until_stopped(provisioning, &stop_flag_init).await {
            Some(Err(error)) => {
                tracing::warn!(%error, "Coordinated bootstrap failed; using network provisioning");
            }
            Some(Ok(())) => {}
            None => return,
        }
        if stop_flag_init.get() {
            return;
        }

        for network in &networks_for_spawn {
            spawn_network(network, network_ownership.remove(&network.key));
        }
    });

    Ok(SchedulerHandle {
        stop_flag,
        networks,
    })
}

/// Drop pending ownership/download/import work when the scheduler is stopped.
async fn provision_until_stopped<F>(
    provisioning: F,
    stop_flag: &Cell<bool>,
) -> Option<Result<(), LocalDbError>>
where
    F: Future<Output = Result<(), LocalDbError>>,
{
    let stopped = async {
        while !stop_flag.get() {
            TimeoutFuture::new(100).await;
        }
    };
    match select(Box::pin(provisioning), Box::pin(stopped)).await {
        Either::Left((result, _)) => Some(result),
        Either::Right(_) => None,
    }
}

fn spawn_network_loop<R>(
    runner: R,
    db: LocalDb,
    callback: Option<Rc<Function>>,
    stop_flag: Rc<Cell<bool>>,
    interval_ms: u32,
    sync_readiness: SyncReadiness,
    status_store: LocalDbSyncStatusStore,
) where
    R: SchedulerRunner + 'static,
{
    spawn_local(async move {
        run_network_loop(
            runner,
            db,
            callback,
            stop_flag,
            interval_ms,
            sync_readiness,
            status_store,
        )
        .await;
    });
}

async fn run_network_loop<R>(
    mut runner: R,
    db: LocalDb,
    callback: Option<Rc<Function>>,
    stop_flag: Rc<Cell<bool>>,
    interval_ms: u32,
    sync_readiness: SyncReadiness,
    status_store: LocalDbSyncStatusStore,
) where
    R: SchedulerRunner + 'static,
{
    let chain_id = runner.chain_id().unwrap_or(0);
    let raindex_ids = runner.raindex_ids();
    let mut was_leader_last_cycle = false;

    emit_network_status(
        &status_store,
        callback.as_deref(),
        NetworkSyncStatus::syncing(chain_id),
    );

    emit_raindex_sync_statuses(
        &status_store,
        callback.as_deref(),
        &raindex_ids,
        SyncPhase::PreparingLocalDatabase,
    );

    let mut backoff = super::CycleBackoff::default();
    loop {
        if stop_flag.get() {
            break;
        }

        if was_leader_last_cycle {
            emit_network_status(
                &status_store,
                callback.as_deref(),
                NetworkSyncStatus::syncing(chain_id),
            );
        }

        let phase_status_store = status_store.clone();
        let phase_callback = callback.clone();
        let phase_raindex_ids = raindex_ids.clone();
        let on_phase = Rc::new(move |phase| {
            emit_raindex_sync_statuses(
                &phase_status_store,
                phase_callback.as_deref(),
                &phase_raindex_ids,
                phase,
            );
        });

        let result = runner.run_once(&db, on_phase).await;
        let cycle = super::CycleResult::of(&result);
        match result {
            Ok(outcome) => match outcome {
                RunOutcome::Report(report) => {
                    was_leader_last_cycle = true;
                    set_scheduler_state(SchedulerState::Leader);

                    if report.failures.is_empty() && cycle == super::CycleResult::CaughtUp {
                        sync_readiness.mark_ready(chain_id);
                        status_store.record_chain_ready(chain_id);
                        emit_raindex_active_statuses(
                            &status_store,
                            callback.as_deref(),
                            &raindex_ids,
                            SchedulerState::Leader,
                        );
                        emit_network_status(
                            &status_store,
                            callback.as_deref(),
                            NetworkSyncStatus::active(chain_id, SchedulerState::Leader),
                        );
                    } else if report.failures.is_empty() {
                        emit_network_status(
                            &status_store,
                            callback.as_deref(),
                            NetworkSyncStatus::syncing(chain_id),
                        );
                    } else {
                        let first = &report.failures[0];
                        let msg = format!(
                            "raindex {:#x} failed at {:?}: {}",
                            first.raindex_id.raindex_address,
                            first.stage,
                            first.error.to_readable_msg()
                        );
                        emit_raindex_status(
                            &status_store,
                            callback.as_deref(),
                            RaindexSyncStatus::failure(first.raindex_id.clone(), msg.clone()),
                        );
                        emit_network_status(
                            &status_store,
                            callback.as_deref(),
                            NetworkSyncStatus::failure(chain_id, msg),
                        );
                    }
                }
                RunOutcome::NotLeader => {
                    was_leader_last_cycle = false;
                    set_scheduler_state(SchedulerState::NotLeader);
                    sync_readiness.mark_ready(chain_id);
                    status_store.record_chain_ready(chain_id);
                    emit_raindex_active_statuses(
                        &status_store,
                        callback.as_deref(),
                        &raindex_ids,
                        SchedulerState::NotLeader,
                    );
                    emit_network_status(
                        &status_store,
                        callback.as_deref(),
                        NetworkSyncStatus::active(chain_id, SchedulerState::NotLeader),
                    );
                }
            },
            Err(err) => {
                was_leader_last_cycle = true;
                emit_network_status(
                    &status_store,
                    callback.as_deref(),
                    NetworkSyncStatus::failure(chain_id, err.to_readable_msg()),
                );
            }
        }

        // Sleep in short steps so a stop during a long backoff releases the
        // runner (and its network Web Lock) promptly.
        let mut remaining = backoff.next_delay_ms(u64::from(interval_ms), cycle);
        while remaining > 0 && !stop_flag.get() {
            let step = remaining.min(super::STOP_POLL_MS);
            TimeoutFuture::new(step as u32).await;
            remaining -= step;
        }
    }
}

fn emit_initial_sync_statuses(
    settings: &ParsedRunnerSettings,
    status_store: &LocalDbSyncStatusStore,
    callback: Option<&Function>,
) {
    let mut emitted_networks = HashSet::new();
    for raindex in settings.raindexes.values() {
        let chain_id = raindex.network.chain_id;
        if !settings.syncs.contains_key(&raindex.network.key) {
            continue;
        }
        if emitted_networks.insert(chain_id) {
            emit_network_status(status_store, callback, NetworkSyncStatus::syncing(chain_id));
        }
        emit_raindex_status(
            status_store,
            callback,
            RaindexSyncStatus::syncing(
                RaindexIdentifier::new(chain_id, raindex.address),
                SyncPhase::PreparingLocalDatabase,
            ),
        );
    }
}

fn emit_raindex_sync_statuses(
    status_store: &LocalDbSyncStatusStore,
    callback: Option<&Function>,
    raindex_ids: &[RaindexIdentifier],
    phase: SyncPhase,
) {
    for raindex_id in raindex_ids {
        emit_raindex_status(
            status_store,
            callback,
            RaindexSyncStatus::syncing(raindex_id.clone(), phase),
        );
    }
}

fn emit_raindex_active_statuses(
    status_store: &LocalDbSyncStatusStore,
    callback: Option<&Function>,
    raindex_ids: &[RaindexIdentifier],
    scheduler_state: SchedulerState,
) {
    for raindex_id in raindex_ids {
        emit_raindex_status(
            status_store,
            callback,
            RaindexSyncStatus::active(raindex_id.clone(), scheduler_state),
        );
    }
}

fn emit_raindex_status(
    status_store: &LocalDbSyncStatusStore,
    callback: Option<&Function>,
    status: RaindexSyncStatus,
) {
    status_store.record_raindex_status(status.clone());
    if let Some(callback) = callback {
        if let Ok(value) = serde_wasm_bindgen::to_value(&status) {
            let _ = callback.call1(&JsValue::NULL, &value);
        }
    }
}

fn emit_network_status(
    status_store: &LocalDbSyncStatusStore,
    callback: Option<&Function>,
    status: NetworkSyncStatus,
) {
    status_store.record_network_status(status.clone());
    if let Some(callback) = callback {
        if let Ok(value) = serde_wasm_bindgen::to_value(&status) {
            let _ = callback.call1(&JsValue::NULL, &value);
        }
    }
}

#[cfg(all(test, target_family = "wasm", feature = "browser-tests"))]
mod wasm_tests {
    use super::*;
    use crate::local_db::pipeline::runner::utils::parse_runner_settings;
    use crate::local_db::pipeline::runner::{RunReport, TargetFailure, TargetStage};
    use crate::local_db::RaindexIdentifier;
    use crate::raindex_client::local_db::pipeline::status::get_scheduler_state;
    use crate::raindex_client::local_db::LocalDbStatus;
    use alloy::primitives::Address;
    use gloo_timers::future::TimeoutFuture;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    fn noop_callback() -> Function {
        Function::new_no_args("return undefined;")
    }

    fn noop_local_db() -> LocalDb {
        LocalDb::from_js_local_db(
            crate::raindex_client::tests::local_db_object_from_query_callback(noop_callback()),
        )
        .unwrap()
    }

    impl SchedulerHandle {
        pub(crate) fn stop_flag_ptr(&self) -> *const Cell<bool> {
            Rc::as_ptr(&self.stop_flag)
        }
    }

    struct RecordingRunner {
        chain_id: u32,
        calls: Rc<Cell<usize>>,
        failures: Rc<Cell<usize>>,
        outcomes: Rc<RefCell<VecDeque<Option<bool>>>>,
    }

    impl RecordingRunner {
        fn new(
            chain_id: u32,
            calls: Rc<Cell<usize>>,
            failures: Rc<Cell<usize>>,
            outcomes: Vec<Option<bool>>,
        ) -> Self {
            Self {
                chain_id,
                calls,
                failures,
                outcomes: Rc::new(RefCell::new(VecDeque::from(outcomes))),
            }
        }
    }

    impl SchedulerRunner for RecordingRunner {
        fn run_once<'a>(
            &'a mut self,
            _db_executor: &'a LocalDb,
            _on_phase: Rc<dyn Fn(SyncPhase) + 'a>,
        ) -> Pin<Box<dyn Future<Output = Result<RunOutcome, LocalDbError>> + 'a>> {
            let calls = Rc::clone(&self.calls);
            let failures = Rc::clone(&self.failures);
            let outcomes = Rc::clone(&self.outcomes);

            Box::pin(async move {
                calls.set(calls.get() + 1);
                let outcome = outcomes.borrow_mut().pop_front().unwrap_or(Some(false));
                match outcome {
                    Some(should_fail) => {
                        if should_fail {
                            failures.set(failures.get() + 1);
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

        fn chain_id(&self) -> Option<u32> {
            Some(self.chain_id)
        }

        fn raindex_ids(&self) -> Vec<RaindexIdentifier> {
            Vec::new()
        }
    }

    #[wasm_bindgen_test]
    async fn start_returns_error_for_invalid_yaml() {
        let result = parse_runner_settings("not yaml");
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn startup_stop_drops_pending_provisioning_future() {
        struct PendingWork(Rc<Cell<bool>>);
        impl Drop for PendingWork {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }
        let stop = Rc::new(Cell::new(false));
        let dropped = Rc::new(Cell::new(false));
        let work = PendingWork(Rc::clone(&dropped));
        let provisioning = async move {
            let _work = work;
            futures::future::pending::<Result<(), LocalDbError>>().await
        };
        let signal = Rc::clone(&stop);
        spawn_local(async move {
            TimeoutFuture::new(5).await;
            signal.set(true);
        });
        assert!(provision_until_stopped(provisioning, &stop).await.is_none());
        assert!(dropped.get());
    }

    #[wasm_bindgen_test]
    async fn startup_failure_returns_provisioning_error() {
        let result = provision_until_stopped(
            async { Err(LocalDbError::CustomError("offline".to_string())) },
            &Cell::new(false),
        )
        .await;
        assert!(matches!(result, Some(Err(_))));
    }

    #[wasm_bindgen_test]
    async fn startup_stop_cancels_an_open_import_session() {
        use crate::local_db::query::LocalDbQueryExecutor;
        use std::sync::Arc;

        let object = Function::new_no_args(
            r#"return {
                appended: false, cancels: 0,
                query() { return {value:'[]'}; },
                transaction() {}, wipeAndRecreate() {},
                beginSqlDumpImport() { return {value:'pending-session'}; },
                appendSqlDumpChunk() {
                    this.appended = true;
                    return new Promise(resolve => { this.resolveAppend = resolve; });
                },
                finishSqlDumpImport() { throw new Error('must not commit'); },
                cancelSqlDumpImport(id) {
                    if (id !== 'pending-session') throw new Error('wrong session');
                    this.cancels++; return {value:''};
                }
            };"#,
        )
        .call0(&JsValue::UNDEFINED)
        .unwrap();
        let db = LocalDb::from_js_local_db(object.clone()).unwrap();
        let stop = Rc::new(Cell::new(false));
        let signal = Rc::clone(&stop);
        let seen = object.clone();
        spawn_local(async move {
            while !js_sys::Reflect::get(&seen, &"appended".into())
                .unwrap()
                .as_bool()
                .unwrap()
            {
                TimeoutFuture::new(1).await;
            }
            signal.set(true);
        });
        let import = async {
            db.execute_sql_dumps(vec![Arc::new("BEGIN; SELECT 1; COMMIT;".to_string())])
                .await
                .map_err(LocalDbError::DumpImportFailed)
        };
        assert!(provision_until_stopped(import, &stop).await.is_none());
        assert_eq!(
            js_sys::Reflect::get(&object, &"cancels".into())
                .unwrap()
                .as_f64(),
            Some(0.0)
        );
        // The SDK rejects rollback while an append is still in flight.
        let resolve = js_sys::Reflect::get(&object, &"resolveAppend".into())
            .unwrap()
            .dyn_into::<Function>()
            .unwrap();
        let result = Function::new_no_args("return {value:''};")
            .call0(&JsValue::UNDEFINED)
            .unwrap();
        resolve.call1(&JsValue::UNDEFINED, &result).unwrap();
        TimeoutFuture::new(10).await;
        assert_eq!(
            js_sys::Reflect::get(&object, &"cancels".into())
                .unwrap()
                .as_f64(),
            Some(1.0)
        );
        // Cancellation cleanup releases the serialization gate for later reads.
        assert_eq!(
            db.query_text(&crate::local_db::query::SqlStatement::new("SELECT 1"))
                .await
                .unwrap(),
            "[]"
        );
    }

    #[wasm_bindgen_test]
    async fn network_loop_runs_until_stopped() {
        let calls = Rc::new(Cell::new(0));
        let failures = Rc::new(Cell::new(0));
        let runner = RecordingRunner::new(
            1,
            Rc::clone(&calls),
            Rc::clone(&failures),
            vec![Some(false)],
        );
        let stop_flag = Rc::new(Cell::new(false));

        spawn_network_loop(
            runner,
            noop_local_db(),
            None,
            Rc::clone(&stop_flag),
            1,
            SyncReadiness::new(),
            LocalDbSyncStatusStore::new(),
        );

        TimeoutFuture::new(0).await;
        TimeoutFuture::new(3).await;

        stop_flag.set(true);
        TimeoutFuture::new(5).await;

        assert!(calls.get() >= 1);
        assert_eq!(failures.get(), 0);
    }

    #[wasm_bindgen_test]
    async fn network_loop_continues_after_failure() {
        let calls = Rc::new(Cell::new(0));
        let failures = Rc::new(Cell::new(0));
        let runner = RecordingRunner::new(
            1,
            Rc::clone(&calls),
            Rc::clone(&failures),
            vec![Some(true), Some(false)],
        );
        let stop_flag = Rc::new(Cell::new(false));

        spawn_network_loop(
            runner,
            noop_local_db(),
            None,
            Rc::clone(&stop_flag),
            1,
            SyncReadiness::new(),
            LocalDbSyncStatusStore::new(),
        );

        TimeoutFuture::new(0).await;
        TimeoutFuture::new(5).await;

        stop_flag.set(true);
        TimeoutFuture::new(5).await;

        assert!(failures.get() >= 1);
        assert!(calls.get() >= 2);
    }

    #[wasm_bindgen_test]
    async fn network_loop_handles_not_leader() {
        let calls = Rc::new(Cell::new(0));
        let failures = Rc::new(Cell::new(0));
        let runner = RecordingRunner::new(
            1,
            Rc::clone(&calls),
            Rc::clone(&failures),
            vec![None, Some(false)],
        );
        let stop_flag = Rc::new(Cell::new(false));

        let statuses = Rc::new(RefCell::new(Vec::new()));
        let status_callback = {
            let statuses = Rc::clone(&statuses);
            let closure = Closure::wrap(Box::new(move |value: JsValue| {
                let snapshot: NetworkSyncStatus =
                    serde_wasm_bindgen::from_value(value).expect("valid status value");
                statuses.borrow_mut().push(snapshot);
            }) as Box<dyn FnMut(JsValue)>);
            let function: Function = closure.as_ref().clone().unchecked_into();
            closure.forget();
            function
        };
        let readiness = SyncReadiness::new();

        spawn_network_loop(
            runner,
            noop_local_db(),
            Some(Rc::new(status_callback)),
            Rc::clone(&stop_flag),
            1,
            readiness.clone(),
            LocalDbSyncStatusStore::new(),
        );

        TimeoutFuture::new(0).await;
        TimeoutFuture::new(5).await;

        stop_flag.set(true);
        TimeoutFuture::new(5).await;

        let recorded = statuses.borrow();
        assert!(
            recorded
                .iter()
                .any(|s| s.scheduler_state == SchedulerState::NotLeader),
            "expected NotLeader status"
        );
        assert_eq!(
            get_scheduler_state(),
            SchedulerState::Leader,
            "scheduler state should be Leader after recovering"
        );
        assert!(
            readiness.is_ready(1),
            "observer should be able to read local DB"
        );
    }

    #[wasm_bindgen_test]
    async fn network_loop_emits_status_transitions() {
        let calls = Rc::new(Cell::new(0));
        let failures = Rc::new(Cell::new(0));
        let runner = RecordingRunner::new(
            42,
            Rc::clone(&calls),
            Rc::clone(&failures),
            vec![Some(true), Some(false)],
        );
        let stop_flag = Rc::new(Cell::new(false));

        let statuses = Rc::new(RefCell::new(Vec::new()));
        let status_callback = {
            let statuses = Rc::clone(&statuses);
            let closure = Closure::wrap(Box::new(move |value: JsValue| {
                let snapshot: NetworkSyncStatus =
                    serde_wasm_bindgen::from_value(value).expect("valid status value");
                statuses.borrow_mut().push(snapshot);
            }) as Box<dyn FnMut(JsValue)>);
            let function: Function = closure.as_ref().clone().unchecked_into();
            closure.forget();
            function
        };

        spawn_network_loop(
            runner,
            noop_local_db(),
            Some(Rc::new(status_callback)),
            Rc::clone(&stop_flag),
            1,
            SyncReadiness::new(),
            LocalDbSyncStatusStore::new(),
        );

        TimeoutFuture::new(0).await;
        TimeoutFuture::new(5).await;

        stop_flag.set(true);
        TimeoutFuture::new(5).await;

        let recorded = statuses.borrow();
        assert!(
            recorded
                .iter()
                .any(|s| s.status == LocalDbStatus::Failure && s.chain_id == 42),
            "expected failure status for chain 42"
        );
        assert!(
            recorded
                .iter()
                .any(|s| s.status == LocalDbStatus::Active && s.chain_id == 42),
            "expected active status for chain 42"
        );
        assert!(
            recorded
                .iter()
                .any(|s| s.status == LocalDbStatus::Syncing && s.chain_id == 42),
            "expected syncing status for chain 42"
        );
        assert!(
            recorded.iter().all(|s| s.chain_id == 42),
            "expected all statuses to have chain_id 42"
        );
    }

    #[wasm_bindgen_test]
    async fn network_loop_emits_syncing_immediately_on_start() {
        let calls = Rc::new(Cell::new(0));
        let failures = Rc::new(Cell::new(0));
        let runner = RecordingRunner::new(
            99,
            Rc::clone(&calls),
            Rc::clone(&failures),
            vec![Some(false)],
        );
        let stop_flag = Rc::new(Cell::new(false));

        let statuses = Rc::new(RefCell::new(Vec::new()));
        let status_callback = {
            let statuses = Rc::clone(&statuses);
            let closure = Closure::wrap(Box::new(move |value: JsValue| {
                let snapshot: NetworkSyncStatus =
                    serde_wasm_bindgen::from_value(value).expect("valid status value");
                statuses.borrow_mut().push(snapshot);
            }) as Box<dyn FnMut(JsValue)>);
            let function: Function = closure.as_ref().clone().unchecked_into();
            closure.forget();
            function
        };

        spawn_network_loop(
            runner,
            noop_local_db(),
            Some(Rc::new(status_callback)),
            Rc::clone(&stop_flag),
            1,
            SyncReadiness::new(),
            LocalDbSyncStatusStore::new(),
        );

        TimeoutFuture::new(0).await;
        TimeoutFuture::new(5).await;

        stop_flag.set(true);
        TimeoutFuture::new(5).await;

        let recorded = statuses.borrow();
        assert!(
            !recorded.is_empty(),
            "expected at least one status to be emitted"
        );
        assert_eq!(
            recorded[0].status,
            LocalDbStatus::Syncing,
            "first status should be Syncing"
        );
        assert_eq!(
            recorded[0].chain_id, 99,
            "first status should have correct chain_id"
        );
    }

    struct DelayedRunner {
        chain_id: u32,
        calls: Rc<Cell<usize>>,
        delay_ms: u32,
    }

    impl DelayedRunner {
        fn new(chain_id: u32, calls: Rc<Cell<usize>>, delay_ms: u32) -> Self {
            Self {
                chain_id,
                calls,
                delay_ms,
            }
        }
    }

    impl SchedulerRunner for DelayedRunner {
        fn run_once<'a>(
            &'a mut self,
            _db_executor: &'a LocalDb,
            _on_phase: Rc<dyn Fn(SyncPhase) + 'a>,
        ) -> Pin<Box<dyn Future<Output = Result<RunOutcome, LocalDbError>> + 'a>> {
            let calls = Rc::clone(&self.calls);
            let delay_ms = self.delay_ms;

            Box::pin(async move {
                if delay_ms > 0 {
                    TimeoutFuture::new(delay_ms).await;
                }
                calls.set(calls.get() + 1);
                Ok(RunOutcome::Report(RunReport {
                    successes: vec![],
                    failures: vec![],
                }))
            })
        }

        fn chain_id(&self) -> Option<u32> {
            Some(self.chain_id)
        }

        fn raindex_ids(&self) -> Vec<RaindexIdentifier> {
            Vec::new()
        }
    }

    #[wasm_bindgen_test]
    fn scheduler_handle_networks_returns_correct_network_configs() {
        use crate::raindex_client::tests::get_local_db_test_yaml;

        let yaml = format!(
            r#"{}
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
"#,
            get_local_db_test_yaml()
        );
        let settings = parse_runner_settings(&yaml).expect("should parse valid yaml");
        let handle = start(
            settings,
            noop_local_db(),
            None,
            SyncReadiness::new(),
            LocalDbSyncStatusStore::new(),
            false,
        )
        .expect("should start with valid yaml");

        handle.stop();

        let networks = handle.networks();
        assert_eq!(networks.len(), 1, "expected exactly one network");
        assert_eq!(networks[0].key, "arbitrum");
        assert_eq!(networks[0].chain_id, 42161);

        let network_keys = handle.network_keys();
        assert_eq!(network_keys, vec!["arbitrum".to_string()]);
    }

    #[wasm_bindgen_test]
    async fn slow_network_does_not_block_fast_networks() {
        let slow_calls = Rc::new(Cell::new(0));
        let fast_calls = Rc::new(Cell::new(0));

        let slow_runner = DelayedRunner::new(1, Rc::clone(&slow_calls), 100);
        let fast_runner = DelayedRunner::new(2, Rc::clone(&fast_calls), 0);

        let stop_flag = Rc::new(Cell::new(false));

        spawn_network_loop(
            slow_runner,
            noop_local_db(),
            None,
            Rc::clone(&stop_flag),
            1,
            SyncReadiness::new(),
            LocalDbSyncStatusStore::new(),
        );
        spawn_network_loop(
            fast_runner,
            noop_local_db(),
            None,
            Rc::clone(&stop_flag),
            1,
            SyncReadiness::new(),
            LocalDbSyncStatusStore::new(),
        );

        TimeoutFuture::new(0).await;
        TimeoutFuture::new(50).await;

        let fast_count_midway = fast_calls.get();
        let slow_count_midway = slow_calls.get();

        TimeoutFuture::new(100).await;

        stop_flag.set(true);
        TimeoutFuture::new(10).await;

        assert!(
            fast_count_midway > slow_count_midway,
            "fast network should have more cycles than slow network midway: fast={}, slow={}",
            fast_count_midway,
            slow_count_midway
        );
        assert!(
            fast_calls.get() >= 3,
            "fast network should complete multiple cycles: got {}",
            fast_calls.get()
        );
        assert!(
            slow_calls.get() >= 1,
            "slow network should complete at least one cycle: got {}",
            slow_calls.get()
        );
    }
}
