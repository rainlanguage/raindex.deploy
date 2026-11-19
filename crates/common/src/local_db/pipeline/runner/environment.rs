use super::remotes::{download_and_gunzip, get_manifests};
use super::utils::RunnerTarget;
use crate::local_db::pipeline::adapters::apply::ApplyPipeline;
use crate::local_db::pipeline::adapters::bootstrap::BootstrapPipeline;
use crate::local_db::pipeline::engine::SyncEngine;
use crate::local_db::pipeline::{EventsPipeline, StatusBus, TokensPipeline, WindowPipeline};
use crate::local_db::LocalDbError;
use raindex_app_settings::raindex::RaindexCfg;
use raindex_app_settings::remote::manifest::ManifestMap;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use url::Url;

pub type PinnedDbFuture<RES> = Pin<Box<dyn Future<Output = Result<RES, LocalDbError>> + 'static>>;
pub type ManifestFuture = PinnedDbFuture<ManifestMap>;
pub type DumpFuture = PinnedDbFuture<String>;

pub type ManifestFetcher =
    Arc<dyn Fn(&HashMap<String, RaindexCfg>) -> ManifestFuture + Send + Sync>;
pub type DumpDownloader = Arc<dyn Fn(&Url) -> DumpFuture + Send + Sync>;
pub type EngineBuilder<B, W, E, T, A, S> = Arc<
    dyn Fn(&RunnerTarget) -> Result<EnginePipelines<B, W, E, T, A, S>, LocalDbError> + Send + Sync,
>;

/// Environment-specific dependencies used by runner orchestrators.
pub struct RunnerEnvironment<B, W, E, T, A, S> {
    manifest_fetcher: ManifestFetcher,
    dump_downloader: DumpDownloader,
    engine_builder: EngineBuilder<B, W, E, T, A, S>,
}

impl<B, W, E, T, A, S> Clone for RunnerEnvironment<B, W, E, T, A, S> {
    fn clone(&self) -> Self {
        Self {
            manifest_fetcher: Arc::clone(&self.manifest_fetcher),
            dump_downloader: Arc::clone(&self.dump_downloader),
            engine_builder: Arc::clone(&self.engine_builder),
        }
    }
}

impl<B, W, E, T, A, S> RunnerEnvironment<B, W, E, T, A, S>
where
    B: BootstrapPipeline,
    W: WindowPipeline,
    E: EventsPipeline,
    T: TokensPipeline,
    A: ApplyPipeline,
    S: StatusBus,
{
    pub fn new(
        manifest_fetcher: ManifestFetcher,
        dump_downloader: DumpDownloader,
        engine_builder: EngineBuilder<B, W, E, T, A, S>,
    ) -> Self {
        Self {
            manifest_fetcher,
            dump_downloader,
            engine_builder,
        }
    }

    pub async fn fetch_manifests(
        &self,
        raindexes: &HashMap<String, RaindexCfg>,
    ) -> Result<ManifestMap, LocalDbError> {
        (self.manifest_fetcher)(raindexes).await
    }

    pub async fn download_dump(&self, url: &Url) -> Result<String, LocalDbError> {
        (self.dump_downloader)(url).await
    }

    pub fn build_engine(
        &self,
        target: &RunnerTarget,
    ) -> Result<EnginePipelines<B, W, E, T, A, S>, LocalDbError> {
        (self.engine_builder)(target)
    }
}

pub struct EnginePipelines<B, W, E, T, A, S> {
    pub bootstrap: B,
    pub window: W,
    pub events: E,
    pub tokens: T,
    pub apply: A,
    pub status: S,
}

impl<B, W, E, T, A, S> EnginePipelines<B, W, E, T, A, S>
where
    B: BootstrapPipeline,
    W: WindowPipeline,
    E: EventsPipeline,
    T: TokensPipeline,
    A: ApplyPipeline,
    S: StatusBus,
{
    pub fn new(bootstrap: B, window: W, events: E, tokens: T, apply: A, status: S) -> Self {
        Self {
            bootstrap,
            window,
            events,
            tokens,
            apply,
            status,
        }
    }

    pub fn into_engine(self) -> SyncEngine<B, W, E, T, A, S> {
        SyncEngine::new(
            self.bootstrap,
            self.window,
            self.events,
            self.tokens,
            self.apply,
            self.status,
        )
    }
}

pub fn default_manifest_fetcher() -> ManifestFetcher {
    Arc::new(|raindexes: &HashMap<String, RaindexCfg>| {
        let raindexes = raindexes.clone();
        Box::pin(async move { get_manifests(&raindexes).await })
    })
}

pub fn default_dump_downloader() -> DumpDownloader {
    Arc::new(|url: &Url| {
        let url = url.clone();
        Box::pin(async move { download_and_gunzip(&url).await })
    })
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use crate::erc20::TokenInfo;
    use crate::local_db::fetch::FetchConfig;
    use crate::local_db::pipeline::adapters::apply::{ApplyPipeline, ApplyPipelineTargetInfo};
    use crate::local_db::pipeline::adapters::bootstrap::{
        BootstrapConfig, BootstrapPipeline, BootstrapState,
    };
    use crate::local_db::pipeline::engine::SyncInputs;
    use crate::local_db::pipeline::runner::utils::parse_runner_settings;
    use crate::local_db::pipeline::{FinalityConfig, SyncConfig, SyncPhase, WindowOverrides};
    use crate::local_db::query::sql_statement_batch::SqlStatementBatch;
    use crate::local_db::query::LocalDbQueryExecutor;
    use crate::local_db::{LocalDbError, RaindexIdentifier};
    use crate::rpc_client::LogEntryResponse;
    use alloy::primitives::{address, b256, Address, B256};
    use async_trait::async_trait;
    use raindex_app_settings::local_db_manifest::{DB_SCHEMA_VERSION, MANIFEST_VERSION};
    use raindex_app_settings::local_db_remotes::LocalDbRemoteCfg;
    use raindex_app_settings::raindex::RaindexCfg;
    use raindex_app_settings::spec_version::SpecVersion;
    use raindex_app_settings::yaml::default_document;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    #[cfg(not(target_family = "wasm"))]
    use flate2::write::GzEncoder;
    #[cfg(not(target_family = "wasm"))]
    use httpmock::prelude::*;
    #[cfg(not(target_family = "wasm"))]
    use std::io::Write;

    fn block_on<F: Future>(future: F) -> F::Output {
        Runtime::new().expect("runtime").block_on(future)
    }

    fn sample_target() -> RunnerTarget {
        let fetch = FetchConfig::new(1, 1, 1, 1, 0, 0).expect("fetch config");
        RunnerTarget {
            raindex_key: "sample".to_string(),
            manifest_url: Url::parse("https://example.com/manifest.yaml").unwrap(),
            network_key: "network-a".to_string(),
            inputs: SyncInputs {
                raindex_id: RaindexIdentifier {
                    chain_id: 1,
                    raindex_address: address!("0000000000000000000000000000000000000001"),
                },
                metadata_rpcs: vec![Url::parse("https://rpc.example.com").unwrap()],
                cfg: SyncConfig {
                    deployment_block: 0,
                    fetch,
                    finality: FinalityConfig { depth: 0 },
                    window_overrides: WindowOverrides::default(),
                    max_blocks_per_cycle: None,
                },
                dump_str: None,
                defer_analyze: false,
                block_number_threshold: 100,
                manifest_end_block: 1,
            },
        }
    }

    #[derive(Clone)]
    struct StubBootstrap {
        marker: &'static str,
    }

    #[derive(Clone)]
    struct StubWindow {
        marker: &'static str,
    }

    #[derive(Clone)]
    struct StubEvents {
        marker: &'static str,
    }

    #[derive(Clone)]
    struct StubTokens {
        marker: &'static str,
    }

    #[derive(Clone)]
    struct StubApply {
        marker: &'static str,
    }

    #[derive(Clone)]
    struct StubStatus {
        marker: &'static str,
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
            _raindex_id: &RaindexIdentifier,
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
            _db: &DB,
            _config: &BootstrapConfig,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
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

    #[async_trait(?Send)]
    impl WindowPipeline for StubWindow {
        async fn compute<DB>(
            &self,
            _db: &DB,
            _target: &RaindexIdentifier,
            _cfg: &SyncConfig,
            latest_block: u64,
        ) -> Result<(u64, u64), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            Ok((0, latest_block))
        }
    }

    #[async_trait(?Send)]
    impl EventsPipeline for StubEvents {
        async fn latest_block(&self) -> Result<u64, LocalDbError> {
            Ok(0)
        }

        async fn fetch_raindex(
            &self,
            _raindex_address: Address,
            _from_block: u64,
            _to_block: u64,
            _cfg: &crate::local_db::FetchConfig,
        ) -> Result<Vec<LogEntryResponse>, LocalDbError> {
            Ok(Vec::new())
        }

        async fn fetch_stores(
            &self,
            _store_addresses: &[Address],
            _from_block: u64,
            _to_block: u64,
            _cfg: &crate::local_db::FetchConfig,
        ) -> Result<Vec<LogEntryResponse>, LocalDbError> {
            Ok(Vec::new())
        }

        fn decode(
            &self,
            _logs: &[LogEntryResponse],
        ) -> Result<
            Vec<crate::local_db::decode::DecodedEventData<crate::local_db::decode::DecodedEvent>>,
            LocalDbError,
        > {
            Ok(Vec::new())
        }

        async fn block_hash(&self, _block_number: u64) -> Result<B256, LocalDbError> {
            Ok(b256!(
                "0x0000000000000000000000000000000000000000000000000000000000000000"
            ))
        }
    }

    #[async_trait(?Send)]
    impl TokensPipeline for StubTokens {
        async fn load_existing<DB>(
            &self,
            _db: &DB,
            _raindex_id: &RaindexIdentifier,
            _token_addrs_lower: &[Address],
        ) -> Result<
            Vec<crate::local_db::query::fetch_erc20_tokens_by_addresses::Erc20TokenRow>,
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
            _cfg: &crate::local_db::FetchConfig,
        ) -> Result<Vec<(Address, TokenInfo)>, LocalDbError> {
            Ok(Vec::new())
        }
    }

    #[async_trait(?Send)]
    impl ApplyPipeline for StubApply {
        fn build_batch(
            &self,
            _target_info: &ApplyPipelineTargetInfo,
            _raw_logs: &[LogEntryResponse],
            _decoded_events: &[crate::local_db::decode::DecodedEventData<
                crate::local_db::decode::DecodedEvent,
            >],
            _existing_tokens: &[crate::local_db::query::fetch_erc20_tokens_by_addresses::Erc20TokenRow],
            _tokens_to_upsert: &[(Address, TokenInfo)],
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
            Ok(())
        }
    }

    #[async_trait(?Send)]
    impl StatusBus for StubStatus {
        async fn send(&self, _phase: SyncPhase) -> Result<(), LocalDbError> {
            Ok(())
        }
    }

    #[test]
    fn fetch_manifests_uses_environment_fetcher() {
        let counter = Arc::new(AtomicUsize::new(0));
        let manifest_map = Arc::new(HashMap::new());
        let counter_fetcher = counter.clone();
        let fetcher: ManifestFetcher = Arc::new(move |_raindexes| {
            let counter = counter_fetcher.clone();
            let manifest_map = Arc::clone(&manifest_map);
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok((*manifest_map).clone())
            })
        });

        let downloader: DumpDownloader =
            Arc::new(|_url| Box::pin(async { Ok("dump".to_string()) }));

        let environment = RunnerEnvironment::new(
            fetcher,
            downloader,
            Arc::new(|_target: &RunnerTarget| {
                Ok(EnginePipelines::new(
                    StubBootstrap { marker: "b" },
                    StubWindow { marker: "w" },
                    StubEvents { marker: "e" },
                    StubTokens { marker: "t" },
                    StubApply { marker: "a" },
                    StubStatus { marker: "s" },
                ))
            }),
        );

        let raindexes: HashMap<String, RaindexCfg> = HashMap::new();
        let result = block_on(environment.fetch_manifests(&raindexes)).expect("manifest map");
        assert!(result.is_empty());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn fetch_manifests_propagates_errors() {
        let fetcher: ManifestFetcher = Arc::new(|_raindexes| {
            Box::pin(async { Err(LocalDbError::HttpStatus { status: 500 }) })
        });
        let downloader: DumpDownloader = Arc::new(|_url| Box::pin(async { Ok(String::new()) }));
        let environment = RunnerEnvironment::new(
            fetcher,
            downloader,
            Arc::new(|_target: &RunnerTarget| {
                Ok(EnginePipelines::new(
                    StubBootstrap { marker: "b" },
                    StubWindow { marker: "w" },
                    StubEvents { marker: "e" },
                    StubTokens { marker: "t" },
                    StubApply { marker: "a" },
                    StubStatus { marker: "s" },
                ))
            }),
        );
        let raindexes: HashMap<String, RaindexCfg> = HashMap::new();
        let err = block_on(environment.fetch_manifests(&raindexes)).unwrap_err();
        match err {
            LocalDbError::HttpStatus { status } => assert_eq!(status, 500),
            other => panic!("unexpected error {other:?}"),
        }
    }

    #[test]
    fn download_dump_uses_environment_downloader() {
        let fetcher: ManifestFetcher =
            Arc::new(|_raindexes| Box::pin(async { Ok(HashMap::new()) }));
        let downloader: DumpDownloader =
            Arc::new(|_url| Box::pin(async { Ok("hello".to_string()) }));
        let environment = RunnerEnvironment::new(
            fetcher,
            downloader,
            Arc::new(|_target: &RunnerTarget| {
                Ok(EnginePipelines::new(
                    StubBootstrap { marker: "b" },
                    StubWindow { marker: "w" },
                    StubEvents { marker: "e" },
                    StubTokens { marker: "t" },
                    StubApply { marker: "a" },
                    StubStatus { marker: "s" },
                ))
            }),
        );
        let contents =
            block_on(environment.download_dump(&Url::parse("https://example.com").unwrap()))
                .expect("dump");
        assert_eq!(contents, "hello");
    }

    #[test]
    fn download_dump_propagates_errors() {
        let fetcher: ManifestFetcher =
            Arc::new(|_raindexes| Box::pin(async { Ok(HashMap::new()) }));
        let downloader: DumpDownloader = Arc::new(|_url| {
            Box::pin(async {
                Err(LocalDbError::IoError(std::io::Error::other(
                    "download failed",
                )))
            })
        });
        let environment = RunnerEnvironment::new(
            fetcher,
            downloader,
            Arc::new(|_target: &RunnerTarget| {
                Ok(EnginePipelines::new(
                    StubBootstrap { marker: "b" },
                    StubWindow { marker: "w" },
                    StubEvents { marker: "e" },
                    StubTokens { marker: "t" },
                    StubApply { marker: "a" },
                    StubStatus { marker: "s" },
                ))
            }),
        );
        let err = block_on(environment.download_dump(&Url::parse("https://example.com").unwrap()))
            .unwrap_err();
        assert!(matches!(err, LocalDbError::IoError(_)));
    }

    #[test]
    fn build_engine_returns_pipelines() {
        let marker = Arc::new(AtomicUsize::new(0));
        let marker_clone = marker.clone();
        let environment = RunnerEnvironment::new(
            Arc::new(|_raindexes| Box::pin(async { Ok(HashMap::new()) })),
            Arc::new(|_url| Box::pin(async { Ok("dump".into()) })),
            Arc::new(move |_target: &RunnerTarget| {
                marker_clone.fetch_add(1, Ordering::SeqCst);
                Ok(EnginePipelines::new(
                    StubBootstrap {
                        marker: "bootstrap",
                    },
                    StubWindow { marker: "window" },
                    StubEvents { marker: "events" },
                    StubTokens { marker: "tokens" },
                    StubApply { marker: "apply" },
                    StubStatus { marker: "status" },
                ))
            }),
        );

        let pipelines = environment
            .build_engine(&sample_target())
            .expect("pipelines");
        assert_eq!(pipelines.bootstrap.marker, "bootstrap");
        assert_eq!(pipelines.window.marker, "window");
        assert_eq!(pipelines.events.marker, "events");
        assert_eq!(pipelines.tokens.marker, "tokens");
        assert_eq!(pipelines.apply.marker, "apply");
        assert_eq!(pipelines.status.marker, "status");
        assert_eq!(marker.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn build_engine_propagates_errors() {
        let builder: EngineBuilder<
            StubBootstrap,
            StubWindow,
            StubEvents,
            StubTokens,
            StubApply,
            StubStatus,
        > = Arc::new(
            |_target: &RunnerTarget| -> Result<
                EnginePipelines<
                    StubBootstrap,
                    StubWindow,
                    StubEvents,
                    StubTokens,
                    StubApply,
                    StubStatus,
                >,
                LocalDbError,
            > { Err(LocalDbError::InvalidBootstrapImplementation) },
        );

        let environment: RunnerEnvironment<
            StubBootstrap,
            StubWindow,
            StubEvents,
            StubTokens,
            StubApply,
            StubStatus,
        > = RunnerEnvironment::new(
            Arc::new(|_raindexes| Box::pin(async { Ok(HashMap::new()) })),
            Arc::new(|_url| Box::pin(async { Ok(String::new()) })),
            builder,
        );
        let result = environment.build_engine(&sample_target());
        assert!(
            matches!(result, Err(LocalDbError::InvalidBootstrapImplementation)),
            "expected InvalidBootstrapImplementation"
        );
    }

    #[test]
    fn runner_environment_clone_shares_dependencies() {
        let fetch_counter = Arc::new(AtomicUsize::new(0));
        let fetch_counter_clone = fetch_counter.clone();
        let fetcher: ManifestFetcher = Arc::new(move |_raindexes| {
            let counter = fetch_counter_clone.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(HashMap::new())
            })
        });
        let download_counter = Arc::new(AtomicUsize::new(0));
        let download_counter_clone = download_counter.clone();
        let downloader: DumpDownloader = Arc::new(move |_url| {
            let counter = download_counter_clone.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(String::new())
            })
        });

        let environment = RunnerEnvironment::new(
            fetcher,
            downloader,
            Arc::new(|_target: &RunnerTarget| {
                Ok(EnginePipelines::new(
                    StubBootstrap { marker: "b" },
                    StubWindow { marker: "w" },
                    StubEvents { marker: "e" },
                    StubTokens { marker: "t" },
                    StubApply { marker: "a" },
                    StubStatus { marker: "s" },
                ))
            }),
        );
        let clone = environment.clone();

        block_on(environment.fetch_manifests(&HashMap::new())).unwrap();
        block_on(clone.fetch_manifests(&HashMap::new())).unwrap();
        assert_eq!(fetch_counter.load(Ordering::SeqCst), 2);

        block_on(environment.download_dump(&Url::parse("https://example.com").unwrap())).unwrap();
        block_on(clone.download_dump(&Url::parse("https://example.com").unwrap())).unwrap();
        assert_eq!(download_counter.load(Ordering::SeqCst), 2);
    }

    #[cfg(not(target_family = "wasm"))]
    fn sample_settings_yaml() -> String {
        format!(
            r#"
version: {version}
networks:
  network-a:
    rpcs:
      - https://rpc.network/a
    chain-id: 1
subgraphs:
  network-a: https://subgraph.network/a
local-db-remotes:
  remote-a: https://remotes.example.com/a.yaml
local-db-sync:
  network-a:
    batch-size: 10
    max-concurrent-batches: 5
    retry-attempts: 3
    retry-delay-ms: 100
    rate-limit-delay-ms: 50
    finality-depth: 12
    bootstrap-block-threshold: 10000
    sync-interval-ms: 5000
raindexes:
  raindex-a:
    address: 0x00000000000000000000000000000000000000a1
    network: network-a
    subgraph: network-a
    local-db-remote: remote-a
    deployment-block: 111
"#,
            version = SpecVersion::current()
        )
    }

    #[cfg(not(target_family = "wasm"))]
    fn update_remote_url(raindexes: &mut HashMap<String, RaindexCfg>, key: &str, url: &Url) {
        if let Some(raindex) = raindexes.get_mut(key) {
            let remote = LocalDbRemoteCfg {
                document: default_document(),
                key: raindex
                    .local_db_remote
                    .as_ref()
                    .expect("raindex has remote")
                    .key
                    .clone(),
                url: url.clone(),
            };
            raindex.local_db_remote = Some(Arc::new(remote));
        }
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn default_manifest_fetcher_uses_remotes() {
        let server = MockServer::start();
        let manifest_yaml = format!(
            r#"
manifest-version: {version}
db-schema-version: {db_schema_version}
networks:
  mainnet:
    chain-id: 1
    raindexes:
      - address: "0x00000000000000000000000000000000000000a1"
        dump-url: "{base}/dump.sql.gz"
        end-block: 123
        end-block-hash: "0x01"
        end-block-time-ms: 1000
"#,
            version = MANIFEST_VERSION,
            db_schema_version = DB_SCHEMA_VERSION,
            base = server.base_url()
        );

        server.mock(|when, then| {
            when.method(GET).path("/");
            then.status(200)
                .header("content-type", "application/x-yaml")
                .body(manifest_yaml.clone());
        });

        let mut parsed = parse_runner_settings(&sample_settings_yaml()).expect("sample settings");
        let manifest_url = Url::parse(&server.base_url()).unwrap();
        update_remote_url(&mut parsed.raindexes, "raindex-a", &manifest_url);

        let fetcher = default_manifest_fetcher();
        let manifests = block_on(fetcher(&parsed.raindexes)).expect("manifest map");
        assert_eq!(manifests.len(), 1);
        assert!(manifests.contains_key(&manifest_url));
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn default_dump_downloader_fetches_gzip() {
        let server = MockServer::start();
        let body_text = "CREATE TABLE test (id INTEGER PRIMARY KEY);";
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(body_text.as_bytes())
            .expect("gzip encode");
        let gzipped = encoder.finish().expect("gzip finish");

        server.mock(|when, then| {
            when.method(GET).path("/dump.sql.gz");
            then.status(200)
                .header("content-type", "application/gzip")
                .body(gzipped.clone());
        });

        let downloader = default_dump_downloader();
        let url = Url::parse(&format!("{}/dump.sql.gz", server.base_url())).unwrap();
        let contents = block_on(downloader(&url)).expect("contents");
        assert_eq!(contents, body_text);
    }

    #[test]
    fn engine_pipelines_into_engine_succeeds() {
        let pipelines = EnginePipelines::new(
            StubBootstrap { marker: "b" },
            StubWindow { marker: "w" },
            StubEvents { marker: "e" },
            StubTokens { marker: "t" },
            StubApply { marker: "a" },
            StubStatus { marker: "s" },
        );

        let _engine = pipelines.into_engine();
    }
}
