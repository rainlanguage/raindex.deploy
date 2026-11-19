//! Provision missing browser targets before independent network sync starts.

use super::super::bootstrap::BOOTSTRAP_CACHE_SIZE_QUERY_SQL;
use crate::local_db::{
    pipeline::{
        runner::{
            environment::{DumpDownloader, ManifestFetcher},
            remotes::lookup_manifest_entry,
            utils::{build_runner_targets, ParsedRunnerSettings},
        },
        SyncPhase,
    },
    query::{
        fetch_target_watermark::{fetch_target_watermark_stmt, TargetWatermarkRow},
        LocalDbQueryExecutor, SqlStatement,
    },
    LocalDbError, RaindexIdentifier,
};
use futures::future::{join_all, select, Either};
use gloo_timers::future::TimeoutFuture;
use std::collections::HashSet;
use std::sync::Arc;

/// Populated databases retain their indexes and do not benefit from a cohort.
pub(super) async fn has_persisted_targets<DB: LocalDbQueryExecutor + ?Sized>(
    db: &DB,
) -> Result<bool, LocalDbError> {
    let rows: Vec<serde_json::Value> = db
        .query_json_retryable(&SqlStatement::new(
            "SELECT 1 AS present FROM target_watermarks LIMIT 1",
        ))
        .await?;
    Ok(!rows.is_empty())
}

/// The caller starts with an empty target_watermarks table and holds cross-tab
/// bootstrap ownership through preflight and commit. Per-target watermark
/// checks defensively skip targets committed after that global preflight.
pub(super) async fn provision_missing_targets<DB, F>(
    db: &DB,
    settings: &ParsedRunnerSettings,
    manifests: &ManifestFetcher,
    download: &DumpDownloader,
    on_phase: F,
) -> Result<(), LocalDbError>
where
    DB: LocalDbQueryExecutor + ?Sized,
    F: Fn(&RaindexIdentifier, SyncPhase),
{
    let raindexes = settings
        .raindexes
        .iter()
        .filter(|(_, cfg)| settings.syncs.contains_key(&cfg.network.key))
        .map(|(key, cfg)| (key.clone(), cfg.clone()))
        .collect();
    let mut targets = build_runner_targets(&raindexes, &settings.syncs)?;
    targets.sort_by(|a, b| a.raindex_key.cmp(&b.raindex_key));
    let mut missing = Vec::new();
    for target in targets {
        let rows: Vec<TargetWatermarkRow> = db
            .query_json_retryable(&fetch_target_watermark_stmt(&target.inputs.raindex_id))
            .await?;
        if rows.is_empty() {
            on_phase(&target.inputs.raindex_id, SyncPhase::FetchingSyncManifest);
            missing.push(target);
        }
    }
    if missing.is_empty() {
        return Ok(());
    }

    let missing_raindexes = missing
        .iter()
        .filter_map(|target| raindexes.get_key_value(&target.raindex_key))
        .map(|(key, cfg)| (key.clone(), cfg.clone()))
        .collect();
    let manifest_map = with_download_timeout(manifests(&missing_raindexes)).await?;
    let mut seeded_ids = HashSet::new();
    let seeded = missing
        .iter()
        .filter_map(|target| {
            lookup_manifest_entry(&manifest_map, target).map(|entry| (target, entry))
        })
        // Distinct settings aliases may resolve to the same persisted target.
        // Import its plain INSERT seed once, using the first eligible alias.
        .filter(|(target, _)| seeded_ids.insert(target.inputs.raindex_id.clone()))
        .collect::<Vec<_>>();
    // One bad remote must not discard seeds that have already downloaded.
    // Failed targets are left without watermarks for their normal runner retries.
    let results = join_all(seeded.iter().map(|(target, entry)| {
        on_phase(&target.inputs.raindex_id, SyncPhase::DownloadingInitialDump);
        let future = download(&entry.dump_url);
        async move { with_download_timeout(future).await.map(Arc::new) }
    }))
    .await;
    let mut dumps = Vec::new();
    for ((target, _), result) in seeded.iter().zip(results) {
        let dump = match result {
            Ok(dump) => dump,
            Err(error) => {
                tracing::warn!(%error, target = %target.raindex_key, "Initial seed download failed; using network provisioning");
                continue;
            }
        };
        // Network leadership is held by the scheduler through this import.
        // Recheck after downloading so a seed committed before ownership was
        // acquired cannot be inserted twice into this atomic cohort.
        let rows: Vec<TargetWatermarkRow> = db
            .query_json_retryable(&fetch_target_watermark_stmt(&target.inputs.raindex_id))
            .await?;
        if rows.is_empty() {
            on_phase(&target.inputs.raindex_id, SyncPhase::RunningBootstrap);
            dumps.push(dump);
        }
    }
    if dumps.is_empty() {
        return Ok(());
    }
    // Set connection state outside the atomic import; the empty SELECT ensures
    // sqlite-web's JSON result path and makes timeout retry safe.
    let _: Vec<serde_json::Value> = db
        .query_json_retryable(&SqlStatement::new(BOOTSTRAP_CACHE_SIZE_QUERY_SQL))
        .await?;
    db.execute_sql_dumps(dumps)
        .await
        .map_err(LocalDbError::DumpImportFailed)?;
    // The normal first catch-up cycle analyzes its applied data. An ANALYZE
    // here would repeat that full-database scan before the networks are ready.
    Ok(())
}

/// A broken remote must not leave every network waiting forever before syncing.
async fn with_download_timeout<T>(
    future: crate::local_db::pipeline::runner::environment::PinnedDbFuture<T>,
) -> Result<T, LocalDbError> {
    match select(future, Box::pin(TimeoutFuture::new(120_000))).await {
        Either::Left((result, _)) => result,
        Either::Right(_) => Err(LocalDbError::CustomError(
            "Initial bootstrap download exceeded two minutes".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_db::{
        pipeline::runner::utils::parse_runner_settings,
        query::{FromDbJson, LocalDbQueryError, SqlStatementBatch, SqlValue},
    };
    use alloy::primitives::Bytes;
    use raindex_app_settings::local_db_manifest::{
        LocalDbManifest, ManifestNetwork, ManifestRaindex,
    };
    use std::{
        cell::RefCell,
        collections::{HashMap, HashSet},
        sync::atomic::{AtomicUsize, Ordering},
    };
    use wasm_bindgen_test::*;

    #[derive(Default)]
    struct RecordingDb {
        warm: HashSet<u32>,
        becomes_warm: HashSet<u32>,
        watermark_reads: RefCell<HashMap<u32, usize>>,
        imports: RefCell<Vec<Vec<Arc<String>>>>,
        analyzes: RefCell<usize>,
        fail_import: bool,
    }

    #[async_trait::async_trait(?Send)]
    impl LocalDbQueryExecutor for RecordingDb {
        async fn execute_batch(&self, _: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            panic!("cohort must use the atomic multi-dump API");
        }
        async fn execute_sql_dumps(
            &self,
            dumps: Vec<Arc<String>>,
        ) -> Result<(), LocalDbQueryError> {
            self.imports.borrow_mut().push(dumps);
            if self.fail_import {
                Err(LocalDbQueryError::database("bad SQL"))
            } else {
                Ok(())
            }
        }
        async fn query_json<T: FromDbJson>(
            &self,
            stmt: &SqlStatement,
        ) -> Result<T, LocalDbQueryError> {
            let value = match stmt.params().first() {
                None if stmt.sql() == "SELECT 1 AS present FROM target_watermarks LIMIT 1" => {
                    if self.warm.is_empty() {
                        serde_json::json!([])
                    } else {
                        serde_json::json!([{ "present": 1 }])
                    }
                }
                Some(SqlValue::U64(chain))
                    if {
                        let mut reads = self.watermark_reads.borrow_mut();
                        let count = reads.entry(*chain as u32).or_default();
                        *count += 1;
                        self.warm.contains(&(*chain as u32))
                            || (self.becomes_warm.contains(&(*chain as u32)) && *count > 1)
                    } =>
                {
                    serde_json::json!([{
                        "chain_id": chain,
                        "raindex_address": "0x0000000000000000000000000000000000000001",
                        "last_block": 100,
                        "last_hash": "0x01",
                        "updated_at": 0
                    }])
                }
                _ => serde_json::json!([]),
            };
            serde_json::from_value(value).map_err(|_| LocalDbQueryError::invalid_response())
        }
        async fn query_text(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            assert_eq!(stmt.sql(), "ANALYZE");
            *self.analyzes.borrow_mut() += 1;
            Ok(String::new())
        }
        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            panic!("cohort must not reset the DB");
        }
    }

    fn settings_and_manifest() -> (ParsedRunnerSettings, ManifestFetcher) {
        let settings = parse_runner_settings(
            r#"
version: 6
networks:
  a:
    chain-id: 1
    rpcs:
      - https://rpc.example/a
  b:
    chain-id: 2
    rpcs:
      - https://rpc.example/b
subgraphs:
  a: https://subgraph.example/a
  b: https://subgraph.example/b
local-db-remotes:
  shared: https://dump.example/manifest.yaml
raindexes:
  a:
    network: a
    subgraph: a
    address: 0x0000000000000000000000000000000000000001
    deployment-block: 1
    local-db-remote: shared
  b:
    network: b
    subgraph: b
    address: 0x0000000000000000000000000000000000000002
    deployment-block: 1
    local-db-remote: shared
local-db-sync:
  a:
    batch-size: 10
    max-concurrent-batches: 1
    retry-attempts: 1
    retry-delay-ms: 1
    rate-limit-delay-ms: 1
    finality-depth: 1
    bootstrap-block-threshold: 100
    sync-interval-ms: 5000
  b:
    batch-size: 10
    max-concurrent-batches: 1
    retry-attempts: 1
    retry-delay-ms: 1
    rate-limit-delay-ms: 1
    finality-depth: 1
    bootstrap-block-threshold: 100
    sync-interval-ms: 5000
"#,
        )
        .unwrap();
        let mut manifest = LocalDbManifest::new();
        for (key, cfg) in &settings.raindexes {
            manifest.networks.insert(
                key.clone(),
                ManifestNetwork {
                    chain_id: cfg.network.chain_id,
                    raindexes: vec![ManifestRaindex {
                        address: cfg.address,
                        dump_url: format!("https://dump.example/{key}.sql.gz")
                            .parse()
                            .unwrap(),
                        end_block: 100,
                        end_block_hash: Bytes::from_static(&[1]),
                        end_block_time_ms: 0,
                    }],
                },
            );
        }
        let manifests: ManifestFetcher = Arc::new(move |_| {
            let manifest = manifest.clone();
            Box::pin(async move {
                Ok(HashMap::from([(
                    "https://dump.example/manifest.yaml".parse().unwrap(),
                    manifest,
                )]))
            })
        });
        (settings, manifests)
    }

    #[wasm_bindgen_test]
    async fn reversed_download_order_still_imports_one_cohort() {
        let (settings, manifests) = settings_and_manifest();
        for slow_a in [true, false] {
            let db = RecordingDb::default();
            let downloader: DumpDownloader = Arc::new(move |url| {
                let a = url.path().contains("a.sql");
                Box::pin(async move {
                    if a == slow_a {
                        TimeoutFuture::new(10).await;
                    }
                    Ok(format!(
                        "BEGIN; INSERT INTO t VALUES ('{}'); COMMIT;",
                        if a { "a" } else { "b" }
                    ))
                })
            });
            provision_missing_targets(&db, &settings, &manifests, &downloader, |_, _| {})
                .await
                .unwrap();
            let imports = db.imports.borrow();
            assert_eq!(imports.len(), 1);
            assert_eq!(imports[0].len(), 2);
            assert!(imports[0][0].contains("'a'"));
            assert!(imports[0][1].contains("'b'"));
            assert_eq!(*db.analyzes.borrow(), 0, "catch-up owns ANALYZE");
        }
    }

    #[wasm_bindgen_test]
    async fn aliases_of_the_same_target_import_only_one_seed() {
        let (mut settings, manifests) = settings_and_manifest();
        let mut alias = settings.raindexes["a"].clone();
        alias.key = "a-alias".to_string();
        settings.raindexes.insert(alias.key.clone(), alias);
        let db = RecordingDb::default();
        let downloads = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&downloads);
        let downloader: DumpDownloader = Arc::new(move |_| {
            seen.fetch_add(1, Ordering::Relaxed);
            Box::pin(async { Ok("BEGIN; INSERT INTO t VALUES (1); COMMIT;".to_string()) })
        });
        provision_missing_targets(&db, &settings, &manifests, &downloader, |_, _| {})
            .await
            .unwrap();
        assert_eq!(downloads.load(Ordering::Relaxed), 2);
        let imports = db.imports.borrow();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].len(), 2);
        assert_eq!(*db.analyzes.borrow(), 0, "catch-up owns ANALYZE");
    }

    #[wasm_bindgen_test]
    async fn defensive_preflight_skips_persisted_targets() {
        let (settings, _) = settings_and_manifest();
        let manifests: ManifestFetcher =
            Arc::new(|_| panic!("warm targets must not fetch manifests"));
        let db = RecordingDb {
            warm: HashSet::from([1, 2]),
            ..Default::default()
        };
        let downloader: DumpDownloader = Arc::new(|_| panic!("warm targets must not download"));
        provision_missing_targets(&db, &settings, &manifests, &downloader, |_, _| {})
            .await
            .unwrap();
        assert!(db.imports.borrow().is_empty());
        assert_eq!(*db.analyzes.borrow(), 0);
    }

    #[wasm_bindgen_test]
    async fn defensive_preflight_filters_already_persisted_target() {
        let (settings, manifests) = settings_and_manifest();
        let manifests: ManifestFetcher = Arc::new(move |raindexes| {
            assert_eq!(raindexes.len(), 1);
            assert!(raindexes.contains_key("b"));
            manifests(raindexes)
        });
        let db = RecordingDb {
            warm: HashSet::from([1]),
            ..Default::default()
        };
        let downloader: DumpDownloader = Arc::new(|url| {
            assert!(url.path().contains("b.sql"));
            Box::pin(async { Ok("BEGIN; INSERT INTO t VALUES ('b'); COMMIT;".to_string()) })
        });
        provision_missing_targets(&db, &settings, &manifests, &downloader, |_, _| {})
            .await
            .unwrap();
        assert_eq!(db.imports.borrow()[0].len(), 1);
    }

    #[wasm_bindgen_test]
    async fn failed_download_never_opens_an_import() {
        let (settings, manifests) = settings_and_manifest();
        let db = RecordingDb::default();
        let downloader: DumpDownloader =
            Arc::new(|_| Box::pin(async { Err(LocalDbError::CustomError("offline".to_string())) }));
        provision_missing_targets(&db, &settings, &manifests, &downloader, |_, _| {})
            .await
            .unwrap();
        assert!(db.imports.borrow().is_empty());
        assert_eq!(*db.analyzes.borrow(), 0);
    }

    #[wasm_bindgen_test]
    async fn one_failed_download_keeps_the_successful_seed() {
        let (settings, manifests) = settings_and_manifest();
        let db = RecordingDb::default();
        let downloader: DumpDownloader = Arc::new(|url| {
            let fails = url.path().contains("b.sql");
            Box::pin(async move {
                if fails {
                    Err(LocalDbError::CustomError("404".to_string()))
                } else {
                    Ok("BEGIN; INSERT INTO t VALUES ('a'); COMMIT;".to_string())
                }
            })
        });
        provision_missing_targets(&db, &settings, &manifests, &downloader, |_, _| {})
            .await
            .unwrap();
        let imports = db.imports.borrow();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].len(), 1);
        assert!(imports[0][0].contains("'a'"));
        assert_eq!(*db.analyzes.borrow(), 0, "catch-up owns ANALYZE");
    }

    #[wasm_bindgen_test]
    async fn recheck_skips_a_seed_already_committed_by_another_leader() {
        let (settings, manifests) = settings_and_manifest();
        let db = RecordingDb {
            becomes_warm: HashSet::from([1]),
            ..Default::default()
        };
        let downloader: DumpDownloader = Arc::new(|url| {
            let a = url.path().contains("a.sql");
            Box::pin(async move {
                Ok(format!(
                    "BEGIN; INSERT INTO t VALUES ('{}'); COMMIT;",
                    if a { "a" } else { "b" }
                ))
            })
        });
        provision_missing_targets(&db, &settings, &manifests, &downloader, |_, _| {})
            .await
            .unwrap();
        let imports = db.imports.borrow();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].len(), 1);
        assert!(imports[0][0].contains("'b'"));
    }

    #[wasm_bindgen_test]
    async fn persisted_target_preflight_detects_partial_database() {
        assert!(!has_persisted_targets(&RecordingDb::default())
            .await
            .unwrap());
        let db = RecordingDb {
            warm: HashSet::from([1]),
            ..Default::default()
        };
        assert!(has_persisted_targets(&db).await.unwrap());
        assert!(db.imports.borrow().is_empty());
    }

    #[wasm_bindgen_test]
    async fn failed_atomic_import_does_not_analyze() {
        let (settings, manifests) = settings_and_manifest();
        let db = RecordingDb {
            fail_import: true,
            ..Default::default()
        };
        let downloader: DumpDownloader = Arc::new(|_| {
            Box::pin(async { Ok("BEGIN; INSERT INTO t VALUES (1); COMMIT;".to_string()) })
        });
        assert!(matches!(
            provision_missing_targets(&db, &settings, &manifests, &downloader, |_, _| {}).await,
            Err(LocalDbError::DumpImportFailed(_))
        ));
        assert_eq!(*db.analyzes.borrow(), 0);
    }
}
