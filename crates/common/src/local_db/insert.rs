use super::decode::{DecodedEvent, DecodedEventData, InterpreterStoreSetEvent};
use super::query::{SqlStatement, SqlStatementBatch, SqlValue};
use super::RaindexIdentifier;
use crate::erc20::TokenInfo;
use alloy::primitives::Bytes;
use alloy::sol_types::SolValue;
use alloy::{
    hex,
    primitives::{keccak256, Address, FixedBytes, B256, U256},
};
use rain_math_float::Float;
use raindex_bindings::IRaindexV7::{
    AddOrderV3, AfterClearV2, ClearV3, DepositV2, OrderV4, RemoveOrderV3, TakeOrderV4, WithdrawV2,
    IOV2,
};
use raindex_bindings::Raindex::MetaV1_2;
use std::collections::HashMap;
use std::convert::TryInto;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InsertError {
    #[error("Failed to parse hex string: {hex_str}")]
    HexParseError { hex_str: String },
    #[error("{field} exceeds u64 range")]
    IoIndexOverflow { field: &'static str },
    #[error("{side} {io_type} IO index {index} out of bounds (len {len})")]
    IoIndexOutOfBounds {
        side: &'static str,
        io_type: &'static str,
        index: usize,
        len: usize,
    },
    #[error("Float conversion failed: {0}")]
    FloatConversion(String),
    #[error("Missing decimals for token {token}")]
    MissingTokenDecimals { token: String },
}

fn u256_to_u64(value: &U256, field: &'static str) -> Result<u64, InsertError> {
    value
        .try_into()
        .map_err(|_| InsertError::IoIndexOverflow { field })
}

fn u64_to_usize(value: u64, field: &'static str) -> Result<usize, InsertError> {
    usize::try_from(value).map_err(|_| InsertError::IoIndexOverflow { field })
}

fn vault_id_by_index<'a>(
    ios: &'a [IOV2],
    index: usize,
    side: &'static str,
    io_type: &'static str,
) -> Result<&'a FixedBytes<32>, InsertError> {
    ios.get(index)
        .map(|io| &io.vaultId)
        .ok_or(InsertError::IoIndexOutOfBounds {
            side,
            io_type,
            index,
            len: ios.len(),
        })
}

struct EventContext {
    raindex_id: RaindexIdentifier,
    block_number: u64,
    block_timestamp: u64,
    transaction_hash: B256,
    log_index: u64,
}

fn event_context(
    raindex_id: &RaindexIdentifier,
    event: &DecodedEventData<DecodedEvent>,
) -> Result<EventContext, InsertError> {
    Ok(EventContext {
        raindex_id: raindex_id.clone(),
        block_number: u256_to_u64(&event.block_number, "block_number")?,
        block_timestamp: u256_to_u64(&event.block_timestamp, "block_timestamp")?,
        transaction_hash: event.transaction_hash,
        log_index: u256_to_u64(&event.log_index, "log_index")?,
    })
}

pub fn decoded_events_to_statements(
    raindex_id: &RaindexIdentifier,
    events: &[DecodedEventData<DecodedEvent>],
    decimals_by_token: &HashMap<Address, u8>,
) -> Result<SqlStatementBatch, InsertError> {
    let mut batch = SqlStatementBatch::new();

    for event in events {
        let context = event_context(raindex_id, event)?;
        match &event.decoded_data {
            DecodedEvent::DepositV2(decoded) => {
                batch.add(generate_deposit_statement(
                    &context,
                    decoded.as_ref(),
                    decimals_by_token,
                )?);
            }
            DecodedEvent::WithdrawV2(decoded) => {
                batch.add(generate_withdraw_statement(&context, decoded.as_ref()));
            }
            DecodedEvent::AddOrderV3(decoded) => {
                let add_event = decoded.as_ref();
                batch.add(generate_add_order_statement(&context, add_event));
                batch.extend(generate_order_ios_statements(&context, &add_event.order));
            }
            DecodedEvent::RemoveOrderV3(decoded) => {
                let remove_event = decoded.as_ref();
                batch.add(generate_remove_order_statement(&context, remove_event));
                batch.extend(generate_order_ios_statements(&context, &remove_event.order));
            }
            DecodedEvent::TakeOrderV4(decoded) => {
                let take_event = decoded.as_ref();
                batch.add(generate_take_order_statement(&context, take_event)?);
            }
            DecodedEvent::ClearV3(decoded) => {
                batch.add(generate_clear_v3_statement(&context, decoded.as_ref())?);
            }
            DecodedEvent::AfterClearV2(decoded) => {
                batch.add(generate_after_clear_statement(&context, decoded.as_ref()));
            }
            DecodedEvent::MetaV1_2(decoded) => {
                batch.add(generate_meta_statement(&context, decoded.as_ref()));
            }
            DecodedEvent::InterpreterStoreSet(decoded) => {
                batch.add(generate_store_set_statement(&context, decoded.as_ref()));
            }
            DecodedEvent::Unknown(decoded) => {
                eprintln!(
                    "Warning: Unknown event type for transaction {}: {}",
                    event.transaction_hash, decoded.note
                );
            }
        }
    }

    Ok(batch)
}

/// Build upsert SQL for erc20_tokens. Only include successfully fetched tokens.
pub fn generate_erc20_token_statements(
    raindex_id: &RaindexIdentifier,
    tokens: &[(Address, TokenInfo)],
) -> SqlStatementBatch {
    let mut batch = SqlStatementBatch::new();

    for (addr, info) in tokens.iter() {
        batch.add(SqlStatement::new_with_params(
            r#"INSERT INTO erc20_tokens (
            chain_id,
            raindex_address,
            token_address,
            name,
            symbol,
            decimals
        ) VALUES (
            ?1,
            ?2,
            ?3,
            ?4,
            ?5,
            ?6
        )
        ON CONFLICT(chain_id, raindex_address, token_address) DO UPDATE SET decimals = excluded.decimals, name = excluded.name, symbol = excluded.symbol;
        "#,
            [
                SqlValue::from(raindex_id.chain_id),
                SqlValue::from(raindex_id.raindex_address),
                SqlValue::from(*addr),
                SqlValue::from(info.name.clone()),
                SqlValue::from(info.symbol.clone()),
                SqlValue::from(info.decimals as u64),
            ],
        ));
    }

    batch
}

#[cfg(test)]
fn sql_string_literal(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('\'');
    for ch in value.chars() {
        match ch {
            '\'' => literal.push_str("''"),
            _ => literal.push(ch),
        }
    }
    literal.push('\'');
    literal
}

fn generate_deposit_statement(
    context: &EventContext,
    decoded: &DepositV2,
    decimals_by_token: &HashMap<Address, u8>,
) -> Result<SqlStatement, InsertError> {
    let decimals = decimals_by_token
        .get(&decoded.token)
        .copied()
        .ok_or_else(|| InsertError::MissingTokenDecimals {
            token: hex::encode_prefixed(decoded.token),
        })?;

    let deposit_amount_float = Float::from_fixed_decimal(decoded.depositAmountUint256, decimals)
        .map_err(|err| InsertError::FloatConversion(err.to_string()))?;

    Ok(SqlStatement::new_with_params(
        r#"INSERT INTO deposits (
    chain_id,
    raindex_address,
    block_number,
    block_timestamp,
    transaction_hash,
    log_index,
    sender,
    token,
    vault_id,
    deposit_amount,
    deposit_amount_uint256
) VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8,
    ?9,
    ?10,
    ?11
);
"#,
        vec![
            SqlValue::from(context.raindex_id.chain_id),
            SqlValue::from(context.raindex_id.raindex_address),
            SqlValue::from(context.block_number),
            SqlValue::from(context.block_timestamp),
            SqlValue::from(context.transaction_hash),
            SqlValue::from(context.log_index),
            SqlValue::from(decoded.sender),
            SqlValue::from(decoded.token),
            SqlValue::from(decoded.vaultId),
            SqlValue::from(deposit_amount_float),
            SqlValue::from(decoded.depositAmountUint256),
        ],
    ))
}

fn generate_withdraw_statement(context: &EventContext, decoded: &WithdrawV2) -> SqlStatement {
    SqlStatement::new_with_params(
        r#"INSERT INTO withdrawals (
    chain_id,
    raindex_address,
    block_number,
    block_timestamp,
    transaction_hash,
    log_index,
    sender,
    token,
    vault_id,
    target_amount,
    withdraw_amount,
    withdraw_amount_uint256
) VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8,
    ?9,
    ?10,
    ?11,
    ?12
);
"#,
        vec![
            SqlValue::from(context.raindex_id.chain_id),
            SqlValue::from(context.raindex_id.raindex_address),
            SqlValue::from(context.block_number),
            SqlValue::from(context.block_timestamp),
            SqlValue::from(context.transaction_hash),
            SqlValue::from(context.log_index),
            SqlValue::from(decoded.sender),
            SqlValue::from(decoded.token),
            SqlValue::from(decoded.vaultId),
            SqlValue::from(decoded.targetAmount),
            SqlValue::from(decoded.withdrawAmount),
            SqlValue::from(decoded.withdrawAmountUint256),
        ],
    )
}

fn generate_add_order_statement(context: &EventContext, decoded: &AddOrderV3) -> SqlStatement {
    SqlStatement::new_with_params(
        r#"INSERT INTO order_events (
    chain_id,
    raindex_address,
    block_number,
    block_timestamp,
    transaction_hash,
    log_index,
    event_type,
    sender,
    interpreter_address,
    store_address,
    order_hash,
    order_owner,
    order_nonce,
    order_bytes
) VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    'AddOrderV3',
    ?7,
    ?8,
    ?9,
    ?10,
    ?11,
    ?12,
    ?13
);
"#,
        vec![
            SqlValue::from(context.raindex_id.chain_id),
            SqlValue::from(context.raindex_id.raindex_address),
            SqlValue::from(context.block_number),
            SqlValue::from(context.block_timestamp),
            SqlValue::from(context.transaction_hash),
            SqlValue::from(context.log_index),
            SqlValue::from(decoded.sender),
            SqlValue::from(decoded.order.evaluable.interpreter),
            SqlValue::from(decoded.order.evaluable.store),
            SqlValue::from(decoded.orderHash),
            SqlValue::from(decoded.order.owner),
            SqlValue::from(decoded.order.nonce),
            SqlValue::from(Bytes::from(decoded.order.abi_encode())),
        ],
    )
}

fn generate_remove_order_statement(
    context: &EventContext,
    decoded: &RemoveOrderV3,
) -> SqlStatement {
    SqlStatement::new_with_params(
        r#"INSERT INTO order_events (
    chain_id,
    raindex_address,
    block_number,
    block_timestamp,
    transaction_hash,
    log_index,
    event_type,
    sender,
    interpreter_address,
    store_address,
    order_hash,
    order_owner,
    order_nonce,
    order_bytes
) VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    'RemoveOrderV3',
    ?7,
    ?8,
    ?9,
    ?10,
    ?11,
    ?12,
    ?13
);
"#,
        vec![
            SqlValue::from(context.raindex_id.chain_id),
            SqlValue::from(context.raindex_id.raindex_address),
            SqlValue::from(context.block_number),
            SqlValue::from(context.block_timestamp),
            SqlValue::from(context.transaction_hash),
            SqlValue::from(context.log_index),
            SqlValue::from(decoded.sender),
            SqlValue::from(decoded.order.evaluable.interpreter),
            SqlValue::from(decoded.order.evaluable.store),
            SqlValue::from(decoded.orderHash),
            SqlValue::from(decoded.order.owner),
            SqlValue::from(decoded.order.nonce),
            SqlValue::from(Bytes::from(decoded.order.abi_encode())),
        ],
    )
}

fn generate_take_order_statement(
    context: &EventContext,
    decoded: &TakeOrderV4,
) -> Result<SqlStatement, InsertError> {
    let input_io_index_u64 = u256_to_u64(&decoded.config.inputIOIndex, "inputIOIndex")?;
    let output_io_index_u64 = u256_to_u64(&decoded.config.outputIOIndex, "outputIOIndex")?;

    Ok(SqlStatement::new_with_params(
        r#"INSERT INTO take_orders (
    chain_id,
    raindex_address,
    block_number,
    block_timestamp,
    transaction_hash,
    log_index,
    sender,
    order_owner,
    order_nonce,
    input_io_index,
    output_io_index,
    taker_input,
    taker_output
) VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8,
    ?9,
    ?10,
    ?11,
    ?12,
    ?13
);
"#,
        vec![
            SqlValue::from(context.raindex_id.chain_id),
            SqlValue::from(context.raindex_id.raindex_address),
            SqlValue::from(context.block_number),
            SqlValue::from(context.block_timestamp),
            SqlValue::from(context.transaction_hash),
            SqlValue::from(context.log_index),
            SqlValue::from(decoded.sender),
            SqlValue::from(decoded.config.order.owner),
            SqlValue::from(decoded.config.order.nonce),
            SqlValue::from(input_io_index_u64),
            SqlValue::from(output_io_index_u64),
            SqlValue::from(decoded.input),
            SqlValue::from(decoded.output),
        ],
    ))
}

fn generate_clear_v3_statement(
    context: &EventContext,
    decoded: &ClearV3,
) -> Result<SqlStatement, InsertError> {
    let alice_input_io_index_u64 =
        u256_to_u64(&decoded.clearConfig.aliceInputIOIndex, "aliceInputIOIndex")?;
    let alice_output_io_index_u64 = u256_to_u64(
        &decoded.clearConfig.aliceOutputIOIndex,
        "aliceOutputIOIndex",
    )?;
    let bob_input_io_index_u64 =
        u256_to_u64(&decoded.clearConfig.bobInputIOIndex, "bobInputIOIndex")?;
    let bob_output_io_index_u64 =
        u256_to_u64(&decoded.clearConfig.bobOutputIOIndex, "bobOutputIOIndex")?;

    let alice_input_index = u64_to_usize(alice_input_io_index_u64, "aliceInputIOIndex")?;
    let alice_output_index = u64_to_usize(alice_output_io_index_u64, "aliceOutputIOIndex")?;
    let bob_input_index = u64_to_usize(bob_input_io_index_u64, "bobInputIOIndex")?;
    let bob_output_index = u64_to_usize(bob_output_io_index_u64, "bobOutputIOIndex")?;

    let alice_input_vault_id = vault_id_by_index(
        &decoded.alice.validInputs,
        alice_input_index,
        "alice",
        "input",
    )?;
    let alice_output_vault_id = vault_id_by_index(
        &decoded.alice.validOutputs,
        alice_output_index,
        "alice",
        "output",
    )?;
    let bob_input_vault_id =
        vault_id_by_index(&decoded.bob.validInputs, bob_input_index, "bob", "input")?;
    let bob_output_vault_id =
        vault_id_by_index(&decoded.bob.validOutputs, bob_output_index, "bob", "output")?;

    Ok(SqlStatement::new_with_params(
        r#"INSERT INTO clear_v3_events (
    chain_id,
    raindex_address,
    block_number,
    block_timestamp,
    transaction_hash,
    log_index,
    sender,
    alice_order_hash,
    alice_order_owner,
    alice_input_io_index,
    alice_output_io_index,
    alice_bounty_vault_id,
    alice_input_vault_id,
    alice_output_vault_id,
    bob_order_hash,
    bob_order_owner,
    bob_input_io_index,
    bob_output_io_index,
    bob_bounty_vault_id,
    bob_input_vault_id,
    bob_output_vault_id
) VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8,
    ?9,
    ?10,
    ?11,
    ?12,
    ?13,
    ?14,
    ?15,
    ?16,
    ?17,
    ?18,
    ?19,
    ?20,
    ?21
);
"#,
        vec![
            SqlValue::from(context.raindex_id.chain_id),
            SqlValue::from(context.raindex_id.raindex_address),
            SqlValue::from(context.block_number),
            SqlValue::from(context.block_timestamp),
            SqlValue::from(context.transaction_hash),
            SqlValue::from(context.log_index),
            SqlValue::from(decoded.sender),
            SqlValue::from(keccak256(decoded.alice.abi_encode())),
            SqlValue::from(decoded.alice.owner),
            SqlValue::from(alice_input_io_index_u64),
            SqlValue::from(alice_output_io_index_u64),
            SqlValue::from(decoded.clearConfig.aliceBountyVaultId),
            SqlValue::from(*alice_input_vault_id),
            SqlValue::from(*alice_output_vault_id),
            SqlValue::from(keccak256(decoded.bob.abi_encode())),
            SqlValue::from(decoded.bob.owner),
            SqlValue::from(bob_input_io_index_u64),
            SqlValue::from(bob_output_io_index_u64),
            SqlValue::from(decoded.clearConfig.bobBountyVaultId),
            SqlValue::from(*bob_input_vault_id),
            SqlValue::from(*bob_output_vault_id),
        ],
    ))
}

fn generate_after_clear_statement(context: &EventContext, decoded: &AfterClearV2) -> SqlStatement {
    SqlStatement::new_with_params(
        r#"INSERT INTO after_clear_v2_events (
    chain_id,
    raindex_address,
    block_number,
    block_timestamp,
    transaction_hash,
    log_index,
    sender,
    alice_input,
    alice_output,
    bob_input,
    bob_output
) VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8,
    ?9,
    ?10,
    ?11
);
"#,
        vec![
            SqlValue::from(context.raindex_id.chain_id),
            SqlValue::from(context.raindex_id.raindex_address),
            SqlValue::from(context.block_number),
            SqlValue::from(context.block_timestamp),
            SqlValue::from(context.transaction_hash),
            SqlValue::from(context.log_index),
            SqlValue::from(decoded.sender),
            SqlValue::from(decoded.clearStateChange.aliceInput),
            SqlValue::from(decoded.clearStateChange.aliceOutput),
            SqlValue::from(decoded.clearStateChange.bobInput),
            SqlValue::from(decoded.clearStateChange.bobOutput),
        ],
    )
}

fn generate_meta_statement(context: &EventContext, decoded: &MetaV1_2) -> SqlStatement {
    SqlStatement::new_with_params(
        r#"INSERT INTO meta_events (
    chain_id,
    raindex_address,
    block_number,
    block_timestamp,
    transaction_hash,
    log_index,
    sender,
    subject,
    meta
) VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8,
    ?9
);
"#,
        vec![
            SqlValue::from(context.raindex_id.chain_id),
            SqlValue::from(context.raindex_id.raindex_address),
            SqlValue::from(context.block_number),
            SqlValue::from(context.block_timestamp),
            SqlValue::from(context.transaction_hash),
            SqlValue::from(context.log_index),
            SqlValue::from(decoded.sender),
            SqlValue::from(decoded.subject),
            SqlValue::from(decoded.meta.clone()),
        ],
    )
}

fn generate_store_set_statement(
    context: &EventContext,
    decoded: &InterpreterStoreSetEvent,
) -> SqlStatement {
    SqlStatement::new_with_params(
        r#"INSERT INTO interpreter_store_sets (
    chain_id,
    raindex_address,
    store_address,
    block_number,
    block_timestamp,
    transaction_hash,
    log_index,
    namespace,
    key,
    value
) VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8,
    ?9,
    ?10
) ON CONFLICT(chain_id, raindex_address, transaction_hash, log_index) DO UPDATE SET
    store_address = excluded.store_address,
    block_number = excluded.block_number,
    block_timestamp = excluded.block_timestamp,
    namespace = excluded.namespace,
    key = excluded.key,
    value = excluded.value;
"#,
        vec![
            SqlValue::from(context.raindex_id.chain_id),
            SqlValue::from(context.raindex_id.raindex_address),
            SqlValue::from(decoded.store_address),
            SqlValue::from(context.block_number),
            SqlValue::from(context.block_timestamp),
            SqlValue::from(context.transaction_hash),
            SqlValue::from(context.log_index),
            SqlValue::from(decoded.payload.namespace),
            SqlValue::from(decoded.payload.key),
            SqlValue::from(decoded.payload.value),
        ],
    )
}

fn generate_order_ios_statements(context: &EventContext, order: &OrderV4) -> SqlStatementBatch {
    const INSERT_IO_SQL: &str = r#"INSERT INTO order_ios (
    chain_id,
    raindex_address,
    transaction_hash,
    log_index,
    io_index,
    io_type,
    token,
    vault_id
) VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8
);
"#;

    let mut batch = SqlStatementBatch::new();
    let transaction_hash = context.transaction_hash;
    let log_index = context.log_index;

    for (index, input) in order.validInputs.iter().enumerate() {
        batch.add(SqlStatement::new_with_params(
            INSERT_IO_SQL,
            vec![
                SqlValue::from(context.raindex_id.chain_id),
                SqlValue::from(context.raindex_id.raindex_address),
                SqlValue::from(transaction_hash),
                SqlValue::from(log_index),
                SqlValue::from(index as u64),
                SqlValue::from("input"),
                SqlValue::from(input.token),
                SqlValue::from(input.vaultId),
            ],
        ));
    }

    for (index, output) in order.validOutputs.iter().enumerate() {
        batch.add(SqlStatement::new_with_params(
            INSERT_IO_SQL,
            vec![
                SqlValue::from(context.raindex_id.chain_id),
                SqlValue::from(context.raindex_id.raindex_address),
                SqlValue::from(transaction_hash),
                SqlValue::from(log_index),
                SqlValue::from(index as u64),
                SqlValue::from("output"),
                SqlValue::from(output.token),
                SqlValue::from(output.vaultId),
            ],
        ));
    }

    batch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_db::decode::{EventType, UnknownEventDecoded};
    use crate::local_db::query::SqlValue;
    use alloy::hex;
    use alloy::primitives::{b256, Address, Bytes, FixedBytes, B256, U256};
    use itertools::Itertools;
    use raindex_bindings::IInterpreterStoreV3::Set;
    use raindex_bindings::IRaindexV7::{
        ClearConfigV2, ClearStateChangeV2, EvaluableV4, SignedContextV2, TakeOrderConfigV5,
    };
    use std::collections::HashMap;

    fn encode_u256_prefixed(value: &U256) -> String {
        hex::encode_prefixed(value.to_be_bytes::<32>())
    }

    fn parse_u256(value: &str) -> U256 {
        let trimmed = value.trim();
        if let Some(hex) = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
        {
            U256::from_str_radix(hex, 16).expect("valid hex literal")
        } else {
            U256::from_str_radix(trimmed, 10).expect("valid decimal literal")
        }
    }

    fn build_event(
        event_type: EventType,
        block_number: &str,
        block_timestamp: &str,
        transaction_hash: B256,
        log_index: &str,
        decoded: DecodedEvent,
    ) -> DecodedEventData<DecodedEvent> {
        DecodedEventData {
            event_type,
            block_number: parse_u256(block_number),
            block_timestamp: parse_u256(block_timestamp),
            transaction_hash,
            log_index: parse_u256(log_index),
            decoded_data: decoded,
        }
    }

    fn sample_order() -> OrderV4 {
        OrderV4 {
            owner: Address::from([1u8; 20]),
            nonce: U256::from(1).into(),
            evaluable: EvaluableV4 {
                interpreter: Address::from([2u8; 20]),
                store: Address::from([3u8; 20]),
                bytecode: Bytes::from(vec![0x01, 0x02, 0x03, 0x04]),
            },
            validInputs: vec![
                IOV2 {
                    token: Address::from([4u8; 20]),
                    vaultId: U256::from(0x64).into(),
                },
                IOV2 {
                    token: Address::from([5u8; 20]),
                    vaultId: U256::from(0xc8).into(),
                },
            ],
            validOutputs: vec![IOV2 {
                token: Address::from([6u8; 20]),
                vaultId: U256::from(0x12c).into(),
            }],
        }
    }

    fn sample_add_event() -> DecodedEventData<DecodedEvent> {
        let add = AddOrderV3 {
            sender: Address::from([0x07; 20]),
            orderHash: FixedBytes::<32>::from([0x08; 32]),
            order: sample_order(),
        };

        build_event(
            EventType::AddOrderV3,
            "0x100",
            "0x200",
            b256!("0x000000000000000000000000000000000000000000000000000000000000aaa0"),
            "0x1",
            DecodedEvent::AddOrderV3(Box::new(add)),
        )
    }

    fn sample_remove_event() -> DecodedEventData<DecodedEvent> {
        let remove = RemoveOrderV3 {
            sender: Address::from([0x0b; 20]),
            orderHash: FixedBytes::<32>::from([0x0c; 32]),
            order: sample_order(),
        };

        build_event(
            EventType::RemoveOrderV3,
            "0x101",
            "0x201",
            b256!("0x000000000000000000000000000000000000000000000000000000000000bb00"),
            "0x2",
            DecodedEvent::RemoveOrderV3(Box::new(remove)),
        )
    }

    fn sample_clear_event() -> DecodedEventData<DecodedEvent> {
        let clear = ClearV3 {
            sender: Address::from([0x11; 20]),
            alice: sample_order(),
            bob: OrderV4 {
                owner: Address::from([0x12; 20]),
                nonce: U256::from(2).into(),
                evaluable: EvaluableV4 {
                    interpreter: Address::from([0x13; 20]),
                    store: Address::from([0x14; 20]),
                    bytecode: Bytes::from(vec![0x05, 0x06, 0x07, 0x08]),
                },
                validInputs: vec![IOV2 {
                    token: Address::from([0x15; 20]),
                    vaultId: U256::from(0x2bc).into(),
                }],
                validOutputs: vec![IOV2 {
                    token: Address::from([0x16; 20]),
                    vaultId: U256::from(0x320).into(),
                }],
            },
            clearConfig: ClearConfigV2 {
                aliceInputIOIndex: U256::from(0),
                aliceOutputIOIndex: U256::from(0),
                bobInputIOIndex: U256::from(0),
                bobOutputIOIndex: U256::from(0),
                aliceBountyVaultId: U256::from(0).into(),
                bobBountyVaultId: U256::from(0).into(),
            },
        };

        build_event(
            EventType::ClearV3,
            "0x100",
            "0x200",
            b256!("0x000000000000000000000000000000000000000000000000000000000000abc0"),
            "0x1",
            DecodedEvent::ClearV3(Box::new(clear)),
        )
    }

    fn sample_take_event() -> DecodedEventData<DecodedEvent> {
        let take = TakeOrderV4 {
            sender: Address::from([0x09; 20]),
            config: TakeOrderConfigV5 {
                order: sample_order(),
                inputIOIndex: U256::from(0),
                outputIOIndex: U256::from(0),
                signedContext: vec![SignedContextV2 {
                    signer: Address::from([0x0a; 20]),
                    domainSeparator: alloy::primitives::B256::repeat_byte(0xd5),
                    context: vec![U256::from(0x2a).into()],
                    signature: Bytes::from(vec![0x11, 0x22, 0x33]),
                }],
            },
            input: U256::from(0x3e8).into(),
            output: U256::from(0x7d0).into(),
        };

        build_event(
            EventType::TakeOrderV4,
            "0x101",
            "0x201",
            b256!("0x000000000000000000000000000000000000000000000000000000000000def0"),
            "0x2",
            DecodedEvent::TakeOrderV4(Box::new(take)),
        )
    }

    fn sample_after_clear_event() -> DecodedEventData<DecodedEvent> {
        let after_clear = AfterClearV2 {
            sender: Address::from([0x17; 20]),
            clearStateChange: ClearStateChangeV2 {
                aliceInput: U256::from(0x1f4).into(),
                aliceOutput: U256::from(0x258).into(),
                bobInput: U256::from(0x2bc).into(),
                bobOutput: U256::from(0x320).into(),
            },
        };

        build_event(
            EventType::AfterClearV2,
            "0x103",
            "0x203",
            b256!("0x0000000000000000000000000000000000000000000000000000000000002468"),
            "0x5",
            DecodedEvent::AfterClearV2(Box::new(after_clear)),
        )
    }

    fn sample_meta_event() -> DecodedEventData<DecodedEvent> {
        let meta = MetaV1_2 {
            sender: Address::from([0x18; 20]),
            subject: FixedBytes::<32>::from([0x19; 32]),
            meta: Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]),
        };

        build_event(
            EventType::MetaV1_2,
            "0x104",
            "0x204",
            b256!("0x0000000000000000000000000000000000000000000000000000000000001122"),
            "0x6",
            DecodedEvent::MetaV1_2(Box::new(meta)),
        )
    }

    fn sample_store_set_event() -> DecodedEventData<DecodedEvent> {
        let store = InterpreterStoreSetEvent {
            store_address: Address::from([0x30; 20]),
            payload: Set {
                namespace: U256::from_be_bytes([0xaa; 32]),
                key: FixedBytes::<32>::from([0xbb; 32]),
                value: FixedBytes::<32>::from([0xcc; 32]),
            },
        };

        build_event(
            EventType::InterpreterStoreSet,
            "0x200",
            "0x300",
            b256!("0x000000000000000000000000000000000000000000000000000000000000feed"),
            "0x4",
            DecodedEvent::InterpreterStoreSet(Box::new(store)),
        )
    }

    fn sample_deposit_event() -> DecodedEventData<DecodedEvent> {
        let deposit = DepositV2 {
            sender: Address::from([0x0d; 20]),
            token: Address::from([0x0e; 20]),
            vaultId: U256::from(0x258).into(),
            depositAmountUint256: U256::from(0xfa0),
        };

        build_event(
            EventType::DepositV2,
            "0x102",
            "0x202",
            b256!("0x0000000000000000000000000000000000000000000000000000000000001230"),
            "0x3",
            DecodedEvent::DepositV2(Box::new(deposit)),
        )
    }

    #[test]
    fn store_set_sql_generation() {
        let event = sample_store_set_event();
        let context = event_context(&RaindexIdentifier::new(1, Address::ZERO), &event).unwrap();
        let DecodedEvent::InterpreterStoreSet(decoded) = &event.decoded_data else {
            unreachable!()
        };
        let statement = generate_store_set_statement(&context, decoded.as_ref());
        assert!(statement
            .sql()
            .contains("INSERT INTO interpreter_store_sets"));
        assert!(statement.sql().contains("?10"));
        let params = statement.params();
        assert_eq!(params.len(), 10);
        assert!(matches!(
            params[0],
            SqlValue::U64(v) if v == context.raindex_id.chain_id as u64
        ));
        assert!(matches!(
            params[1],
            SqlValue::Text(ref v)
                if v == &hex::encode_prefixed(context.raindex_id.raindex_address)
        ));
        assert!(
            matches!(params[2], SqlValue::Text(ref v) if v == &hex::encode_prefixed(decoded.store_address))
        );
        assert!(matches!(params[3], SqlValue::U64(v) if v == context.block_number));
        assert!(matches!(
            params[4],
            SqlValue::U64(v) if v == context.block_timestamp
        ));
        assert!(
            matches!(params[5], SqlValue::Text(ref v) if v == &hex::encode_prefixed(context.transaction_hash))
        );
        assert!(matches!(params[6], SqlValue::U64(v) if v == context.log_index));
        assert!(matches!(
            params[7],
            SqlValue::Text(ref v)
                if v == &encode_u256_prefixed(&decoded.payload.namespace)
        ));
        assert!(
            matches!(params[8], SqlValue::Text(ref v) if v == &hex::encode_prefixed(decoded.payload.key))
        );
        assert!(
            matches!(params[9], SqlValue::Text(ref v) if v == &hex::encode_prefixed(decoded.payload.value))
        );
    }

    #[test]
    fn deposit_statement_generation() {
        let event = sample_deposit_event();
        let context = event_context(&RaindexIdentifier::new(1, Address::ZERO), &event).unwrap();
        let DecodedEvent::DepositV2(decoded) = &event.decoded_data else {
            unreachable!()
        };
        let deposit = decoded.as_ref();
        let mut decimals = HashMap::new();
        decimals.insert(deposit.token, 6);
        let statement = generate_deposit_statement(&context, deposit, &decimals).unwrap();
        assert!(statement.sql().contains("INSERT INTO deposits"));
        assert!(statement.sql().contains("?11"));
        let params = statement.params();
        assert_eq!(params.len(), 11);
        assert!(matches!(
            params[0],
            SqlValue::U64(v) if v == context.raindex_id.chain_id as u64
        ));
        assert!(matches!(
            params[1],
            SqlValue::Text(ref v)
                if v == &hex::encode_prefixed(context.raindex_id.raindex_address)
        ));
        assert!(matches!(params[2], SqlValue::U64(v) if v == context.block_number));
        assert!(matches!(params[3], SqlValue::U64(v) if v == context.block_timestamp));
        assert!(matches!(params[5], SqlValue::U64(v) if v == context.log_index));
        assert!(matches!(
            params[6],
            SqlValue::Text(ref v) if v == &hex::encode_prefixed(deposit.sender)
        ));
        assert!(matches!(
            params[7],
            SqlValue::Text(ref v) if v == &hex::encode_prefixed(deposit.token)
        ));
        let expected_uint256 = encode_u256_prefixed(&deposit.depositAmountUint256);
        assert!(matches!(params[10], SqlValue::Text(ref v) if v == &expected_uint256));
    }

    #[test]
    fn add_order_sql_includes_evaluable_addresses() {
        let event = sample_add_event();
        let context = event_context(&RaindexIdentifier::new(1, Address::ZERO), &event).unwrap();
        let DecodedEvent::AddOrderV3(decoded) = &event.decoded_data else {
            unreachable!()
        };
        let statement = generate_add_order_statement(&context, decoded);
        assert!(statement.sql().contains("order_bytes"));
        assert!(statement.sql().contains("interpreter_address"));
        assert!(statement.sql().contains("store_address"));
        assert!(statement.sql().contains("?13"));
        let params = statement.params();
        assert_eq!(params.len(), 13);
        assert!(matches!(
            params[0],
            SqlValue::U64(v) if v == context.raindex_id.chain_id as u64
        ));
        assert!(matches!(
            params[1],
            SqlValue::Text(ref v)
                if v == &hex::encode_prefixed(context.raindex_id.raindex_address)
        ));
        assert!(matches!(params[5], SqlValue::U64(v) if v == context.log_index));
        let expected_sender = hex::encode_prefixed(decoded.sender);
        assert!(matches!(params[6], SqlValue::Text(ref v) if v == &expected_sender));
        let expected_interpreter = hex::encode_prefixed(decoded.order.evaluable.interpreter);
        assert!(matches!(params[7], SqlValue::Text(ref v) if v == &expected_interpreter));
        let expected_store = hex::encode_prefixed(decoded.order.evaluable.store);
        assert!(matches!(params[8], SqlValue::Text(ref v) if v == &expected_store));
        let expected_bytes = hex::encode_prefixed(decoded.order.abi_encode());
        assert!(matches!(params[12], SqlValue::Text(ref v) if v == &expected_bytes));
        let ios_batch = generate_order_ios_statements(&context, &decoded.order);
        let expected_ios_len = decoded.order.validInputs.len() + decoded.order.validOutputs.len();
        assert_eq!(ios_batch.len(), expected_ios_len);
        assert!(ios_batch
            .statements()
            .iter()
            .all(|stmt| stmt.sql().contains("INSERT INTO order_ios")));
    }

    #[test]
    fn remove_order_sql_includes_evaluable_addresses() {
        let event = sample_remove_event();
        let context = event_context(&RaindexIdentifier::new(1, Address::ZERO), &event).unwrap();
        let DecodedEvent::RemoveOrderV3(decoded) = &event.decoded_data else {
            unreachable!()
        };
        let statement = generate_remove_order_statement(&context, decoded);
        assert!(statement.sql().contains("'RemoveOrderV3'"));
        assert!(statement.sql().contains("interpreter_address"));
        assert!(statement.sql().contains("store_address"));
        let params = statement.params();
        assert_eq!(params.len(), 13);
        assert!(matches!(
            params[0],
            SqlValue::U64(v) if v == context.raindex_id.chain_id as u64
        ));
        assert!(matches!(
            params[1],
            SqlValue::Text(ref v)
                if v == &hex::encode_prefixed(context.raindex_id.raindex_address)
        ));
        assert!(matches!(params[5], SqlValue::U64(v) if v == context.log_index));
        let expected_sender = hex::encode_prefixed(decoded.sender);
        assert!(matches!(params[6], SqlValue::Text(ref v) if v == &expected_sender));
        let expected_interpreter = hex::encode_prefixed(decoded.order.evaluable.interpreter);
        assert!(matches!(params[7], SqlValue::Text(ref v) if v == &expected_interpreter));
        let expected_store = hex::encode_prefixed(decoded.order.evaluable.store);
        assert!(matches!(params[8], SqlValue::Text(ref v) if v == &expected_store));
        let expected_bytes = hex::encode_prefixed(decoded.order.abi_encode());
        assert!(matches!(params[12], SqlValue::Text(ref v) if v == &expected_bytes));
    }

    #[test]
    fn take_order_sql_generation() {
        let event = sample_take_event();
        let context = event_context(&RaindexIdentifier::new(1, Address::ZERO), &event).unwrap();
        let DecodedEvent::TakeOrderV4(decoded) = &event.decoded_data else {
            unreachable!()
        };
        let statement = generate_take_order_statement(&context, decoded).unwrap();
        assert!(statement.sql().contains("INSERT INTO take_orders"));
        assert!(statement.sql().contains("?13"));
        let params = statement.params();
        assert_eq!(params.len(), 13);
        assert!(matches!(
            params[0],
            SqlValue::U64(v) if v == context.raindex_id.chain_id as u64
        ));
        assert!(matches!(
            params[1],
            SqlValue::Text(ref v)
                if v == &hex::encode_prefixed(context.raindex_id.raindex_address)
        ));
        assert!(matches!(params[2], SqlValue::U64(v) if v == context.block_number));
        assert!(matches!(params[3], SqlValue::U64(v) if v == context.block_timestamp));
        assert!(matches!(params[5], SqlValue::U64(v) if v == context.log_index));
        assert!(
            matches!(params[6], SqlValue::Text(ref v) if v == "0x0909090909090909090909090909090909090909")
        );

        let batch =
            decoded_events_to_statements(&context.raindex_id, &[event], &HashMap::new()).unwrap();
        let sql: Vec<_> = batch.statements().iter().map(|stmt| stmt.sql()).collect();
        assert!(sql
            .iter()
            .any(|stmt| stmt.contains("INSERT INTO take_orders")));
        assert!(!sql.iter().any(|stmt| stmt.contains("take_order_contexts")));
        assert!(!sql.iter().any(|stmt| stmt.contains("context_values")));
    }

    #[test]
    fn after_clear_sql_generation() {
        let event = sample_after_clear_event();
        let context = event_context(&RaindexIdentifier::new(1, Address::ZERO), &event).unwrap();
        let DecodedEvent::AfterClearV2(decoded) = &event.decoded_data else {
            unreachable!()
        };
        let statement = generate_after_clear_statement(&context, decoded);
        assert!(statement
            .sql()
            .contains("INSERT INTO after_clear_v2_events"));
        assert!(statement.sql().contains("?11"));
        let params = statement.params();
        assert_eq!(params.len(), 11);
        assert!(matches!(
            params[0],
            SqlValue::U64(v) if v == context.raindex_id.chain_id as u64
        ));
        assert!(matches!(
            params[1],
            SqlValue::Text(ref v)
                if v == &hex::encode_prefixed(context.raindex_id.raindex_address)
        ));
        assert!(matches!(params[2], SqlValue::U64(v) if v == context.block_number));
        assert!(matches!(params[3], SqlValue::U64(v) if v == context.block_timestamp));
        assert!(matches!(params[5], SqlValue::U64(v) if v == context.log_index));
        assert!(
            matches!(params[6], SqlValue::Text(ref v) if v == &hex::encode_prefixed(decoded.sender))
        );
        let expected_alice_input = hex::encode_prefixed(decoded.clearStateChange.aliceInput);
        assert!(matches!(params[7], SqlValue::Text(ref v) if v == &expected_alice_input));
        let expected_bob_output = hex::encode_prefixed(decoded.clearStateChange.bobOutput);
        assert!(matches!(params[10], SqlValue::Text(ref v) if v == &expected_bob_output));
    }

    #[test]
    fn meta_sql_generation() {
        let event = sample_meta_event();
        let context = event_context(&RaindexIdentifier::new(1, Address::ZERO), &event).unwrap();
        let DecodedEvent::MetaV1_2(decoded) = &event.decoded_data else {
            unreachable!()
        };
        let statement = generate_meta_statement(&context, decoded);
        assert!(statement.sql().contains("INSERT INTO meta_events"));
        assert!(statement.sql().contains("?9"));
        let params = statement.params();
        assert_eq!(params.len(), 9);
        assert!(matches!(
            params[0],
            SqlValue::U64(v) if v == context.raindex_id.chain_id as u64
        ));
        assert!(matches!(
            params[1],
            SqlValue::Text(ref v)
                if v == &hex::encode_prefixed(context.raindex_id.raindex_address)
        ));
        assert!(matches!(params[2], SqlValue::U64(v) if v == context.block_number));
        assert!(matches!(params[3], SqlValue::U64(v) if v == context.block_timestamp));
        assert!(matches!(params[5], SqlValue::U64(v) if v == context.log_index));
        let expected_sender = hex::encode_prefixed(decoded.sender);
        assert!(matches!(params[6], SqlValue::Text(ref v) if v == &expected_sender));
        let expected_subject = hex::encode_prefixed(decoded.subject);
        assert!(matches!(params[7], SqlValue::Text(ref v) if v == &expected_subject));
        let expected_meta = hex::encode_prefixed(&decoded.meta);
        assert!(matches!(params[8], SqlValue::Text(ref v) if v == &expected_meta));
    }

    #[test]
    fn clear_v3_sql_generation() {
        let event = sample_clear_event();
        let context = event_context(&RaindexIdentifier::new(1, Address::ZERO), &event).unwrap();
        let DecodedEvent::ClearV3(decoded) = &event.decoded_data else {
            unreachable!()
        };
        let statement = generate_clear_v3_statement(&context, decoded).unwrap();
        assert!(statement.sql().contains("INSERT INTO clear_v3_events"));
        assert!(statement.sql().contains("?20"));
        let params = statement.params();
        assert_eq!(params.len(), 21);
        assert!(matches!(
            params[0],
            SqlValue::U64(v) if v == context.raindex_id.chain_id as u64
        ));
        assert!(matches!(
            params[1],
            SqlValue::Text(ref v)
                if v == &hex::encode_prefixed(context.raindex_id.raindex_address)
        ));
        assert!(matches!(params[2], SqlValue::U64(v) if v == context.block_number));
        assert!(matches!(params[3], SqlValue::U64(v) if v == context.block_timestamp));
        assert!(matches!(params[5], SqlValue::U64(v) if v == context.log_index));
        let expected_alice_input_vault = hex::encode_prefixed(decoded.alice.validInputs[0].vaultId);
        assert!(matches!(params[12], SqlValue::Text(ref v) if v == &expected_alice_input_vault));
        let expected_bob_input_vault = hex::encode_prefixed(decoded.bob.validInputs[0].vaultId);
        assert!(matches!(params[19], SqlValue::Text(ref v) if v == &expected_bob_input_vault));
    }

    #[test]
    fn decoded_events_to_statements_multiple_events() {
        let clear_event = sample_clear_event();
        let deposit_event = sample_deposit_event();
        let mut decimals = HashMap::new();
        if let DecodedEvent::DepositV2(deposit) = &deposit_event.decoded_data {
            decimals.insert(deposit.token, 6);
        }
        let batch = decoded_events_to_statements(
            &RaindexIdentifier::new(1, Address::from([0x11; 20])),
            &[deposit_event, clear_event],
            &decimals,
        )
        .unwrap()
        .ensure_transaction();
        let sql = batch.statements().iter().map(|stmt| stmt.sql()).join("\n");
        assert!(sql.contains("INSERT INTO deposits"));
        assert!(sql.contains("INSERT INTO clear_v3_events"));
    }

    #[test]
    fn unknown_event_is_logged() {
        let unknown_event = build_event(
            EventType::Unknown,
            "0x0",
            "0x0",
            b256!("0x000000000000000000000000000000000000000000000000000000000000beef"),
            "0x0",
            DecodedEvent::Unknown(UnknownEventDecoded {
                raw_data: "0xdead".into(),
                note: "n/a".into(),
            }),
        );
        let batch = decoded_events_to_statements(
            &RaindexIdentifier::new(1, Address::from([0x11; 20])),
            &[unknown_event],
            &HashMap::new(),
        )
        .unwrap()
        .ensure_transaction();
        let sql = batch.statements().iter().map(|stmt| stmt.sql()).join("\n");
        assert!(sql.contains("BEGIN TRANSACTION"));
    }

    #[test]
    fn test_generate_erc20_token_statements_builder() {
        let addr1 = Address::from([1u8; 20]);
        let addr2 = Address::from([2u8; 20]);
        let tokens = vec![
            (
                addr1,
                TokenInfo {
                    decimals: 18,
                    name: "Foo Token".to_string(),
                    symbol: "FOO".to_string(),
                },
            ),
            (
                addr2,
                TokenInfo {
                    decimals: 6,
                    name: "Bar's Token".to_string(),
                    symbol: "B'AR".to_string(),
                },
            ),
        ];

        let raindex = Address::from([0x11; 20]);
        let batch = generate_erc20_token_statements(&RaindexIdentifier::new(1, raindex), &tokens);
        assert_eq!(batch.len(), tokens.len());

        for (statement, (expected_addr, expected_info)) in
            batch.statements().iter().zip(tokens.iter())
        {
            let sql = statement.sql();
            assert!(sql.contains("INSERT INTO erc20_tokens"));
            assert!(sql.contains("ON CONFLICT(chain_id, raindex_address, token_address) DO UPDATE"));

            let params = statement.params();
            assert_eq!(params.len(), 6);
            assert!(matches!(params[0], SqlValue::U64(1u64)));
            assert!(matches!(
                &params[1],
                SqlValue::Text(addr) if addr == &format!("0x{:x}", raindex)
            ));
            assert!(matches!(
                &params[2],
                SqlValue::Text(addr) if addr == &format!("0x{:x}", expected_addr)
            ));
            assert!(matches!(
                &params[3],
                SqlValue::Text(name) if name == &expected_info.name
            ));
            assert!(matches!(
                &params[4],
                SqlValue::Text(symbol) if symbol == &expected_info.symbol
            ));
            assert!(matches!(
                params[5],
                SqlValue::U64(value) if value == expected_info.decimals as u64
            ));
        }
    }

    fn assert_literal_round_trip(literal: &str, expected: &str) {
        assert!(literal.starts_with('\'') && literal.ends_with('\''));
        let mut chars = literal[1..literal.len() - 1].chars().peekable();
        let mut reconstructed = String::new();
        while let Some(ch) = chars.next() {
            if ch == '\'' {
                match chars.next() {
                    Some('\'') => reconstructed.push('\''),
                    _ => panic!("found unescaped single quote in literal: {}", literal),
                }
            } else {
                reconstructed.push(ch);
            }
        }
        assert_eq!(reconstructed, expected);
    }

    #[test]
    fn sql_string_literal_round_trip_special_characters() {
        let value = "Alice's \"Token\" \\ Backslash\nNewline\rCarriage\tTab; DROP TABLE tokens; --";
        let literal = super::sql_string_literal(value);
        assert!(literal.contains("''"));
        assert!(literal.contains('"'));
        assert!(literal.contains('\\'));
        assert_literal_round_trip(&literal, value);
    }

    #[test]
    fn generate_erc20_token_statements_escapes_special_characters() {
        let addr = Address::from([3u8; 20]);
        let name =
            "Dangerous 'Name' \"Quotes\" \\ Backslash\nNewline; DROP TABLE tokens; --".to_string();
        let symbol = "SYM'\"\\\n;--".to_string();
        let tokens = vec![(
            addr,
            TokenInfo {
                decimals: 8,
                name: name.clone(),
                symbol: symbol.clone(),
            },
        )];

        let raindex = Address::from([0x22; 20]);
        let batch = generate_erc20_token_statements(&RaindexIdentifier::new(5, raindex), &tokens);
        assert_eq!(batch.len(), 1);
        let statement = &batch.statements()[0];
        let sql = statement.sql();
        assert!(sql.contains("INSERT INTO erc20_tokens"));

        let params = statement.params();
        assert_eq!(params.len(), 6);
        assert!(matches!(params[0], SqlValue::U64(5u64)));
        assert!(matches!(
            &params[1],
            SqlValue::Text(raindex_param) if raindex_param == &format!("0x{:x}", raindex)
        ));
        assert!(matches!(
            &params[2],
            SqlValue::Text(token_param) if token_param == &format!("0x{:x}", addr)
        ));
        assert!(matches!(
            &params[3],
            SqlValue::Text(name_param) if name_param == &name
        ));
        assert!(matches!(
            &params[4],
            SqlValue::Text(symbol_param) if symbol_param == &symbol
        ));
        assert!(matches!(params[5], SqlValue::U64(8u64)));

        // Ensure no in-place escaping mangles the stored strings.
        assert_literal_round_trip(&super::sql_string_literal(&name), &name);
        assert_literal_round_trip(&super::sql_string_literal(&symbol), &symbol);
    }
}
