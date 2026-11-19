use crate::erc20::TokenInfo;
use crate::local_db::decode::DecodedEvent;
use crate::local_db::insert::{
    decoded_events_to_statements as build_decoded_event_sql,
    generate_erc20_token_statements as build_token_upserts,
};
use crate::local_db::query::fetch_erc20_tokens_by_addresses::Erc20TokenRow;
use crate::local_db::query::upsert_derived_trades::upsert_derived_trades_batch;
use crate::local_db::query::upsert_derived_vault_deltas::upsert_derived_vault_deltas_batch;
use crate::local_db::query::upsert_target_watermark::upsert_target_watermark_stmt;
use crate::local_db::query::upsert_vault_balances::upsert_vault_balances_batch;
use crate::local_db::query::{LocalDbQueryExecutor, SqlStatement, SqlStatementBatch};
use crate::local_db::RaindexIdentifier;
use crate::local_db::{decode::DecodedEventData, LocalDbError};
use crate::rpc_client::LogEntryResponse;
use alloy::primitives::{Address, B256};
use async_trait::async_trait;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ApplyPipelineTargetInfo {
    pub raindex_id: RaindexIdentifier,
    pub start_block: u64,
    pub target_block: u64,
    pub defer_analyze: bool,
    pub hash: B256,
}

/// Translates fetched/decoded data into SQL and persists it atomically.
///
/// Responsibilities (concrete):
/// - Build a transactional batch containing:
///   - Token upserts for provided `(Address, TokenInfo)` pairs.
///   - Decoded event INSERTs needed for order/vault queries, binding the target
///     raindex. Raw events and signed take-order contexts are not persisted.
///   - Vault balance change/running balance upserts.
///   - Watermark update to the `target_block` (and later last hash).
/// - Persist the batch with a single-writer gate; must assert that the batch
///   is transaction-wrapped and fail if not.
///
/// Policy (environment-specific): Dump export after a successful persist
/// (producer only); browser is no-op.
#[async_trait(?Send)]
pub trait ApplyPipeline {
    /// Builds the SQL batch for a cycle. The batch must be suitable for
    /// atomic execution (the caller will ensure single-writer semantics).
    fn build_batch(
        &self,
        target_info: &ApplyPipelineTargetInfo,
        _raw_logs: &[LogEntryResponse],
        decoded_events: &[DecodedEventData<DecodedEvent>],
        existing_tokens: &[Erc20TokenRow],
        tokens_to_upsert: &[(Address, TokenInfo)],
    ) -> Result<SqlStatementBatch, LocalDbError> {
        // 1) Build decimals map from existing rows and the incoming upserts
        let decimals_by_token: HashMap<Address, u8> = existing_tokens
            .iter()
            .map(|row| (row.token_address, row.decimals))
            .chain(
                tokens_to_upsert
                    .iter()
                    .map(|(addr, info)| (*addr, info.decimals)),
            )
            .collect();

        // 2) Build component batches
        let mut batch = SqlStatementBatch::new();

        // Token upserts for the missing set only
        if !tokens_to_upsert.is_empty() {
            let upserts = build_token_upserts(&target_info.raindex_id, tokens_to_upsert);
            batch.extend(upserts);
        }

        // Decoded raindex/store events
        let decoded_batch =
            build_decoded_event_sql(&target_info.raindex_id, decoded_events, &decimals_by_token)?;
        batch.extend(decoded_batch);

        // Derived delta refresh plus vault balance change/running balance updates
        if target_info.start_block <= target_info.target_block {
            batch.extend(upsert_derived_vault_deltas_batch(
                &target_info.raindex_id,
                target_info.start_block,
                target_info.target_block,
            ));
            batch.extend(upsert_vault_balances_batch(
                &target_info.raindex_id,
                target_info.start_block,
                target_info.target_block,
            ));
            batch.extend(upsert_derived_trades_batch(
                &target_info.raindex_id,
                target_info.start_block,
                target_info.target_block,
            ));
        }

        // Watermark update to target block
        batch.add(upsert_target_watermark_stmt(
            &target_info.raindex_id,
            target_info.target_block,
            target_info.hash.into(),
        ));

        // Ensure SQLite planner stats are up to date so reads don't suffer from
        // poor query plans.
        if !target_info.defer_analyze {
            batch.add(SqlStatement::new("ANALYZE"));
        }

        Ok(batch)
    }

    /// Persists the previously built batch. Implementations must assert that
    /// the input is wrapped in a transaction and return an error otherwise.
    async fn persist<DB>(&self, db: &DB, batch: &SqlStatementBatch) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        let stmts = batch.clone().ensure_transaction();
        db.execute_batch(&stmts).await.map_err(LocalDbError::from)
    }

    /// Optional policy hook to export dumps after a successful persist.
    /// Default implementation is a no-op.
    async fn export_dump<DB>(
        &self,
        _db: &DB,
        _raindex_id: &RaindexIdentifier,
        _end_block: u64,
    ) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        Ok(())
    }
}

/// Default apply adapter using the trait's provided behavior.
#[derive(Debug, Clone, Default)]
pub struct DefaultApplyPipeline;

impl DefaultApplyPipeline {
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait(?Send)]
impl ApplyPipeline for DefaultApplyPipeline {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_db::query::{LocalDbQueryError, SqlStatement, SqlValue};
    use crate::local_db::RaindexIdentifier;
    use alloy::primitives::{b256, Bytes, B256, U256};

    const SAMPLE_HASH_B256: B256 =
        b256!("0x111122223333444455556666777788889999aaaabbbbccccddddeeeeffff0000");

    fn build_target_info(
        raindex_id: &RaindexIdentifier,
        start_block: u64,
        target_block: u64,
    ) -> ApplyPipelineTargetInfo {
        ApplyPipelineTargetInfo {
            raindex_id: raindex_id.clone(),
            start_block,
            target_block,
            defer_analyze: false,
            hash: SAMPLE_HASH_B256,
        }
    }

    struct MockDb {
        // capture executed SQL for assertions
        pub executed: std::sync::Mutex<Vec<String>>,
    }

    #[cfg_attr(target_family = "wasm", async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait)]
    impl LocalDbQueryExecutor for MockDb {
        async fn execute_batch(&self, batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            let mut exec = self.executed.lock().unwrap();
            for s in batch.statements() {
                exec.push(s.sql().to_string());
            }
            Ok(())
        }

        async fn query_json<T>(&self, _stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: crate::local_db::query::FromDbJson,
        {
            Err(LocalDbQueryError::database("not used"))
        }

        async fn query_text(&self, _stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            Ok(String::new())
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::not_implemented("wipe_and_recreate"))
        }
    }

    struct FailingDb;

    #[cfg_attr(target_family = "wasm", async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait)]
    impl LocalDbQueryExecutor for FailingDb {
        async fn execute_batch(&self, _batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::database("boom"))
        }

        async fn query_json<T>(&self, _stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: crate::local_db::query::FromDbJson,
        {
            Err(LocalDbQueryError::database("not used"))
        }

        async fn query_text(&self, _stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            Err(LocalDbQueryError::database("not used"))
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::not_implemented("wipe_and_recreate"))
        }
    }

    fn sample_raindex_id() -> RaindexIdentifier {
        RaindexIdentifier::new(137, Address::from([0u8; 20]))
    }

    fn deposit_event(addr: Address) -> DecodedEventData<DecodedEvent> {
        use crate::local_db::decode::EventType;
        use raindex_bindings::IRaindexV7::DepositV2;
        DecodedEventData {
            event_type: EventType::DepositV2,
            block_number: U256::from(1),
            block_timestamp: U256::from(2),
            transaction_hash: b256!(
                "0x0000000000000000000000000000000000000000000000000000000000000abc"
            ),
            log_index: U256::ZERO,
            decoded_data: DecodedEvent::DepositV2(Box::new(DepositV2 {
                sender: Address::from([1u8; 20]),
                token: addr,
                vaultId: U256::from(1u64).into(),
                depositAmountUint256: U256::from(1000u64),
            })),
        }
    }

    fn withdraw_event(addr: Address) -> DecodedEventData<DecodedEvent> {
        use crate::local_db::decode::EventType;
        use raindex_bindings::IRaindexV7::WithdrawV2;
        DecodedEventData {
            event_type: EventType::WithdrawV2,
            block_number: U256::from(0x10),
            block_timestamp: U256::from(0x20),
            transaction_hash: b256!(
                "0x0000000000000000000000000000000000000000000000000000000000000def"
            ),
            log_index: U256::from(1),
            decoded_data: DecodedEvent::WithdrawV2(Box::new(WithdrawV2 {
                sender: Address::from([2u8; 20]),
                token: addr,
                vaultId: U256::from(2u64).into(),
                targetAmount: U256::from(2000u64).into(),
                withdrawAmount: U256::from(1500u64).into(),
                withdrawAmountUint256: U256::from(1500u64),
            })),
        }
    }

    #[test]
    fn build_batch_contains_watermark() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let token = Address::from([3u8; 20]);

        // existing token with decimals 18
        let existing = vec![Erc20TokenRow {
            chain_id: raindex_id.chain_id as u32,
            raindex_address: raindex_id.raindex_address,
            token_address: token,
            name: "Token".into(),
            symbol: "TKN".into(),
            decimals: 18,
        }];

        let decoded = vec![deposit_event(token)];

        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 123),
                &[],
                &decoded,
                &existing,
                &[],
            )
            .expect("batch ok");

        assert!(
            batch.clone().ensure_transaction().is_transaction(),
            "ensure_transaction should wrap the statements"
        );

        let texts: Vec<_> = batch.statements().iter().map(|s| s.sql()).collect();
        assert!(
            texts
                .iter()
                .any(|s| s.contains("INSERT INTO target_watermarks")),
            "watermark update present"
        );
    }

    #[tokio::test]
    async fn persist_wraps_when_needed() {
        let pipeline = DefaultApplyPipeline::new();
        let db = MockDb {
            executed: Default::default(),
        };
        // Build a simple non-transactional batch manually
        let mut batch = SqlStatementBatch::new();
        batch.add(crate::local_db::query::SqlStatement::new(
            "INSERT INTO foo VALUES (1)",
        ));

        // Should not error; persist will wrap
        pipeline.persist(&db, &batch).await.expect("persist ok");
        let executed = db.executed.lock().unwrap().clone();
        assert!(executed
            .first()
            .is_some_and(|s| s.trim().eq_ignore_ascii_case("BEGIN TRANSACTION")));
        assert!(executed
            .last()
            .is_some_and(|s| s.trim().eq_ignore_ascii_case("COMMIT")));
    }

    #[tokio::test]
    async fn persist_propagates_db_error() {
        let pipeline = DefaultApplyPipeline::new();
        let db = FailingDb;

        let mut batch = SqlStatementBatch::new();
        batch.add(crate::local_db::query::SqlStatement::new(
            "INSERT INTO foo VALUES (1)",
        ));

        let err = pipeline.persist(&db, &batch).await.unwrap_err();
        match err {
            LocalDbError::LocalDbQueryError(e) => {
                let msg = format!("{}", e);
                assert!(msg.contains("Database operation failed"));
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn empty_work_window_only_watermark() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();

        let batch = pipeline
            .build_batch(&build_target_info(&raindex_id, 1, 42), &[], &[], &[], &[])
            .expect("batch ok");

        // Expect derived state refreshes plus the watermark and ANALYZE when there is no work.
        let texts: Vec<_> = batch.statements().iter().map(|s| s.sql().trim()).collect();
        assert_eq!(texts.len(), 9);
        assert!(texts
            .iter()
            .any(|s| s.contains("DELETE FROM derived_vault_deltas")));
        assert!(texts
            .iter()
            .any(|s| s.contains("INSERT OR REPLACE INTO derived_vault_deltas")));
        assert!(texts
            .iter()
            .any(|s| s.contains("DELETE FROM vault_balance_changes")));
        assert!(texts
            .iter()
            .any(|s| s.contains("INSERT OR IGNORE INTO vault_balance_changes")));
        assert!(texts
            .iter()
            .any(|s| s.contains("INSERT OR REPLACE INTO running_vault_balances")));
        assert!(texts
            .iter()
            .any(|s| s.contains("DELETE FROM derived_trades")));
        assert!(texts
            .iter()
            .any(|s| s.contains("INSERT OR REPLACE INTO derived_trades")));
        assert!(texts
            .iter()
            .any(|s| s.contains("INSERT INTO target_watermarks")));
        assert!(texts.contains(&"ANALYZE"));
    }

    #[test]
    fn decimals_from_upsert_override_existing() {
        use alloy::primitives::U256;
        use rain_math_float::Float;

        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let token = Address::from([7u8; 20]);

        // Existing says 6 decimals
        let existing = vec![Erc20TokenRow {
            chain_id: raindex_id.chain_id as u32,
            raindex_address: raindex_id.raindex_address,
            token_address: token,
            name: "Token".into(),
            symbol: "T6".into(),
            decimals: 6,
        }];
        // Upsert says 18 decimals (override)
        let upserts = vec![(
            token,
            TokenInfo {
                name: "Token".into(),
                symbol: "T18".into(),
                decimals: 18,
            },
        )];

        let decoded = vec![deposit_event(token)];
        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 100),
                &[],
                &decoded,
                &existing,
                &upserts,
            )
            .expect("batch ok");

        // Find the deposit INSERT and inspect params; ?10 is deposit_amount
        let stmt = batch
            .statements()
            .iter()
            .find(|s| s.sql().starts_with("INSERT INTO deposits"))
            .expect("deposit insert present");

        // Params: [chain_id, raindex, block_number, block_timestamp, tx_hash, log_index, sender, token, vault_id, deposit_amount, deposit_amount_uint256]
        let deposit_amount_param = stmt.params().get(9).expect("param 10 present");
        let expected = Float::from_fixed_decimal(U256::from(1000u64), 18)
            .unwrap()
            .as_hex();

        match deposit_amount_param {
            crate::local_db::query::SqlValue::Text(val) => {
                assert_eq!(
                    val, &expected,
                    "upsert decimals should take precedence over existing row"
                );
            }
            other => panic!("unexpected param type for deposit_amount: {other:?}"),
        }
    }

    #[test]
    fn invalid_existing_token_address_is_ignored() {
        use alloy::primitives::U256;
        use rain_math_float::Float;

        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let token = Address::from([10u8; 20]);

        // Existing has an invalid address string
        let existing = vec![Erc20TokenRow {
            chain_id: raindex_id.chain_id as u32,
            raindex_address: raindex_id.raindex_address,
            token_address: Address::ZERO,
            name: "Bad".into(),
            symbol: "BAD".into(),
            decimals: 6,
        }];
        // Upsert provides correct decimals
        let upserts = vec![(
            token,
            TokenInfo {
                name: "Good".into(),
                symbol: "GOOD".into(),
                decimals: 9,
            },
        )];

        let decoded = vec![deposit_event(token)];
        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 100),
                &[],
                &decoded,
                &existing,
                &upserts,
            )
            .expect("batch ok");

        let stmt = batch
            .statements()
            .iter()
            .find(|s| s.sql().starts_with("INSERT INTO deposits"))
            .expect("deposit insert present");
        let deposit_amount_param = stmt.params().get(9).expect("param 10 present");
        let expected = Float::from_fixed_decimal(U256::from(1000u64), 9)
            .unwrap()
            .as_hex();
        match deposit_amount_param {
            crate::local_db::query::SqlValue::Text(val) => {
                assert_eq!(val, &expected);
            }
            other => panic!("unexpected param type for deposit_amount: {other:?}"),
        }
    }

    #[test]
    fn missing_decimals_yields_error() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let token = Address::from([9u8; 20]);
        let decoded = vec![deposit_event(token)];

        let err = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 1),
                &[],
                &decoded,
                &[],
                &[],
            )
            .unwrap_err();

        use crate::local_db::insert::InsertError;
        match err {
            LocalDbError::InsertError(InsertError::MissingTokenDecimals { token: t }) => {
                assert_eq!(t, format!("0x{:x}", token));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn token_upserts_generated_for_provided_tokens() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();

        // Existing has A; upserts contain B
        let token_a = Address::from([1u8; 20]);
        let token_b = Address::from([2u8; 20]);
        let existing = vec![Erc20TokenRow {
            chain_id: raindex_id.chain_id as u32,
            raindex_address: raindex_id.raindex_address,
            token_address: token_a,
            name: "A".into(),
            symbol: "A".into(),
            decimals: 6,
        }];
        let upserts = vec![(
            token_b,
            TokenInfo {
                name: "B".into(),
                symbol: "B".into(),
                decimals: 8,
            },
        )];

        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 1),
                &[],
                &[],
                &existing,
                &upserts,
            )
            .expect("batch ok");

        let upsert_stmts: Vec<_> = batch
            .statements()
            .iter()
            .filter(|s| s.sql().starts_with("INSERT INTO erc20_tokens"))
            .collect();
        assert_eq!(
            upsert_stmts.len(),
            1,
            "upsert statements are generated for provided tokens"
        );

        // The token param is ?3 in the token upsert builder
        let addr_param = upsert_stmts[0].params().get(2).unwrap();
        match addr_param {
            crate::local_db::query::SqlValue::Text(val) => {
                assert_eq!(val, &format!("0x{:x}", token_b));
            }
            other => panic!("unexpected param type for token: {other:?}"),
        }
    }

    #[test]
    fn watermark_param_matches_target_block() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();

        let target_block = 12345u64;
        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, target_block),
                &[],
                &[],
                &[],
                &[],
            )
            .expect("batch ok");

        let stmt = batch
            .statements()
            .iter()
            .find(|s| s.sql().starts_with("INSERT INTO target_watermarks"))
            .expect("watermark update present");

        match stmt.params().get(2) {
            Some(crate::local_db::query::SqlValue::U64(v)) => {
                assert_eq!(*v, target_block);
            }
            Some(crate::local_db::query::SqlValue::I64(v)) => {
                assert_eq!(*v as u64, target_block);
            }
            other => panic!("unexpected watermark param: {other:?}"),
        }

        match stmt.params().get(3) {
            Some(SqlValue::Text(v)) => {
                assert_eq!(v, &SAMPLE_HASH_B256.to_string());
            }
            other => panic!("unexpected watermark hash param: {other:?}"),
        }
    }

    #[test]
    fn decimals_from_existing_when_no_upserts() {
        use alloy::primitives::U256;
        use rain_math_float::Float;

        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let token = Address::from([13u8; 20]);

        // Only existing provides decimals
        let existing = vec![Erc20TokenRow {
            chain_id: raindex_id.chain_id as u32,
            raindex_address: raindex_id.raindex_address,
            token_address: token,
            name: "E".into(),
            symbol: "E".into(),
            decimals: 9,
        }];

        let decoded = vec![deposit_event(token)];
        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 100),
                &[],
                &decoded,
                &existing,
                &[],
            )
            .expect("batch ok");

        let stmt = batch
            .statements()
            .iter()
            .find(|s| s.sql().starts_with("INSERT INTO deposits"))
            .expect("deposit insert present");
        let deposit_amount_param = stmt.params().get(9).expect("param 10 present");
        let expected = Float::from_fixed_decimal(U256::from(1000u64), 9)
            .unwrap()
            .as_hex();
        match deposit_amount_param {
            crate::local_db::query::SqlValue::Text(val) => {
                assert_eq!(val, &expected);
            }
            other => panic!("unexpected param type for deposit_amount: {other:?}"),
        }
    }

    #[tokio::test]
    async fn persist_preserves_prewrapped_transaction() {
        let pipeline = DefaultApplyPipeline::new();
        let db = MockDb {
            executed: Default::default(),
        };

        // Build an already transactional batch
        let mut batch = SqlStatementBatch::new();
        batch.add(crate::local_db::query::SqlStatement::new(
            "INSERT INTO foo VALUES (1)",
        ));
        let wrapped = batch.ensure_transaction();

        pipeline.persist(&db, &wrapped).await.expect("persist ok");

        let executed = db.executed.lock().unwrap().clone();
        let begin_count = executed
            .iter()
            .filter(|s| s.trim().eq_ignore_ascii_case("BEGIN TRANSACTION"))
            .count();
        let commit_count = executed
            .iter()
            .filter(|s| s.trim().eq_ignore_ascii_case("COMMIT"))
            .count();
        assert_eq!(begin_count, 1, "no duplicate BEGIN statements");
        assert_eq!(commit_count, 1, "no duplicate COMMIT statements");
        assert!(executed
            .first()
            .is_some_and(|s| s.trim().eq_ignore_ascii_case("BEGIN TRANSACTION")));
        assert!(executed
            .last()
            .is_some_and(|s| s.trim().eq_ignore_ascii_case("COMMIT")));
    }

    #[test]
    fn duplicate_upserts_last_wins_for_decimals() {
        use alloy::primitives::U256;
        use rain_math_float::Float;

        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let token = Address::from([11u8; 20]);

        // Provide two upserts for same token with differing decimals; later should win
        let upserts = vec![
            (
                token,
                TokenInfo {
                    name: "Tok".into(),
                    symbol: "T".into(),
                    decimals: 6,
                },
            ),
            (
                token,
                TokenInfo {
                    name: "Tok".into(),
                    symbol: "T".into(),
                    decimals: 18,
                },
            ),
        ];

        let decoded = vec![deposit_event(token)];
        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 100),
                &[],
                &decoded,
                &[],
                &upserts,
            )
            .expect("batch ok");

        // Two upserts present
        let upsert_count = batch
            .statements()
            .iter()
            .filter(|s| s.sql().starts_with("INSERT INTO erc20_tokens"))
            .count();
        assert_eq!(upsert_count, 2);

        // Deposit amount uses last upsert decimals (18)
        let stmt = batch
            .statements()
            .iter()
            .find(|s| s.sql().starts_with("INSERT INTO deposits"))
            .expect("deposit insert present");
        let deposit_amount_param = stmt.params().get(9).expect("param 10 present");
        let expected = Float::from_fixed_decimal(U256::from(1000u64), 18)
            .unwrap()
            .as_hex();
        match deposit_amount_param {
            crate::local_db::query::SqlValue::Text(val) => {
                assert_eq!(val, &expected);
            }
            other => panic!("unexpected param type for deposit_amount: {other:?}"),
        }
    }

    #[test]
    fn token_upserts_chain_id_param_matches_target() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();

        let token = Address::from([5u8; 20]);
        let upserts = vec![(
            token,
            TokenInfo {
                name: "X".into(),
                symbol: "X".into(),
                decimals: 18,
            },
        )];

        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 1),
                &[],
                &[],
                &[],
                &upserts,
            )
            .expect("batch ok");

        let stmt = batch
            .statements()
            .iter()
            .find(|s| s.sql().starts_with("INSERT INTO erc20_tokens"))
            .expect("token upsert present");
        match stmt.params().first() {
            Some(crate::local_db::query::SqlValue::U64(chain)) => {
                assert_eq!(*chain, raindex_id.chain_id as u64);
            }
            Some(crate::local_db::query::SqlValue::I64(chain)) => {
                assert_eq!(*chain as u64, raindex_id.chain_id as u64);
            }
            other => panic!("unexpected chain_id param: {other:?}"),
        }
    }

    #[test]
    fn raw_logs_do_not_create_rows_without_decoded_events() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();

        let mk = |block: u64, log_index: u64| LogEntryResponse {
            address: Address::from([0x11; 20]),
            topics: vec![],
            data: Bytes::from([0x01]),
            block_number: U256::from(block),
            block_timestamp: Some(U256::from(1)),
            transaction_hash: B256::from({
                let mut bytes = [0u8; 32];
                bytes[31] = block as u8;
                bytes
            }),
            transaction_index: "0x0".into(),
            block_hash: B256::from({
                let mut bytes = [0u8; 32];
                bytes[31] = block as u8;
                bytes
            }),
            log_index: U256::from(log_index),
            removed: false,
        };
        let raw = [mk(1, 0), mk(1, 1)];

        let batch = pipeline
            .build_batch(&build_target_info(&raindex_id, 1, 2), &raw, &[], &[], &[])
            .expect("batch ok");

        let texts: Vec<_> = batch.statements().iter().map(|s| s.sql()).collect();
        assert!(!texts
            .iter()
            .any(|s| s.starts_with("INSERT INTO raw_events")));
        assert!(!texts
            .iter()
            .any(|s| s.starts_with("INSERT INTO erc20_tokens")));
        assert!(!texts.iter().any(|s| s.starts_with("INSERT INTO deposits")));
        assert!(texts
            .iter()
            .any(|s| s.contains("INSERT INTO target_watermarks")));
    }

    #[test]
    fn watermark_emitted_exactly_once() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let batch = pipeline
            .build_batch(&build_target_info(&raindex_id, 1, 77), &[], &[], &[], &[])
            .expect("batch ok");
        let count = batch
            .statements()
            .iter()
            .filter(|s| s.sql().starts_with("INSERT INTO target_watermarks"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn analyze_emitted_exactly_once_and_is_last() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let token = Address::from([3u8; 20]);

        let existing = vec![Erc20TokenRow {
            chain_id: raindex_id.chain_id as u32,
            raindex_address: raindex_id.raindex_address,
            token_address: token,
            name: "Token".into(),
            symbol: "TKN".into(),
            decimals: 18,
        }];
        let decoded = vec![deposit_event(token)];

        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 100),
                &[],
                &decoded,
                &existing,
                &[],
            )
            .expect("batch ok");

        let analyze_count = batch
            .statements()
            .iter()
            .filter(|s| s.sql().trim() == "ANALYZE")
            .count();
        assert_eq!(analyze_count, 1, "ANALYZE should appear exactly once");

        let last_stmt = batch
            .statements()
            .last()
            .expect("batch should not be empty");
        assert_eq!(
            last_stmt.sql().trim(),
            "ANALYZE",
            "ANALYZE should be the last statement in the batch"
        );
    }

    #[test]
    fn deferred_analysis_omits_per_target_analyze() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let mut target = build_target_info(&raindex_id, 1, 100);
        target.defer_analyze = true;

        let batch = pipeline
            .build_batch(&target, &[], &[], &[], &[])
            .expect("batch ok");

        assert!(batch.statements().iter().all(|s| s.sql() != "ANALYZE"));
        assert!(batch
            .statements()
            .iter()
            .any(|s| s.sql().starts_with("INSERT INTO target_watermarks")));
    }

    #[test]
    fn statements_order_raw_then_tokens_then_decoded_then_watermark() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();

        // Build one raw, one token upsert, one decoded deposit
        let mk_raw = |block: u64, log_index: u64| LogEntryResponse {
            address: Address::from([0x11; 20]),
            topics: vec![],
            data: Bytes::from([0x02]),
            block_number: U256::from(block),
            block_timestamp: Some(U256::from(1)),
            transaction_hash: B256::from({
                let mut bytes = [0u8; 32];
                bytes[31] = block as u8;
                bytes
            }),
            transaction_index: "0x0".into(),
            block_hash: B256::from({
                let mut bytes = [0u8; 32];
                bytes[31] = block as u8;
                bytes
            }),
            log_index: U256::from(log_index),
            removed: false,
        };
        let token = Address::from([8u8; 20]);
        let upserts = vec![(
            token,
            TokenInfo {
                name: "Y".into(),
                symbol: "Y".into(),
                decimals: 18,
            },
        )];
        let decoded = vec![deposit_event(token)];

        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 9),
                &[mk_raw(9, 0)],
                &decoded,
                &[],
                &upserts,
            )
            .expect("batch ok");

        let idx_token = batch
            .statements()
            .iter()
            .position(|s| s.sql().starts_with("INSERT INTO erc20_tokens"))
            .expect("token upsert present");
        let idx_decoded = batch
            .statements()
            .iter()
            .position(|s| s.sql().starts_with("INSERT INTO deposits"))
            .expect("decoded present");
        let idx_watermark = batch
            .statements()
            .iter()
            .position(|s| s.sql().starts_with("INSERT INTO target_watermarks"))
            .expect("watermark present");
        let idx_derived_vault_deltas = batch
            .statements()
            .iter()
            .position(|s| s.sql().starts_with("DELETE FROM derived_vault_deltas"))
            .expect("derived vault deltas refresh present");
        let idx_derived_trades = batch
            .statements()
            .iter()
            .position(|s| s.sql().starts_with("DELETE FROM derived_trades"))
            .expect("derived trades refresh present");

        assert!(idx_token < idx_decoded, "token upserts before decoded");
        assert!(
            idx_decoded < idx_derived_vault_deltas,
            "decoded before derived vault deltas refresh"
        );
        assert!(
            idx_derived_vault_deltas < idx_derived_trades,
            "derived vault deltas refresh before derived trades refresh"
        );
        assert!(
            idx_derived_trades < idx_watermark,
            "derived trades refresh before watermark"
        );
    }

    #[test]
    fn deterministic_batch_shape_for_identical_inputs() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let token = Address::from([3u8; 20]);

        let existing = vec![Erc20TokenRow {
            chain_id: raindex_id.chain_id as u32,
            raindex_address: raindex_id.raindex_address,
            token_address: token,
            name: "Token".into(),
            symbol: "TKN".into(),
            decimals: 18,
        }];
        let decoded = vec![deposit_event(token)];

        let b1 = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 5),
                &[],
                &decoded,
                &existing,
                &[],
            )
            .expect("b1 ok");
        let b2 = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 5),
                &[],
                &decoded,
                &existing,
                &[],
            )
            .expect("b2 ok");

        assert_eq!(b1.statements().len(), b2.statements().len());
        for (s1, s2) in b1.statements().iter().zip(b2.statements().iter()) {
            assert_eq!(s1.sql(), s2.sql());
            assert_eq!(s1.params(), s2.params());
        }
    }

    #[test]
    fn includes_withdraw_decoded_events() {
        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let token = Address::from([12u8; 20]);

        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 5),
                &[],
                &[withdraw_event(token)],
                &[],
                &[],
            )
            .expect("batch ok");

        assert!(batch
            .statements()
            .iter()
            .any(|s| s.sql().starts_with("INSERT INTO withdrawals")));
    }

    #[test]
    fn multi_token_decimals_mapped_correctly() {
        use alloy::primitives::U256;
        use rain_math_float::Float;

        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();

        // Two tokens: A gets decimals from existing, B from upserts
        let token_a = Address::from([21u8; 20]);
        let token_b = Address::from([22u8; 20]);

        let existing = vec![Erc20TokenRow {
            chain_id: raindex_id.chain_id as u32,
            raindex_address: raindex_id.raindex_address,
            token_address: token_a,
            name: "A".into(),
            symbol: "A".into(),
            decimals: 6,
        }];
        let upserts = vec![(
            token_b,
            TokenInfo {
                name: "B".into(),
                symbol: "B".into(),
                decimals: 9,
            },
        )];

        let decoded = vec![deposit_event(token_a), deposit_event(token_b)];
        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 100),
                &[],
                &decoded,
                &existing,
                &upserts,
            )
            .expect("batch ok");

        // Collect deposit statements and check each token's deposit_amount uses the right decimals
        let deposits: Vec<_> = batch
            .statements()
            .iter()
            .filter(|s| s.sql().starts_with("INSERT INTO deposits"))
            .collect();
        assert_eq!(deposits.len(), 2, "expected two deposit inserts");

        for stmt in deposits {
            // token param is ?8 (index 7)
            let token_param = stmt.params().get(7).expect("token param present");
            let amount_param = stmt.params().get(9).expect("deposit_amount param present");

            match token_param {
                crate::local_db::query::SqlValue::Text(addr)
                    if addr == &format!("0x{:x}", token_a) =>
                {
                    let expected = Float::from_fixed_decimal(U256::from(1000u64), 6)
                        .unwrap()
                        .as_hex();
                    match amount_param {
                        crate::local_db::query::SqlValue::Text(val) => assert_eq!(val, &expected),
                        other => panic!("unexpected param type for deposit_amount: {other:?}"),
                    }
                }
                crate::local_db::query::SqlValue::Text(addr)
                    if addr == &format!("0x{:x}", token_b) =>
                {
                    let expected = Float::from_fixed_decimal(U256::from(1000u64), 9)
                        .unwrap()
                        .as_hex();
                    match amount_param {
                        crate::local_db::query::SqlValue::Text(val) => assert_eq!(val, &expected),
                        other => panic!("unexpected param type for deposit_amount: {other:?}"),
                    }
                }
                other => panic!("unexpected token param: {other:?}"),
            }
        }
    }

    #[test]
    fn withdraw_params_are_hex_and_uint256_is_32_bytes() {
        use alloy::{hex, primitives::U256};

        let pipeline = DefaultApplyPipeline::new();
        let raindex_id = sample_raindex_id();
        let token = Address::from([15u8; 20]);

        let batch = pipeline
            .build_batch(
                &build_target_info(&raindex_id, 1, 5),
                &[],
                &[withdraw_event(token)],
                &[],
                &[],
            )
            .expect("batch ok");

        let stmt = batch
            .statements()
            .iter()
            .find(|s| s.sql().starts_with("INSERT INTO withdrawals"))
            .expect("withdraw insert present");

        // token param is ?8 (index 7)
        match stmt.params().get(7) {
            Some(crate::local_db::query::SqlValue::Text(v)) => {
                assert_eq!(v, &format!("0x{:x}", token))
            }
            other => panic!("unexpected token param: {other:?}"),
        }

        // withdraw_amount_uint256 is ?12 (index 11) and must be 32-byte hex of 1500
        let expected_uint256 = hex::encode_prefixed(U256::from(1500u64).to_be_bytes::<32>());
        match stmt.params().get(11) {
            Some(crate::local_db::query::SqlValue::Text(v)) => assert_eq!(v, &expected_uint256),
            other => panic!("unexpected withdraw_amount_uint256 param: {other:?}"),
        }
    }
}
