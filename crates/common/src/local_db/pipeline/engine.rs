use super::adapters::apply::{ApplyPipeline, ApplyPipelineTargetInfo};
use super::adapters::bootstrap::{BootstrapConfig, BootstrapPipeline};
use super::adapters::window::requested_end_block;
use super::{
    EventsPipeline, StatusBus, SyncConfig, SyncOutcome, SyncPhase, TokensPipeline, WindowPipeline,
};
use crate::erc20::TokenInfo;
use crate::local_db::address_collectors::{collect_store_addresses, collect_token_addresses};
use crate::local_db::decode::{
    sort_decoded_events_by_block_and_log, DecodedEvent, DecodedEventData,
};
use crate::local_db::query::fetch_erc20_tokens_by_addresses::Erc20TokenRow;
use crate::local_db::query::fetch_store_addresses::{fetch_store_addresses_stmt, StoreAddressRow};
use crate::local_db::query::LocalDbQueryExecutor;
#[cfg(not(target_family = "wasm"))]
use crate::local_db::query::{SqlStatement, SqlStatementBatch};
use crate::local_db::{LocalDbError, RaindexIdentifier};
use crate::rpc_client::LogEntryResponse;
use alloy::primitives::Address;
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;
use url::Url;

/// Generic engine that orchestrates a full sync cycle by delegating
/// environment-specific behavior to the supplied pipeline adapters.
pub struct SyncEngine<B, W, E, T, A, S> {
    bootstrap: B,
    window: W,
    events: E,
    tokens: T,
    apply: A,
    status: S,
}

/// Runner-supplied inputs that are static for the duration of a sync cycle.
#[derive(Debug, Clone)]
pub struct SyncInputs {
    pub raindex_id: RaindexIdentifier,
    pub metadata_rpcs: Vec<Url>,
    pub cfg: SyncConfig,
    pub dump_str: Option<Arc<String>>,
    /// The browser runner will analyze once after all dump targets finish.
    pub defer_analyze: bool,
    pub block_number_threshold: u32,
    pub manifest_end_block: u64,
}

impl<B, W, E, T, A, S> SyncEngine<B, W, E, T, A, S>
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

    /// Executes a single sync cycle using the supplied adapters.
    pub async fn run<DB>(&self, db: &DB, input: &SyncInputs) -> Result<SyncOutcome, LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        let latest_block = self.bootstrap_phase(db, input).await?;
        let (start_block, target_block) = self.compute_window(db, input, latest_block).await?;
        let caught_up = target_block >= requested_end_block(&input.cfg, latest_block);
        if start_block > target_block {
            self.status.send(SyncPhase::Idle).await?;
            return Ok(SyncOutcome {
                raindex_id: input.raindex_id.clone(),
                start_block,
                target_block,
                fetched_logs: 0,
                caught_up: true,
                decoded_events: 0,
            });
        }

        let (raindex_logs, decoded_raindex_events) = self
            .gather_raindex(input, start_block, target_block)
            .await?;
        let mut all_raw_logs = raindex_logs;
        let mut decoded_events = decoded_raindex_events;

        let (store_logs, mut decoded_store_events, existing_tokens, token_addresses) = self
            .load_store_logs_and_existing_tokens(
                db,
                input,
                start_block,
                target_block,
                &decoded_events,
            )
            .await?;

        if !store_logs.is_empty() {
            all_raw_logs.extend(store_logs);
            decoded_events.append(&mut decoded_store_events);
            sort_decoded_events_by_block_and_log(&mut decoded_events);
        }

        let tokens_to_upsert = self
            .resolve_token_upserts(input, &token_addresses, &existing_tokens)
            .await?;

        let target_hash = self.events.block_hash(target_block).await?;

        self.apply_changes(
            db,
            ApplyContext {
                target_info: &ApplyPipelineTargetInfo {
                    raindex_id: input.raindex_id.clone(),
                    start_block,
                    target_block,
                    defer_analyze: input.defer_analyze,
                    hash: target_hash,
                },
                raw_logs: &all_raw_logs,
                decoded_events: &decoded_events,
                existing_tokens: &existing_tokens,
                tokens_to_upsert: &tokens_to_upsert,
            },
        )
        .await?;

        Ok(SyncOutcome {
            raindex_id: input.raindex_id.clone(),
            start_block,
            target_block,
            fetched_logs: all_raw_logs.len(),
            caught_up,
            decoded_events: decoded_events.len(),
        })
    }
}

impl<B, W, E, T, A, S> SyncEngine<B, W, E, T, A, S>
where
    B: BootstrapPipeline,
    W: WindowPipeline,
    E: EventsPipeline,
    T: TokensPipeline,
    A: ApplyPipeline,
    S: StatusBus,
{
    async fn bootstrap_phase<DB>(&self, db: &DB, input: &SyncInputs) -> Result<u64, LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        self.status.send(SyncPhase::FetchingLatestBlock).await?;
        let latest_block = self.events.latest_block().await?;

        self.status.send(SyncPhase::RunningBootstrap).await?;
        self.bootstrap
            .engine_run(
                db,
                &BootstrapConfig {
                    raindex_id: input.raindex_id.clone(),
                    dump_sql: input.dump_str.clone(),
                    #[cfg(target_family = "wasm")]
                    dump_stmt: None,
                    #[cfg(not(target_family = "wasm"))]
                    dump_stmt: input.dump_str.as_ref().map(|value| {
                        let mut batch_stmt = SqlStatementBatch::new();
                        for line in value.lines() {
                            batch_stmt.add(SqlStatement::new(line));
                        }
                        batch_stmt
                    }),
                    block_number_threshold: input.block_number_threshold,
                    latest_block: input.manifest_end_block,
                    deployment_block: input.cfg.deployment_block,
                },
            )
            .await?;

        Ok(latest_block)
    }

    async fn compute_window<DB>(
        &self,
        db: &DB,
        input: &SyncInputs,
        latest_block: u64,
    ) -> Result<(u64, u64), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        self.status.send(SyncPhase::ComputingSyncWindow).await?;
        self.window
            .compute(db, &input.raindex_id, &input.cfg, latest_block)
            .await
    }

    async fn gather_raindex(
        &self,
        input: &SyncInputs,
        start_block: u64,
        target_block: u64,
    ) -> Result<(Vec<LogEntryResponse>, Vec<DecodedEventData<DecodedEvent>>), LocalDbError> {
        self.status.send(SyncPhase::FetchingRaindexLogs).await?;
        let raindex_logs = self
            .events
            .fetch_raindex(
                input.raindex_id.raindex_address,
                start_block,
                target_block,
                &input.cfg.fetch,
            )
            .await?;

        self.status.send(SyncPhase::DecodingRaindexLogs).await?;
        let decoded_events = self.events.decode(&raindex_logs)?;

        Ok((raindex_logs, decoded_events))
    }

    async fn load_store_logs_and_existing_tokens<DB>(
        &self,
        db: &DB,
        input: &SyncInputs,
        start_block: u64,
        target_block: u64,
        decoded_events: &[DecodedEventData<DecodedEvent>],
    ) -> Result<
        (
            Vec<LogEntryResponse>,
            Vec<DecodedEventData<DecodedEvent>>,
            Vec<Erc20TokenRow>,
            BTreeSet<Address>,
        ),
        LocalDbError,
    >
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        let mut store_addresses = collect_store_addresses(decoded_events);
        let existing_store_rows = load_known_store_addresses(db, &input.raindex_id).await?;
        for row in existing_store_rows {
            if !row.store_address.is_zero() {
                store_addresses.insert(row.store_address);
            }
        }

        let token_addresses: BTreeSet<Address> = collect_token_addresses(decoded_events);
        let token_addresses_vec: Vec<Address> = token_addresses.iter().copied().collect();

        let tokens_fut = self
            .tokens
            .load_existing(db, &input.raindex_id, &token_addresses_vec);

        let store_addresses_vec: Vec<Address> = store_addresses.into_iter().collect();
        let store_fetch_fut = async {
            if store_addresses_vec.is_empty() {
                Ok::<(Vec<LogEntryResponse>, Vec<DecodedEventData<DecodedEvent>>), LocalDbError>((
                    Vec::new(),
                    Vec::new(),
                ))
            } else {
                self.status.send(SyncPhase::FetchingStoreLogs).await?;
                let logs = self
                    .events
                    .fetch_stores(
                        &store_addresses_vec,
                        start_block,
                        target_block,
                        &input.cfg.fetch,
                    )
                    .await?;
                self.status.send(SyncPhase::DecodingStoreLogs).await?;
                let decoded = self.events.decode(&logs)?;
                Ok((logs, decoded))
            }
        };

        let (existing_tokens_res, store_pair_res) = tokio::join!(tokens_fut, store_fetch_fut);
        let existing_tokens = existing_tokens_res?;
        let (store_logs, decoded_store_events) = store_pair_res?;

        Ok((
            store_logs,
            decoded_store_events,
            existing_tokens,
            token_addresses,
        ))
    }

    async fn resolve_token_upserts(
        &self,
        input: &SyncInputs,
        token_addresses: &BTreeSet<Address>,
        existing_tokens: &[Erc20TokenRow],
    ) -> Result<Vec<(Address, TokenInfo)>, LocalDbError> {
        let existing_set = existing_tokens
            .iter()
            .map(|row| row.token_address)
            .collect::<HashSet<Address>>();
        let missing_tokens = token_addresses
            .iter()
            .copied()
            .filter(|addr| !existing_set.contains(addr))
            .collect::<Vec<Address>>();

        if missing_tokens.is_empty() {
            return Ok(Vec::new());
        }

        self.status.send(SyncPhase::FetchingTokenMetadata).await?;
        self.tokens
            .fetch_missing(missing_tokens, &input.cfg.fetch)
            .await
    }

    async fn apply_changes<DB>(&self, db: &DB, ctx: ApplyContext<'_>) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        self.status.send(SyncPhase::BuildingSqlBatch).await?;
        let batch = self.apply.build_batch(
            ctx.target_info,
            ctx.raw_logs,
            ctx.decoded_events,
            ctx.existing_tokens,
            ctx.tokens_to_upsert,
        )?;

        self.status.send(SyncPhase::PersistingToDatabase).await?;
        self.apply.persist(db, &batch).await?;

        self.status.send(SyncPhase::RunningPostSyncExport).await?;
        self.apply
            .export_dump(
                db,
                &ctx.target_info.raindex_id,
                ctx.target_info.target_block,
            )
            .await
    }
}

struct ApplyContext<'a> {
    target_info: &'a ApplyPipelineTargetInfo,
    raw_logs: &'a [LogEntryResponse],
    decoded_events: &'a [DecodedEventData<DecodedEvent>],
    existing_tokens: &'a [Erc20TokenRow],
    tokens_to_upsert: &'a [(Address, TokenInfo)],
}

async fn load_known_store_addresses<DB>(
    db: &DB,
    raindex_id: &RaindexIdentifier,
) -> Result<Vec<StoreAddressRow>, LocalDbError>
where
    DB: LocalDbQueryExecutor + ?Sized,
{
    db.query_json_retryable(&fetch_store_addresses_stmt(raindex_id))
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::erc20::TokenInfo;
    use crate::local_db::decode::{
        DecodedEvent, DecodedEventData, EventType, InterpreterStoreSetEvent,
    };
    use crate::local_db::pipeline::adapters::apply::ApplyPipelineTargetInfo;
    use crate::local_db::pipeline::adapters::bootstrap::BootstrapState;
    use crate::local_db::pipeline::{FinalityConfig, SyncPhase, WindowOverrides};
    use crate::local_db::query::{
        fetch_erc20_tokens_by_addresses::Erc20TokenRow, fetch_store_addresses::StoreAddressRow,
        LocalDbQueryError, SqlStatement, SqlStatementBatch, SqlValue,
    };
    use crate::local_db::FetchConfig;
    use alloy::primitives::{b256, Address, Bytes, FixedBytes, B256, U256};
    use async_trait::async_trait;
    use raindex_bindings::IInterpreterStoreV3::Set;
    use serde_json;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use tokio::sync::Barrier;
    use url::Url;

    fn hex_u64(value: u64) -> String {
        format!("0x{value:x}")
    }

    fn addr(byte: u8) -> Address {
        Address::from([byte; 20])
    }

    fn tx_hash(tag: u8) -> B256 {
        let mut bytes = [0u8; 32];
        bytes[31] = tag;
        B256::from(bytes)
    }

    fn log_entry(address: Address, block: u64, log_index: u64, tx: u8) -> LogEntryResponse {
        LogEntryResponse {
            address,
            topics: vec![Bytes::from(vec![0x1; 32])],
            data: Bytes::from(vec![0x01]),
            block_number: U256::from(block),
            block_timestamp: Some(U256::from(block + 100)),
            transaction_hash: tx_hash(tx),
            transaction_index: hex_u64(0),
            block_hash: B256::from([0xde; 32]),
            log_index: U256::from(log_index),
            removed: false,
        }
    }

    fn deposit_event(
        block: u64,
        log_index: u64,
        token: Address,
        tx: u8,
    ) -> DecodedEventData<DecodedEvent> {
        use raindex_bindings::IRaindexV7::DepositV2;
        DecodedEventData {
            event_type: EventType::DepositV2,
            block_number: U256::from(block),
            block_timestamp: U256::from(block + 100),
            transaction_hash: tx_hash(tx),
            log_index: U256::from(log_index),
            decoded_data: DecodedEvent::DepositV2(Box::new(DepositV2 {
                sender: addr(0x33),
                token,
                vaultId: U256::from(1u64).into(),
                depositAmountUint256: U256::from(100),
            })),
        }
    }

    fn add_order_event(
        block: u64,
        log_index: u64,
        store: Address,
        input_token: Address,
        output_token: Address,
        tx: u8,
    ) -> DecodedEventData<DecodedEvent> {
        use raindex_bindings::IRaindexV7::{AddOrderV3, EvaluableV4, OrderV4, IOV2};
        DecodedEventData {
            event_type: EventType::AddOrderV3,
            block_number: U256::from(block),
            block_timestamp: U256::from(block + 200),
            transaction_hash: tx_hash(tx),
            log_index: U256::from(log_index),
            decoded_data: DecodedEvent::AddOrderV3(Box::new(AddOrderV3 {
                sender: addr(0x44),
                orderHash: FixedBytes::from([0u8; 32]),
                order: OrderV4 {
                    owner: addr(0x55),
                    nonce: U256::from(9u64).into(),
                    evaluable: EvaluableV4 {
                        interpreter: addr(0x66),
                        store,
                        bytecode: Bytes::from(vec![]),
                    },
                    validInputs: vec![IOV2 {
                        token: input_token,
                        vaultId: U256::from(7u64).into(),
                    }],
                    validOutputs: vec![IOV2 {
                        token: output_token,
                        vaultId: U256::from(8u64).into(),
                    }],
                },
            })),
        }
    }

    fn store_set_event(
        block: u64,
        log_index: u64,
        store: Address,
        tx: u8,
    ) -> DecodedEventData<DecodedEvent> {
        DecodedEventData {
            event_type: EventType::InterpreterStoreSet,
            block_number: U256::from(block),
            block_timestamp: U256::from(block + 300),
            transaction_hash: tx_hash(tx),
            log_index: U256::from(log_index),
            decoded_data: DecodedEvent::InterpreterStoreSet(Box::new(InterpreterStoreSetEvent {
                store_address: store,
                payload: Set {
                    namespace: U256::from(0x1234),
                    key: FixedBytes::<32>::from([0xBB; 32]),
                    value: FixedBytes::<32>::from([0xCC; 32]),
                },
            })),
        }
    }

    fn base_target() -> RaindexIdentifier {
        RaindexIdentifier {
            chain_id: 42161,
            raindex_address: addr(0xAB),
        }
    }

    fn base_inputs() -> SyncInputs {
        SyncInputs {
            raindex_id: base_target(),
            metadata_rpcs: vec![Url::parse("https://rpc.example.com").unwrap()],
            cfg: SyncConfig {
                deployment_block: 1,
                fetch: FetchConfig::default(),
                finality: FinalityConfig { depth: 0 },
                window_overrides: WindowOverrides::default(),
                max_blocks_per_cycle: None,
            },
            dump_str: None,
            defer_analyze: false,
            block_number_threshold: 10_000,
            manifest_end_block: 1,
        }
    }

    #[derive(Clone, Default)]
    struct MockStatusBus {
        inner: Arc<MockStatusInner>,
    }

    #[derive(Default)]
    struct MockStatusInner {
        phases: Mutex<Vec<SyncPhase>>,
        results: Mutex<VecDeque<Result<(), LocalDbError>>>,
    }

    impl MockStatusBus {
        fn phases(&self) -> Vec<SyncPhase> {
            self.inner.phases.lock().unwrap().clone()
        }

        fn set_results(&self, results: Vec<Result<(), LocalDbError>>) {
            *self.inner.results.lock().unwrap() = VecDeque::from(results);
        }
    }

    #[async_trait(?Send)]
    impl StatusBus for MockStatusBus {
        async fn send(&self, phase: SyncPhase) -> Result<(), LocalDbError> {
            self.inner.phases.lock().unwrap().push(phase);
            if let Some(result) = self.inner.results.lock().unwrap().pop_front() {
                result
            } else {
                Ok(())
            }
        }
    }

    #[derive(Clone, Default)]
    struct MockBootstrap {
        inner: Arc<MockBootstrapInner>,
    }

    #[derive(Default)]
    struct MockBootstrapInner {
        configs: Mutex<Vec<BootstrapConfig>>,
        results: Mutex<VecDeque<Result<(), LocalDbError>>>,
    }

    impl MockBootstrap {
        fn configs(&self) -> Vec<BootstrapConfig> {
            self.inner.configs.lock().unwrap().clone()
        }
    }

    #[async_trait(?Send)]
    impl BootstrapPipeline for MockBootstrap {
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
            config: &BootstrapConfig,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            self.inner.configs.lock().unwrap().push(config.clone());
            self.inner
                .results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(()))
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

    #[derive(Clone, Default)]
    struct MockWindow {
        inner: Arc<MockWindowInner>,
    }

    #[derive(Default)]
    struct MockWindowInner {
        calls: Mutex<Vec<(RaindexIdentifier, u64, u64, u64)>>,
        results: Mutex<VecDeque<Result<(u64, u64), LocalDbError>>>,
    }

    impl MockWindow {
        fn set_results(&self, results: Vec<Result<(u64, u64), LocalDbError>>) {
            *self.inner.results.lock().unwrap() = VecDeque::from(results);
        }

        fn calls(&self) -> Vec<(RaindexIdentifier, u64, u64, u64)> {
            self.inner.calls.lock().unwrap().clone()
        }
    }

    #[async_trait(?Send)]
    impl WindowPipeline for MockWindow {
        async fn compute<DB>(
            &self,
            _db: &DB,
            raindex_id: &RaindexIdentifier,
            cfg: &SyncConfig,
            latest_block: u64,
        ) -> Result<(u64, u64), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            self.inner.calls.lock().unwrap().push((
                raindex_id.clone(),
                cfg.deployment_block,
                latest_block,
                cfg.finality.depth as u64,
            ));
            self.inner
                .results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok((cfg.deployment_block, latest_block)))
        }
    }

    #[derive(Clone, Default)]
    struct MockEvents {
        inner: Arc<MockEventsInner>,
    }

    #[derive(Default)]
    struct MockEventsInner {
        latest_blocks: Mutex<VecDeque<Result<u64, LocalDbError>>>,
        raindex_results: Mutex<VecDeque<Result<Vec<LogEntryResponse>, LocalDbError>>>,
        store_results: Mutex<VecDeque<Result<Vec<LogEntryResponse>, LocalDbError>>>,
        decode_results: Mutex<VecDeque<Result<Vec<DecodedEventData<DecodedEvent>>, LocalDbError>>>,
        block_hashes: Mutex<VecDeque<Result<B256, LocalDbError>>>,
        raindex_calls: Mutex<Vec<(Address, u64, u64)>>,
        store_calls: Mutex<Vec<(Vec<Address>, u64, u64)>>,
        store_barrier: Mutex<Option<Arc<Barrier>>>,
        store_completed: Mutex<bool>,
    }

    impl MockEvents {
        fn set_latest_blocks(&self, blocks: Vec<Result<u64, LocalDbError>>) {
            *self.inner.latest_blocks.lock().unwrap() = VecDeque::from(blocks);
        }

        fn push_raindex_result(&self, result: Result<Vec<LogEntryResponse>, LocalDbError>) {
            self.inner.raindex_results.lock().unwrap().push_back(result);
        }

        fn push_store_result(&self, result: Result<Vec<LogEntryResponse>, LocalDbError>) {
            self.inner.store_results.lock().unwrap().push_back(result);
        }

        fn push_decode_result(
            &self,
            result: Result<Vec<DecodedEventData<DecodedEvent>>, LocalDbError>,
        ) {
            self.inner.decode_results.lock().unwrap().push_back(result);
        }

        fn push_block_hash(&self, result: Result<B256, LocalDbError>) {
            self.inner.block_hashes.lock().unwrap().push_back(result);
        }

        fn raindex_calls(&self) -> Vec<(Address, u64, u64)> {
            self.inner.raindex_calls.lock().unwrap().clone()
        }

        fn store_calls(&self) -> Vec<(Vec<Address>, u64, u64)> {
            self.inner.store_calls.lock().unwrap().clone()
        }

        fn set_store_barrier(&self, barrier: Arc<Barrier>) {
            *self.inner.store_barrier.lock().unwrap() = Some(barrier);
        }

        fn store_completed(&self) -> bool {
            *self.inner.store_completed.lock().unwrap()
        }
    }

    #[async_trait(?Send)]
    impl EventsPipeline for MockEvents {
        async fn latest_block(&self) -> Result<u64, LocalDbError> {
            self.inner
                .latest_blocks
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(0))
        }

        async fn fetch_raindex(
            &self,
            raindex_address: Address,
            from_block: u64,
            to_block: u64,
            _cfg: &FetchConfig,
        ) -> Result<Vec<LogEntryResponse>, LocalDbError> {
            self.inner
                .raindex_calls
                .lock()
                .unwrap()
                .push((raindex_address, from_block, to_block));
            self.inner
                .raindex_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(Vec::new()))
        }

        async fn fetch_stores(
            &self,
            store_addresses: &[Address],
            from_block: u64,
            to_block: u64,
            _cfg: &FetchConfig,
        ) -> Result<Vec<LogEntryResponse>, LocalDbError> {
            self.inner.store_calls.lock().unwrap().push((
                store_addresses.to_vec(),
                from_block,
                to_block,
            ));
            let barrier = {
                let store_barrier = self.inner.store_barrier.lock().unwrap();
                store_barrier.clone()
            };
            if let Some(barrier) = barrier {
                barrier.wait().await;
            }
            *self.inner.store_completed.lock().unwrap() = true;
            self.inner
                .store_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(Vec::new()))
        }

        fn decode(
            &self,
            _logs: &[LogEntryResponse],
        ) -> Result<Vec<DecodedEventData<DecodedEvent>>, LocalDbError> {
            self.inner
                .decode_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(Vec::new()))
        }

        async fn block_hash(&self, _block_number: u64) -> Result<B256, LocalDbError> {
            self.inner
                .block_hashes
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(B256::ZERO))
        }
    }

    type TokenFetchResult = Result<Vec<(Address, TokenInfo)>, LocalDbError>;

    #[derive(Clone, Default)]
    struct MockTokens {
        inner: Arc<MockTokensInner>,
    }

    #[derive(Default)]
    struct MockTokensInner {
        load_results: Mutex<VecDeque<Result<Vec<Erc20TokenRow>, LocalDbError>>>,
        fetch_results: Mutex<VecDeque<TokenFetchResult>>,
        load_calls: Mutex<Vec<Vec<Address>>>,
        fetch_calls: Mutex<Vec<Vec<Address>>>,
        barrier: Mutex<Option<Arc<Barrier>>>,
        load_completed: Mutex<bool>,
    }

    impl MockTokens {
        fn set_load_existing_results(
            &self,
            results: Vec<Result<Vec<Erc20TokenRow>, LocalDbError>>,
        ) {
            *self.inner.load_results.lock().unwrap() = VecDeque::from(results);
        }

        fn set_fetch_missing_results(&self, results: Vec<TokenFetchResult>) {
            *self.inner.fetch_results.lock().unwrap() = VecDeque::from(results);
        }

        fn load_calls(&self) -> Vec<Vec<Address>> {
            self.inner.load_calls.lock().unwrap().clone()
        }

        fn fetch_calls(&self) -> Vec<Vec<Address>> {
            self.inner.fetch_calls.lock().unwrap().clone()
        }

        fn set_barrier(&self, barrier: Arc<Barrier>) {
            *self.inner.barrier.lock().unwrap() = Some(barrier);
        }

        fn load_completed(&self) -> bool {
            *self.inner.load_completed.lock().unwrap()
        }
    }

    #[async_trait(?Send)]
    impl TokensPipeline for MockTokens {
        async fn load_existing<DB>(
            &self,
            _db: &DB,
            _raindex_id: &RaindexIdentifier,
            token_addrs_lower: &[Address],
        ) -> Result<Vec<Erc20TokenRow>, LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            self.inner
                .load_calls
                .lock()
                .unwrap()
                .push(token_addrs_lower.to_vec());
            let barrier = {
                let barrier_guard = self.inner.barrier.lock().unwrap();
                barrier_guard.clone()
            };
            if let Some(barrier) = barrier {
                barrier.wait().await;
            }
            *self.inner.load_completed.lock().unwrap() = true;
            self.inner
                .load_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(Vec::new()))
        }

        async fn fetch_missing(
            &self,
            missing: Vec<Address>,
            _cfg: &FetchConfig,
        ) -> TokenFetchResult {
            self.inner.fetch_calls.lock().unwrap().push(missing);
            self.inner
                .fetch_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(Vec::new()))
        }
    }

    #[derive(Debug, Clone)]
    struct BuildCall {
        raw_order: Vec<(U256, U256)>,
        decoded_order: Vec<(U256, U256)>,
        existing_tokens: Vec<Address>,
        upsert_tokens: Vec<Address>,
        end_block_hash: B256,
    }

    #[derive(Clone, Default)]
    struct MockApply {
        inner: Arc<MockApplyInner>,
    }

    #[derive(Default)]
    struct MockApplyInner {
        build_results: Mutex<VecDeque<Result<SqlStatementBatch, LocalDbError>>>,
        persist_results: Mutex<VecDeque<Result<(), LocalDbError>>>,
        export_results: Mutex<VecDeque<Result<(), LocalDbError>>>,
        build_calls: Mutex<Vec<BuildCall>>,
        persist_calls: Mutex<Vec<SqlStatementBatch>>,
        export_calls: Mutex<Vec<(RaindexIdentifier, u64)>>,
    }

    impl MockApply {
        fn set_build_results(&self, results: Vec<Result<SqlStatementBatch, LocalDbError>>) {
            *self.inner.build_results.lock().unwrap() = VecDeque::from(results);
        }

        fn set_persist_results(&self, results: Vec<Result<(), LocalDbError>>) {
            *self.inner.persist_results.lock().unwrap() = VecDeque::from(results);
        }

        fn set_export_results(&self, results: Vec<Result<(), LocalDbError>>) {
            *self.inner.export_results.lock().unwrap() = VecDeque::from(results);
        }

        fn build_calls(&self) -> Vec<BuildCall> {
            self.inner.build_calls.lock().unwrap().clone()
        }

        fn persist_calls(&self) -> Vec<SqlStatementBatch> {
            self.inner.persist_calls.lock().unwrap().clone()
        }

        fn export_calls(&self) -> Vec<(RaindexIdentifier, u64)> {
            self.inner.export_calls.lock().unwrap().clone()
        }
    }

    #[async_trait(?Send)]
    impl ApplyPipeline for MockApply {
        fn build_batch(
            &self,
            target_info: &ApplyPipelineTargetInfo,
            raw_logs: &[LogEntryResponse],
            decoded_events: &[DecodedEventData<DecodedEvent>],
            existing_tokens: &[Erc20TokenRow],
            tokens_to_upsert: &[(Address, TokenInfo)],
        ) -> Result<SqlStatementBatch, LocalDbError> {
            let raw_order = raw_logs
                .iter()
                .map(|log| (log.block_number, log.log_index))
                .collect();
            let decoded_order = decoded_events
                .iter()
                .map(|evt| (evt.block_number, evt.log_index))
                .collect();
            let existing_tokens = existing_tokens
                .iter()
                .map(|row| row.token_address)
                .collect();
            let upsert_tokens = tokens_to_upsert.iter().map(|(addr, _)| *addr).collect();

            self.inner.build_calls.lock().unwrap().push(BuildCall {
                raw_order,
                decoded_order,
                existing_tokens,
                upsert_tokens,
                end_block_hash: target_info.hash,
            });

            self.inner
                .build_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(SqlStatementBatch::new()))
        }

        async fn persist<DB>(&self, _db: &DB, batch: &SqlStatementBatch) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            self.inner.persist_calls.lock().unwrap().push(batch.clone());
            self.inner
                .persist_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(()))
        }

        async fn export_dump<DB>(
            &self,
            _db: &DB,
            target: &RaindexIdentifier,
            end_block: u64,
        ) -> Result<(), LocalDbError>
        where
            DB: LocalDbQueryExecutor + ?Sized,
        {
            self.inner
                .export_calls
                .lock()
                .unwrap()
                .push((target.clone(), end_block));
            self.inner
                .export_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(()))
        }
    }

    #[derive(Default)]
    struct TestDb {
        store_responses: Mutex<VecDeque<Result<Vec<StoreAddressRow>, LocalDbQueryError>>>,
        query_statements: Mutex<Vec<SqlStatement>>,
        executed_batches: Mutex<Vec<SqlStatementBatch>>,
    }

    impl TestDb {
        fn set_store_responses(
            &self,
            responses: Vec<Result<Vec<StoreAddressRow>, LocalDbQueryError>>,
        ) {
            *self.store_responses.lock().unwrap() = VecDeque::from(responses);
        }
    }

    #[cfg_attr(target_family = "wasm", async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait)]
    impl LocalDbQueryExecutor for TestDb {
        async fn execute_batch(&self, batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            self.executed_batches.lock().unwrap().push(batch.clone());
            Ok(())
        }

        async fn query_json<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: crate::local_db::query::FromDbJson,
        {
            self.query_statements.lock().unwrap().push(stmt.clone());
            let response = self
                .store_responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(Vec::new()))?;
            let value = serde_json::to_value(response)
                .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))?;
            serde_json::from_value(value)
                .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))
        }

        async fn query_text(&self, _stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            Err(LocalDbQueryError::database(
                "query_text not supported in tests",
            ))
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::not_implemented("wipe_and_recreate"))
        }
    }

    struct EngineHarness {
        db: TestDb,
        bootstrap: MockBootstrap,
        window: MockWindow,
        events: MockEvents,
        tokens: MockTokens,
        apply: MockApply,
        status: MockStatusBus,
        engine:
            SyncEngine<MockBootstrap, MockWindow, MockEvents, MockTokens, MockApply, MockStatusBus>,
    }

    impl EngineHarness {
        fn new() -> Self {
            let bootstrap = MockBootstrap::default();
            let window = MockWindow::default();
            let events = MockEvents::default();
            let tokens = MockTokens::default();
            let apply = MockApply::default();
            let status = MockStatusBus::default();
            let engine = SyncEngine::new(
                bootstrap.clone(),
                window.clone(),
                events.clone(),
                tokens.clone(),
                apply.clone(),
                status.clone(),
            );
            Self {
                db: TestDb::default(),
                bootstrap,
                window,
                events,
                tokens,
                apply,
                status,
                engine,
            }
        }

        async fn run(&self, inputs: &SyncInputs) -> Result<SyncOutcome, LocalDbError> {
            self.engine.run(&self.db, inputs).await
        }
    }

    fn token_row(token: Address) -> Erc20TokenRow {
        Erc20TokenRow {
            chain_id: base_target().chain_id,
            raindex_address: base_target().raindex_address,
            token_address: token,
            name: "Token".into(),
            symbol: "TOK".into(),
            decimals: 18,
        }
    }

    fn token_info(symbol: &str) -> TokenInfo {
        TokenInfo {
            decimals: 18,
            name: format!("{symbol} Token"),
            symbol: symbol.to_string(),
        }
    }

    #[tokio::test]
    async fn run_full_cycle_success() {
        let harness = EngineHarness::new();
        let token_a = addr(0x01);
        let token_b = addr(0x02);
        let store = addr(0x90);
        let raindex_logs = vec![
            log_entry(base_target().raindex_address, 10, 1, 1),
            log_entry(base_target().raindex_address, 11, 0, 2),
        ];
        let store_logs = vec![log_entry(store, 12, 0, 3)];

        harness.events.set_latest_blocks(vec![Ok(12)]);
        harness.events.push_block_hash(Ok(b256!(
            "0x0000000000000000000000000000000000000000000000000000000000000001"
        )));
        harness.window.set_results(vec![Ok((10, 12))]);
        harness.events.push_raindex_result(Ok(raindex_logs.clone()));
        harness.events.push_decode_result(Ok(vec![
            deposit_event(10, 1, token_a, 1),
            add_order_event(11, 0, store, token_a, token_b, 2),
        ]));
        harness.events.push_store_result(Ok(store_logs.clone()));
        harness
            .events
            .push_decode_result(Ok(vec![store_set_event(12, 0, store, 3)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(vec![token_row(token_a)])]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(vec![(token_b, token_info("TOKB"))])]);

        let inputs = base_inputs();

        let outcome = harness.run(&inputs).await.expect("run succeeds");

        assert_eq!(outcome.raindex_id.chain_id, inputs.raindex_id.chain_id);
        assert_eq!(
            outcome.raindex_id.raindex_address,
            inputs.raindex_id.raindex_address
        );
        assert_eq!(outcome.start_block, 10);
        assert_eq!(outcome.target_block, 12);
        assert_eq!(outcome.fetched_logs, 3);
        assert!(outcome.caught_up, "window reached the safe head");
        assert_eq!(outcome.decoded_events, 3);

        let phases = harness.status.phases();
        assert_eq!(
            phases,
            vec![
                SyncPhase::FetchingLatestBlock,
                SyncPhase::RunningBootstrap,
                SyncPhase::ComputingSyncWindow,
                SyncPhase::FetchingRaindexLogs,
                SyncPhase::DecodingRaindexLogs,
                SyncPhase::FetchingStoreLogs,
                SyncPhase::DecodingStoreLogs,
                SyncPhase::FetchingTokenMetadata,
                SyncPhase::BuildingSqlBatch,
                SyncPhase::PersistingToDatabase,
                SyncPhase::RunningPostSyncExport,
            ]
        );

        let build_calls = harness.apply.build_calls();
        assert_eq!(build_calls.len(), 1);
        let build = &build_calls[0];
        assert_eq!(build.raw_order.len(), 3);
        assert_eq!(build.decoded_order.len(), 3);
        assert_eq!(
            build.decoded_order,
            vec![
                (U256::from(10), U256::from(1)),
                (U256::from(11), U256::from(0)),
                (U256::from(12), U256::from(0))
            ]
        );
        assert_eq!(build.existing_tokens, vec![token_a]);
        assert_eq!(build.upsert_tokens, vec![token_b]);
        assert_eq!(
            build.end_block_hash,
            b256!("0x0000000000000000000000000000000000000000000000000000000000000001")
        );

        assert_eq!(harness.apply.persist_calls().len(), 1);
        assert_eq!(harness.apply.export_calls().len(), 1);

        let fetch_calls = harness.tokens.fetch_calls();
        assert_eq!(fetch_calls.len(), 1);
        assert_eq!(fetch_calls[0], vec![token_b]);
    }

    #[tokio::test]
    async fn run_reports_not_caught_up_when_window_stops_short() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(100)]);
        harness.events.push_block_hash(Ok(b256!(
            "0x0000000000000000000000000000000000000000000000000000000000000001"
        )));
        // A capped window: 10..=20 while the safe head is 100.
        harness.window.set_results(vec![Ok((10, 20))]);
        harness.events.push_raindex_result(Ok(vec![]));
        harness.events.push_decode_result(Ok(vec![]));
        harness.tokens.set_load_existing_results(vec![Ok(vec![])]);

        let mut inputs = base_inputs();
        inputs.cfg.max_blocks_per_cycle = Some(11);

        let outcome = harness.run(&inputs).await.expect("run succeeds");

        assert_eq!((outcome.start_block, outcome.target_block), (10, 20));
        assert!(!outcome.caught_up);
    }

    #[tokio::test]
    async fn run_short_circuits_when_window_empty() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(100)]);
        harness.window.set_results(vec![Ok((15, 10))]);

        let outcome = harness.run(&base_inputs()).await.expect("run succeeds");
        assert_eq!(outcome.start_block, 15);
        assert_eq!(outcome.target_block, 10);
        assert_eq!(outcome.fetched_logs, 0);
        assert_eq!(outcome.decoded_events, 0);

        let phases = harness.status.phases();
        assert_eq!(
            phases,
            vec![
                SyncPhase::FetchingLatestBlock,
                SyncPhase::RunningBootstrap,
                SyncPhase::ComputingSyncWindow,
                SyncPhase::Idle,
            ]
        );

        assert!(harness.events.raindex_calls().is_empty());
        assert!(harness.apply.build_calls().is_empty());
    }

    #[tokio::test]
    async fn run_fetches_store_logs_when_addresses_present() {
        let harness = EngineHarness::new();
        let store = addr(0x50);
        harness.events.set_latest_blocks(vec![Ok(20)]);
        harness.window.set_results(vec![Ok((5, 6))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            5,
            0,
            1,
        )]));
        harness.events.push_decode_result(Ok(vec![add_order_event(
            5,
            0,
            store,
            addr(0x10),
            addr(0x11),
            1,
        )]));
        harness
            .events
            .push_store_result(Ok(vec![log_entry(store, 6, 0, 2)]));
        harness
            .events
            .push_decode_result(Ok(vec![store_set_event(6, 0, store, 2)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(Vec::new())]);

        harness.run(&base_inputs()).await.expect("run succeeds");

        let store_calls = harness.events.store_calls();
        assert_eq!(store_calls.len(), 1);
        let (addresses, start, end) = &store_calls[0];
        assert_eq!(*start, 5);
        assert_eq!(*end, 6);
        assert_eq!(addresses, &vec![store]);
    }

    #[tokio::test]
    async fn run_skips_store_fetch_when_no_addresses() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(8)]);
        harness.window.set_results(vec![Ok((2, 4))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            2,
            0,
            1,
        )]));
        harness
            .events
            .push_decode_result(Ok(vec![deposit_event(2, 0, addr(0x01), 1)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(Vec::new())]);

        harness.run(&base_inputs()).await.expect("run succeeds");

        assert!(harness.events.store_calls().is_empty());
    }

    #[tokio::test]
    async fn run_uses_known_store_addresses_and_ignores_zero() {
        let harness = EngineHarness::new();
        harness.db.set_store_responses(vec![Ok(vec![
            StoreAddressRow {
                store_address: Address::ZERO,
            },
            StoreAddressRow {
                store_address: addr(0xFE),
            },
        ])]);
        harness.events.set_latest_blocks(vec![Ok(9)]);
        harness.window.set_results(vec![Ok((3, 5))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            3,
            0,
            1,
        )]));
        harness
            .events
            .push_decode_result(Ok(vec![deposit_event(3, 0, addr(0x10), 1)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(Vec::new())]);
        harness
            .events
            .push_store_result(Ok(vec![log_entry(addr(0xFE), 4, 0, 2)]));
        harness
            .events
            .push_decode_result(Ok(vec![store_set_event(4, 0, addr(0xFE), 2)]));

        harness.run(&base_inputs()).await.expect("run succeeds");

        let store_calls = harness.events.store_calls();
        assert_eq!(store_calls.len(), 1);
        assert_eq!(store_calls[0].0, vec![addr(0xFE)]);
    }

    #[tokio::test]
    async fn run_fetches_missing_tokens_only_for_unknown_addresses() {
        let harness = EngineHarness::new();
        let token_a = addr(0x01);
        let token_b = addr(0x02);
        harness.events.set_latest_blocks(vec![Ok(50)]);
        harness.window.set_results(vec![Ok((10, 12))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            10,
            0,
            1,
        )]));
        harness.events.push_decode_result(Ok(vec![
            deposit_event(10, 0, token_a, 1),
            deposit_event(11, 0, token_b, 2),
        ]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(vec![token_row(token_a)])]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(vec![(token_b, token_info("NEW"))])]);

        harness.run(&base_inputs()).await.expect("run succeeds");

        let fetch_calls = harness.tokens.fetch_calls();
        assert_eq!(fetch_calls.len(), 1);
        assert_eq!(fetch_calls[0], vec![token_b]);
    }

    #[tokio::test]
    async fn run_skips_fetch_missing_when_all_known() {
        let harness = EngineHarness::new();
        let token_a = addr(0x01);
        let token_b = addr(0x02);
        harness.events.set_latest_blocks(vec![Ok(22)]);
        harness.window.set_results(vec![Ok((5, 6))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            5,
            0,
            1,
        )]));
        harness.events.push_decode_result(Ok(vec![
            deposit_event(5, 0, token_a, 1),
            deposit_event(6, 0, token_b, 2),
        ]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(vec![token_row(token_a), token_row(token_b)])]);

        harness.run(&base_inputs()).await.expect("run succeeds");

        assert!(harness.tokens.fetch_calls().is_empty());
    }

    #[tokio::test]
    async fn run_fetches_raindex_with_window_range() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(200)]);
        harness.window.set_results(vec![Ok((42, 123))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            42,
            0,
            1,
        )]));
        harness.events.push_decode_result(Ok(Vec::new()));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(Vec::new())]);

        harness.run(&base_inputs()).await.expect("run succeeds");

        let calls = harness.events.raindex_calls();
        assert_eq!(calls.len(), 1);
        let (_addr, start, end) = calls[0];
        assert_eq!(start, 42);
        assert_eq!(end, 123);
    }

    #[tokio::test]
    async fn run_fetches_store_logs_with_deduped_addresses() {
        let harness = EngineHarness::new();
        let store1 = addr(0xA1);
        let store2 = addr(0xA2);
        harness.db.set_store_responses(vec![Ok(vec![
            StoreAddressRow {
                store_address: store1,
            },
            StoreAddressRow {
                store_address: Address::ZERO,
            },
            StoreAddressRow {
                store_address: store2,
            },
        ])]);
        harness.events.set_latest_blocks(vec![Ok(50)]);
        harness.window.set_results(vec![Ok((10, 12))]);
        harness.events.push_raindex_result(Ok(vec![
            log_entry(base_target().raindex_address, 10, 0, 1),
            log_entry(base_target().raindex_address, 11, 0, 2),
        ]));
        harness.events.push_decode_result(Ok(vec![
            add_order_event(10, 0, store1, addr(0x10), addr(0x11), 1),
            // duplicate store in decoded events
            add_order_event(11, 0, store1, addr(0x12), addr(0x13), 2),
        ]));
        harness
            .events
            .push_store_result(Ok(vec![log_entry(store1, 12, 0, 3)]));
        harness
            .events
            .push_decode_result(Ok(vec![store_set_event(12, 0, store1, 3)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(Vec::new())]);

        harness.run(&base_inputs()).await.expect("run succeeds");

        let calls = harness.events.store_calls();
        assert_eq!(calls.len(), 1);
        let (addresses, start, end) = &calls[0];
        assert_eq!((*start, *end), (10, 12));
        // Expect deduped and sorted (BTreeSet ordering)
        assert_eq!(addresses, &vec![store1, store2]);
    }

    #[tokio::test]
    async fn run_fetches_missing_tokens_dedupes() {
        let harness = EngineHarness::new();
        let token = addr(0x0A);
        harness.events.set_latest_blocks(vec![Ok(60)]);
        harness.window.set_results(vec![Ok((7, 9))]);
        harness.events.push_raindex_result(Ok(vec![
            log_entry(base_target().raindex_address, 7, 0, 1),
            log_entry(base_target().raindex_address, 8, 0, 2),
        ]));
        harness.events.push_decode_result(Ok(vec![
            deposit_event(7, 0, token, 1),
            // duplicate token should be deduped before fetch_missing
            deposit_event(8, 0, token, 2),
        ]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(vec![(token, token_info("TOK"))])]);

        let mut inputs = base_inputs();
        inputs.metadata_rpcs = vec![
            Url::parse("https://rpc1.example").unwrap(),
            Url::parse("https://rpc2.example").unwrap(),
        ];

        harness.run(&inputs).await.expect("run succeeds");

        let fetch_calls = harness.tokens.fetch_calls();
        assert_eq!(fetch_calls.len(), 1);
        assert_eq!(fetch_calls[0], vec![token]);
    }

    #[tokio::test]
    async fn run_export_dump_receives_target_and_block() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(1000)]);
        harness.window.set_results(vec![Ok((100, 105))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            100,
            0,
            1,
        )]));
        harness
            .events
            .push_decode_result(Ok(vec![deposit_event(100, 0, addr(0x01), 1)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(Vec::new())]);

        let inputs = base_inputs();
        harness.run(&inputs).await.expect("run succeeds");

        let calls = harness.apply.export_calls();
        assert_eq!(calls.len(), 1);
        let (raindex_id, end_block) = &calls[0];
        assert_eq!(raindex_id.chain_id, raindex_id.chain_id);
        assert_eq!(raindex_id.raindex_address, raindex_id.raindex_address);
        assert_eq!(*end_block, 105);
    }

    #[tokio::test]
    async fn run_passes_dump_sql_to_bootstrap() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(30)]);
        harness.window.set_results(vec![Ok((3, 3))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            3,
            0,
            1,
        )]));
        harness
            .events
            .push_decode_result(Ok(vec![deposit_event(3, 0, addr(0x10), 1)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(Vec::new())]);
        let mut inputs = base_inputs();
        inputs.dump_str = Some(Arc::new("SELECT 1".into()));

        harness.run(&inputs).await.expect("run succeeds");

        let configs = harness.bootstrap.configs();
        assert_eq!(configs.len(), 1);
        let config = &configs[0];
        assert_eq!(
            config.dump_sql.as_ref().map(|s| s.as_str()),
            Some("SELECT 1")
        );
        #[cfg(not(target_family = "wasm"))]
        assert_eq!(
            config.dump_stmt.as_ref().map(SqlStatementBatch::len),
            Some(1)
        );
        #[cfg(target_family = "wasm")]
        assert!(config.dump_stmt.is_none());
        assert_eq!(config.deployment_block, inputs.cfg.deployment_block);
        assert_eq!(config.block_number_threshold, inputs.block_number_threshold);
        assert_eq!(config.latest_block, inputs.manifest_end_block);
    }

    #[tokio::test]
    async fn run_waits_for_parallel_tasks() {
        let harness = EngineHarness::new();
        let store = addr(0xAA);
        harness.events.set_latest_blocks(vec![Ok(40)]);
        harness.window.set_results(vec![Ok((12, 15))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            12,
            0,
            1,
        )]));
        harness.events.push_decode_result(Ok(vec![add_order_event(
            12,
            0,
            store,
            addr(0x10),
            addr(0x11),
            1,
        )]));
        harness
            .events
            .push_store_result(Ok(vec![log_entry(store, 13, 0, 2)]));
        harness
            .events
            .push_decode_result(Ok(vec![store_set_event(13, 0, store, 2)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(Vec::new())]);

        let barrier = Arc::new(Barrier::new(2));
        harness.tokens.set_barrier(barrier.clone());
        harness.events.set_store_barrier(barrier);

        harness.run(&base_inputs()).await.expect("run succeeds");
        assert!(harness.tokens.load_completed());
        assert!(harness.events.store_completed());
    }

    #[tokio::test]
    async fn run_propagates_status_error_before_latest_block() {
        let harness = EngineHarness::new();
        harness
            .status
            .set_results(vec![Err(LocalDbError::CustomError("status fail".into()))]);

        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "status fail"),
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(
            harness.status.phases(),
            vec![SyncPhase::FetchingLatestBlock]
        );
        assert!(harness.window.calls().is_empty());
    }

    #[tokio::test]
    async fn run_propagates_status_error_after_latest_block() {
        let harness = EngineHarness::new();
        harness.status.set_results(vec![
            Ok(()),
            Err(LocalDbError::CustomError("status fail".into())),
        ]);
        harness.events.set_latest_blocks(vec![Ok(42)]);

        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "status fail"),
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(
            harness.status.phases(),
            vec![SyncPhase::FetchingLatestBlock, SyncPhase::RunningBootstrap]
        );
        assert!(harness.bootstrap.configs().is_empty());
    }

    #[tokio::test]
    async fn run_propagates_raindex_decode_error() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(20)]);
        harness.window.set_results(vec![Ok((1, 5))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            1,
            0,
            1,
        )]));
        harness
            .events
            .push_decode_result(Err(LocalDbError::CustomError("decode fail".into())));

        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "decode fail"),
            other => panic!("unexpected error: {other:?}"),
        }
        assert!(harness.events.store_calls().is_empty());
        assert!(harness.tokens.load_calls().is_empty());
    }

    #[tokio::test]
    async fn run_propagates_store_decode_error() {
        let harness = EngineHarness::new();
        let store = addr(0x77);
        harness.events.set_latest_blocks(vec![Ok(30)]);
        harness.window.set_results(vec![Ok((1, 3))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            1,
            0,
            1,
        )]));
        harness.events.push_decode_result(Ok(vec![add_order_event(
            1,
            0,
            store,
            addr(0x01),
            addr(0x02),
            1,
        )]));
        harness
            .events
            .push_store_result(Ok(vec![log_entry(store, 2, 0, 2)]));
        harness
            .events
            .push_decode_result(Err(LocalDbError::CustomError("store decode fail".into())));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);

        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "store decode fail"),
            other => panic!("unexpected error: {other:?}"),
        }
        let phases = harness.status.phases();
        assert!(phases.contains(&SyncPhase::DecodingStoreLogs));
        assert_eq!(harness.tokens.load_calls().len(), 1);
    }

    #[tokio::test]
    async fn run_propagates_latest_block_error() {
        let harness = EngineHarness::new();
        harness
            .events
            .set_latest_blocks(vec![Err(LocalDbError::CustomError("latest fail".into()))]);
        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "latest fail"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_propagates_fetch_raindex_error() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(10)]);
        harness.window.set_results(vec![Ok((1, 2))]);
        harness
            .events
            .push_raindex_result(Err(LocalDbError::CustomError("raindex fail".into())));

        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "raindex fail"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_propagates_fetch_store_error() {
        let harness = EngineHarness::new();
        let store = addr(0x70);
        harness.events.set_latest_blocks(vec![Ok(20)]);
        harness.window.set_results(vec![Ok((1, 3))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            1,
            0,
            1,
        )]));
        harness.events.push_decode_result(Ok(vec![add_order_event(
            1,
            0,
            store,
            addr(0x01),
            addr(0x02),
            1,
        )]));
        harness
            .events
            .push_store_result(Err(LocalDbError::CustomError("store fail".into())));

        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "store fail"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_propagates_load_existing_error() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(10)]);
        harness.window.set_results(vec![Ok((1, 1))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            1,
            0,
            1,
        )]));
        harness
            .events
            .push_decode_result(Ok(vec![deposit_event(1, 0, addr(0x03), 1)]));
        harness
            .tokens
            .set_load_existing_results(vec![Err(LocalDbError::CustomError("load fail".into()))]);

        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "load fail"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_propagates_fetch_missing_error() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(10)]);
        harness.window.set_results(vec![Ok((1, 1))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            1,
            0,
            1,
        )]));
        harness
            .events
            .push_decode_result(Ok(vec![deposit_event(1, 0, addr(0x05), 1)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Err(LocalDbError::CustomError(
                "fetch missing fail".into(),
            ))]);

        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "fetch missing fail"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_propagates_build_batch_error() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(15)]);
        harness.window.set_results(vec![Ok((1, 2))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            1,
            0,
            1,
        )]));
        harness
            .events
            .push_decode_result(Ok(vec![deposit_event(1, 0, addr(0x06), 1)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(Vec::new())]);
        harness
            .apply
            .set_build_results(vec![Err(LocalDbError::CustomError("build fail".into()))]);

        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "build fail"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_propagates_persist_error() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(15)]);
        harness.window.set_results(vec![Ok((1, 2))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            1,
            0,
            1,
        )]));
        harness
            .events
            .push_decode_result(Ok(vec![deposit_event(1, 0, addr(0x07), 1)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(Vec::new())]);
        harness
            .apply
            .set_persist_results(vec![Err(LocalDbError::CustomError("persist fail".into()))]);

        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "persist fail"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_propagates_export_error() {
        let harness = EngineHarness::new();
        harness.events.set_latest_blocks(vec![Ok(15)]);
        harness.window.set_results(vec![Ok((1, 2))]);
        harness.events.push_raindex_result(Ok(vec![log_entry(
            base_target().raindex_address,
            1,
            0,
            1,
        )]));
        harness
            .events
            .push_decode_result(Ok(vec![deposit_event(1, 0, addr(0x08), 1)]));
        harness
            .tokens
            .set_load_existing_results(vec![Ok(Vec::new())]);
        harness
            .tokens
            .set_fetch_missing_results(vec![Ok(Vec::new())]);
        harness
            .apply
            .set_export_results(vec![Err(LocalDbError::CustomError("export fail".into()))]);

        let err = harness.run(&base_inputs()).await.expect_err("should fail");
        match err {
            LocalDbError::CustomError(msg) => assert_eq!(msg, "export fail"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    struct MockExecutor {
        response: Mutex<Result<Vec<StoreAddressRow>, LocalDbQueryError>>,
        statements: Mutex<Vec<SqlStatement>>,
    }

    impl MockExecutor {
        fn with_response(response: Result<Vec<StoreAddressRow>, LocalDbQueryError>) -> Self {
            Self {
                response: Mutex::new(response),
                statements: Mutex::new(Vec::new()),
            }
        }

        fn success(rows: Vec<StoreAddressRow>) -> Self {
            Self::with_response(Ok(rows))
        }

        fn failure(err: LocalDbQueryError) -> Self {
            Self::with_response(Err(err))
        }

        fn recorded(&self) -> Vec<SqlStatement> {
            self.statements.lock().unwrap().clone()
        }
    }

    #[cfg_attr(target_family = "wasm", async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait)]
    impl LocalDbQueryExecutor for MockExecutor {
        async fn execute_batch(&self, _batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            panic!("execute_batch should not be called");
        }

        async fn query_json<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: crate::local_db::query::FromDbJson,
        {
            self.statements.lock().unwrap().push(stmt.clone());
            match self.response.lock().unwrap().clone() {
                Ok(rows) => {
                    let value = serde_json::to_value(rows).expect("serialize rows");
                    serde_json::from_value(value)
                        .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))
                }
                Err(err) => Err(err),
            }
        }

        async fn query_text(&self, _stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            panic!("query_text should not be called");
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::not_implemented("wipe_and_recreate"))
        }
    }

    #[tokio::test]
    async fn load_known_store_addresses_yields_rows_and_binds_params() {
        let rows = vec![
            StoreAddressRow {
                store_address: Address::from([0x11u8; 20]),
            },
            StoreAddressRow {
                store_address: Address::from([0x22u8; 20]),
            },
        ];
        let executor = MockExecutor::success(rows.clone());
        let raindex_id = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: Address::repeat_byte(0xAA),
        };

        let actual = load_known_store_addresses(&executor, &raindex_id)
            .await
            .expect("fetch rows");

        assert_eq!(actual, rows);
        let calls = executor.recorded();
        assert_eq!(calls.len(), 1);
        let stmt = &calls[0];
        assert_eq!(stmt.params().len(), 2);
        assert_eq!(stmt.params()[0], SqlValue::U64(raindex_id.chain_id as u64));
        assert_eq!(
            stmt.params()[1],
            SqlValue::Text("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string())
        );
    }

    #[tokio::test]
    async fn load_known_store_addresses_returns_empty_vec() {
        let executor = MockExecutor::success(Vec::new());
        let raindex_id = RaindexIdentifier {
            chain_id: 10,
            raindex_address: Address::repeat_byte(0xBB),
        };

        let actual = load_known_store_addresses(&executor, &raindex_id)
            .await
            .expect("fetch rows");
        assert!(actual.is_empty());
    }

    #[tokio::test]
    async fn load_known_store_addresses_propagates_query_error() {
        let executor = MockExecutor::failure(LocalDbQueryError::database("boom"));
        let raindex_id = RaindexIdentifier {
            chain_id: 1,
            raindex_address: Address::repeat_byte(0xCC),
        };

        let err = load_known_store_addresses(&executor, &raindex_id)
            .await
            .expect_err("should propagate error");
        match err {
            LocalDbError::LocalDbQueryError(inner) => match inner {
                LocalDbQueryError::Database { message } => assert_eq!(message, "boom"),
                other => panic!("unexpected query error: {other:?}"),
            },
            other => panic!("unexpected error variant: {other:?}"),
        }
    }
}
