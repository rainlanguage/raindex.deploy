use crate::rpc_client::LogEntryResponse;
use alloy::{
    hex,
    primitives::{Address, B256, U256},
    sol_types::{abi::token::WordToken, SolEvent},
};
use core::convert::TryFrom;
use raindex_bindings::{
    IInterpreterStoreV3::Set,
    IRaindexV7::{
        AddOrderV3, AfterClearV2, ClearV3, DepositV2, RemoveOrderV3, TakeOrderV4, WithdrawV2,
    },
    Raindex::MetaV1_2,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("Hex decode error: {0}")]
    HexDecode(#[from] hex::FromHexError),
    #[error("ABI decode error: {0}")]
    AbiDecode(String),
    #[error("log at index {index} missing required field {field}")]
    MissingRequiredField { field: &'static str, index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    AddOrderV3,
    TakeOrderV4,
    WithdrawV2,
    DepositV2,
    RemoveOrderV3,
    ClearV3,
    AfterClearV2,
    MetaV1_2,
    InterpreterStoreSet,
    Unknown,
}

impl EventType {
    fn from_topic(topic: &[u8]) -> Self {
        if topic == AddOrderV3::SIGNATURE_HASH.as_slice() {
            return Self::AddOrderV3;
        }
        if topic == TakeOrderV4::SIGNATURE_HASH.as_slice() {
            return Self::TakeOrderV4;
        }
        if topic == WithdrawV2::SIGNATURE_HASH.as_slice() {
            return Self::WithdrawV2;
        }
        if topic == DepositV2::SIGNATURE_HASH.as_slice() {
            return Self::DepositV2;
        }
        if topic == RemoveOrderV3::SIGNATURE_HASH.as_slice() {
            return Self::RemoveOrderV3;
        }
        if topic == ClearV3::SIGNATURE_HASH.as_slice() {
            return Self::ClearV3;
        }
        if topic == AfterClearV2::SIGNATURE_HASH.as_slice() {
            return Self::AfterClearV2;
        }
        if topic == MetaV1_2::SIGNATURE_HASH.as_slice() {
            return Self::MetaV1_2;
        }
        if topic == Set::SIGNATURE_HASH.as_slice() {
            return Self::InterpreterStoreSet;
        }

        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
pub struct DecodedEventData<T> {
    pub event_type: EventType,
    pub block_number: U256,
    pub block_timestamp: U256,
    pub transaction_hash: B256,
    pub log_index: U256,
    pub decoded_data: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DecodedEvent {
    AddOrderV3(Box<AddOrderV3>),
    TakeOrderV4(Box<TakeOrderV4>),
    WithdrawV2(Box<WithdrawV2>),
    DepositV2(Box<DepositV2>),
    RemoveOrderV3(Box<RemoveOrderV3>),
    ClearV3(Box<ClearV3>),
    AfterClearV2(Box<AfterClearV2>),
    MetaV1_2(Box<MetaV1_2>),
    InterpreterStoreSet(Box<InterpreterStoreSetEvent>),
    Unknown(UnknownEventDecoded),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterpreterStoreSetEvent {
    pub store_address: Address,
    pub payload: Set,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownEventDecoded {
    pub raw_data: String,
    pub note: String,
}

pub fn decode_events(
    events: &[LogEntryResponse],
) -> Result<Vec<DecodedEventData<DecodedEvent>>, DecodeError> {
    events
        .iter()
        .enumerate()
        .map(|(index, event)| {
            let topic0 = event
                .topics
                .first()
                .ok_or(DecodeError::MissingRequiredField {
                    field: "topic0",
                    index,
                })?;
            if event.data.is_empty() {
                return Err(DecodeError::MissingRequiredField {
                    field: "data",
                    index,
                });
            }
            let event_type = EventType::from_topic(topic0.as_ref());

            let decoded_data = match event_type {
                EventType::AddOrderV3 => {
                    DecodedEvent::AddOrderV3(Box::new(decode_event::<AddOrderV3>(event)?))
                }
                EventType::TakeOrderV4 => {
                    DecodedEvent::TakeOrderV4(Box::new(decode_event::<TakeOrderV4>(event)?))
                }
                EventType::WithdrawV2 => {
                    DecodedEvent::WithdrawV2(Box::new(decode_event::<WithdrawV2>(event)?))
                }
                EventType::DepositV2 => {
                    DecodedEvent::DepositV2(Box::new(decode_event::<DepositV2>(event)?))
                }
                EventType::RemoveOrderV3 => {
                    DecodedEvent::RemoveOrderV3(Box::new(decode_event::<RemoveOrderV3>(event)?))
                }
                EventType::ClearV3 => {
                    DecodedEvent::ClearV3(Box::new(decode_event::<ClearV3>(event)?))
                }
                EventType::AfterClearV2 => {
                    DecodedEvent::AfterClearV2(Box::new(decode_event::<AfterClearV2>(event)?))
                }
                EventType::MetaV1_2 => {
                    DecodedEvent::MetaV1_2(Box::new(decode_event::<MetaV1_2>(event)?))
                }
                EventType::InterpreterStoreSet => {
                    DecodedEvent::InterpreterStoreSet(Box::new(InterpreterStoreSetEvent {
                        store_address: event.address,
                        payload: decode_event::<Set>(event)?,
                    }))
                }
                EventType::Unknown => DecodedEvent::Unknown(UnknownEventDecoded {
                    raw_data: format!("{:#x}", event.data),
                    note: "Unknown event type - could not decode".to_string(),
                }),
            };

            Ok(DecodedEventData {
                event_type,
                block_number: event.block_number,
                block_timestamp: event.block_timestamp.unwrap_or_default(),
                transaction_hash: event.transaction_hash,
                log_index: event.log_index,
                decoded_data,
            })
        })
        .collect()
}

fn decode_event<E: SolEvent>(event: &LogEntryResponse) -> Result<E, DecodeError> {
    let topics = event
        .topics
        .iter()
        .map(|topic| {
            let b256 = B256::try_from(topic.as_ref())
                .map_err(|_| DecodeError::AbiDecode("topic length != 32 bytes".to_string()))?;
            Ok::<WordToken, DecodeError>(WordToken::from(b256))
        })
        .collect::<Result<Vec<_>, DecodeError>>()?;
    E::decode_raw_log(topics, event.data.as_ref())
        .map_err(|err| DecodeError::AbiDecode(err.to_string()))
}

pub fn sort_decoded_events_by_block_and_log(events: &mut [DecodedEventData<DecodedEvent>]) {
    events.sort_by(|a, b| {
        a.block_number
            .cmp(&b.block_number)
            .then_with(|| a.log_index.cmp(&b.log_index))
    });
}

#[cfg(test)]
mod test_helpers {
    use super::*;
    use crate::rpc_client::LogEntryResponse;
    use alloy::hex;
    use alloy::primitives::{address, b256, Address, Bytes, FixedBytes, B256, U256};
    use raindex_bindings::{
        IRaindexV7::{
            AddOrderV3, AfterClearV2, ClearConfigV2, ClearStateChangeV2, ClearV3, DepositV2,
            RemoveOrderV3, SignedContextV2, TakeOrderConfigV5, TakeOrderV4, WithdrawV2,
        },
        IRaindexV7::{EvaluableV4, OrderV4, IOV2},
        Raindex::MetaV1_2,
    };
    use serde_json::Value;
    use std::str::FromStr;

    fn create_sample_order_v4() -> OrderV4 {
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
                    vaultId: U256::from(100).into(),
                },
                IOV2 {
                    token: Address::from([5u8; 20]),
                    vaultId: U256::from(200).into(),
                },
            ],
            validOutputs: vec![IOV2 {
                token: Address::from([6u8; 20]),
                vaultId: U256::from(300).into(),
            }],
        }
    }

    fn fixed_u256(value: u64) -> FixedBytes<32> {
        FixedBytes::<32>::from(U256::from(value).to_be_bytes::<32>())
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

    fn new_log_entry(
        topic: Bytes,
        data: Bytes,
        block_number: &str,
        block_timestamp: Option<&str>,
        transaction_hash: B256,
        log_index: &str,
    ) -> LogEntryResponse {
        LogEntryResponse {
            address: Address::ZERO,
            topics: vec![topic],
            data,
            block_number: parse_u256(block_number),
            block_timestamp: block_timestamp.map(parse_u256),
            transaction_hash,
            transaction_index: "0x0".to_string(),
            block_hash: B256::ZERO,
            log_index: parse_u256(log_index),
            removed: false,
        }
    }

    fn decode_events_vec(events: Vec<LogEntryResponse>) -> Vec<DecodedEventData<DecodedEvent>> {
        decode_events(&events).unwrap()
    }

    fn build_clear_log(
        sender: Address,
        alice: OrderV4,
        bob: OrderV4,
        clear_config: ClearConfigV2,
    ) -> LogEntryResponse {
        let event_data = ClearV3 {
            sender,
            alice,
            bob,
            clearConfig: clear_config,
        };

        let encoded_data = event_data.encode_data();
        new_log_entry(
            Bytes::from(ClearV3::SIGNATURE_HASH.as_slice().to_vec()),
            Bytes::from(encoded_data),
            "0x123456",
            Some("0x64b8c123"),
            b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"),
            "0x0",
        )
    }

    fn create_add_order_v3_event_data() -> LogEntryResponse {
        let sender = Address::from([7u8; 20]);
        let order_hash = FixedBytes::<32>::from([8u8; 32]);
        let order = create_sample_order_v4();

        let event_data = AddOrderV3 {
            sender,
            orderHash: order_hash,
            order,
        };

        let encoded_data = event_data.encode_data();
        new_log_entry(
            Bytes::from(AddOrderV3::SIGNATURE_HASH.as_slice().to_vec()),
            Bytes::from(encoded_data),
            "0x123456",
            Some("0x64b8c123"),
            b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"),
            "0x0",
        )
    }

    fn create_take_order_v3_event_data() -> LogEntryResponse {
        let sender = Address::from([9u8; 20]);
        let config = TakeOrderConfigV5 {
            order: create_sample_order_v4(),
            inputIOIndex: U256::from(0),
            outputIOIndex: U256::from(0),
            signedContext: vec![SignedContextV2 {
                signer: Address::from([10u8; 20]),
                domainSeparator: alloy::primitives::B256::repeat_byte(0xd5),
                context: vec![U256::from(42).into(), U256::from(43).into()],
                signature: Bytes::from(vec![0x11, 0x22, 0x33]),
            }],
        };
        let input = U256::from(1000);
        let output = U256::from(2000);

        let event_data = TakeOrderV4 {
            sender,
            config,
            input: input.into(),
            output: output.into(),
        };

        let encoded_data = event_data.encode_data();
        new_log_entry(
            Bytes::from(TakeOrderV4::SIGNATURE_HASH.as_slice().to_vec()),
            Bytes::from(encoded_data),
            "0x123457",
            Some("0x64b8c124"),
            b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567891"),
            "0x1",
        )
    }

    fn create_withdraw_v2_event_data() -> LogEntryResponse {
        let sender = Address::from([11u8; 20]);
        let token = Address::from([12u8; 20]);
        let vault_id = U256::from(500);
        let target_amount = U256::from(3000);
        let withdraw_amount = U256::from(2500);
        let withdraw_amount_uint256 = U256::from(2500);

        let event_data = WithdrawV2 {
            sender,
            token,
            vaultId: vault_id.into(),
            targetAmount: target_amount.into(),
            withdrawAmount: withdraw_amount.into(),
            withdrawAmountUint256: withdraw_amount_uint256,
        };

        let encoded_data = event_data.encode_data();
        new_log_entry(
            Bytes::from(WithdrawV2::SIGNATURE_HASH.as_slice().to_vec()),
            Bytes::from(encoded_data),
            "0x123458",
            Some("0x64b8c125"),
            b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567892"),
            "0x2",
        )
    }

    fn create_deposit_v2_event_data() -> LogEntryResponse {
        let sender = Address::from([13u8; 20]);
        let token = Address::from([14u8; 20]);
        let vault_id = U256::from(600);
        let deposit_amount_uint256 = U256::from(4000);

        let event_data = DepositV2 {
            sender,
            token,
            vaultId: vault_id.into(),
            depositAmountUint256: deposit_amount_uint256,
        };

        let encoded_data = event_data.encode_data();
        new_log_entry(
            Bytes::from(DepositV2::SIGNATURE_HASH.as_slice().to_vec()),
            Bytes::from(encoded_data),
            "0x123459",
            Some("0x64b8c126"),
            b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567893"),
            "0x3",
        )
    }

    fn create_remove_order_v3_event_data() -> LogEntryResponse {
        let sender = Address::from([15u8; 20]);
        let order_hash = FixedBytes::<32>::from([16u8; 32]);
        let order = create_sample_order_v4();

        let event_data = RemoveOrderV3 {
            sender,
            orderHash: order_hash,
            order,
        };

        let encoded_data = event_data.encode_data();
        new_log_entry(
            Bytes::from(RemoveOrderV3::SIGNATURE_HASH.as_slice().to_vec()),
            Bytes::from(encoded_data),
            "0x12345a",
            Some("0x64b8c127"),
            b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567894"),
            "0x4",
        )
    }

    fn create_store_set_log() -> LogEntryResponse {
        let event_data = Set {
            namespace: U256::from_be_bytes([0x33u8; 32]),
            key: FixedBytes::<32>::from([0x44u8; 32]),
            value: FixedBytes::<32>::from([0x55u8; 32]),
        };

        let encoded = format!("0x{}", hex::encode(event_data.encode_data()));
        let mut entry = new_log_entry(
            Bytes::from(Set::SIGNATURE_HASH.as_slice().to_vec()),
            Bytes::from_str(&encoded).unwrap(),
            "0x12345c",
            Some("0x64b8c129"),
            b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567896"),
            "0x6",
        );
        entry.address = address!("0x0123456789abcdef0123456789abcdef01234567");
        entry
    }

    fn create_clear_v3_event_data() -> LogEntryResponse {
        let sender = Address::from([17u8; 20]);
        let alice_order = create_sample_order_v4();
        let bob_order = OrderV4 {
            owner: Address::from([18u8; 20]),
            nonce: U256::from(2).into(),
            evaluable: EvaluableV4 {
                interpreter: Address::from([19u8; 20]),
                store: Address::from([20u8; 20]),
                bytecode: Bytes::from(vec![0x05, 0x06, 0x07, 0x08]),
            },
            validInputs: vec![IOV2 {
                token: Address::from([21u8; 20]),
                vaultId: U256::from(700).into(),
            }],
            validOutputs: vec![IOV2 {
                token: Address::from([22u8; 20]),
                vaultId: U256::from(800).into(),
            }],
        };
        let clear_config = ClearConfigV2 {
            aliceInputIOIndex: U256::from(0),
            aliceOutputIOIndex: U256::from(0),
            bobInputIOIndex: U256::from(0),
            bobOutputIOIndex: U256::from(0),
            aliceBountyVaultId: U256::from(0).into(),
            bobBountyVaultId: U256::from(0).into(),
        };

        let event_data = ClearV3 {
            sender,
            alice: alice_order,
            bob: bob_order,
            clearConfig: clear_config,
        };

        let encoded_data = event_data.encode_data();
        new_log_entry(
            Bytes::from(ClearV3::SIGNATURE_HASH.as_slice().to_vec()),
            Bytes::from(encoded_data),
            "0x12345b",
            Some("0x64b8c128"),
            b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567895"),
            "0x5",
        )
    }

    fn create_after_clear_v2_event_data() -> LogEntryResponse {
        let sender = Address::from([23u8; 20]);
        let clear_state_change = ClearStateChangeV2 {
            aliceInput: U256::from(5000).into(),
            aliceOutput: U256::from(6000).into(),
            bobInput: U256::from(7000).into(),
            bobOutput: U256::from(8000).into(),
        };

        let event_data = AfterClearV2 {
            sender,
            clearStateChange: clear_state_change,
        };

        let encoded_data = event_data.encode_data();
        new_log_entry(
            Bytes::from(AfterClearV2::SIGNATURE_HASH.as_slice().to_vec()),
            Bytes::from(encoded_data),
            "0x12345c",
            Some("0x64b8c129"),
            b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567896"),
            "0x6",
        )
    }

    fn create_meta_v1_2_event_data() -> LogEntryResponse {
        let sender = Address::from([24u8; 20]);
        let subject = Address::from([25u8; 20]);
        let meta = Bytes::from(vec![0x09, 0x0a, 0x0b, 0x0c, 0x0d]);

        let event_data = MetaV1_2 {
            sender,
            subject: {
                let mut bytes = [0u8; 32];
                bytes[12..32].copy_from_slice(&subject[..]);
                FixedBytes::<32>::from(bytes)
            },
            meta,
        };

        let encoded_data = event_data.encode_data();
        new_log_entry(
            Bytes::from(MetaV1_2::SIGNATURE_HASH.as_slice().to_vec()),
            Bytes::from(encoded_data),
            "0x12345d",
            Some("0x64b8c12a"),
            b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567897"),
            "0x7",
        )
    }

    #[test]
    fn test_add_order_v3_decode() {
        let event_data = create_add_order_v3_event_data();
        let decoded_events = decode_events_vec(vec![event_data]);
        assert_eq!(decoded_events.len(), 1);

        let decoded_event = &decoded_events[0];
        assert_eq!(decoded_event.event_type, EventType::AddOrderV3);

        let DecodedEvent::AddOrderV3(add_order) = &decoded_event.decoded_data else {
            panic!("expected AddOrderV3 decoded data");
        };

        let add_order = add_order.as_ref();
        assert_eq!(add_order.sender, Address::from([7u8; 20]));
        assert_eq!(add_order.orderHash, FixedBytes::<32>::from([8u8; 32]));

        let expected_order = create_sample_order_v4();
        assert_eq!(add_order.order, expected_order);
    }

    #[test]
    fn test_take_order_v3_decode() {
        let event_data = create_take_order_v3_event_data();
        let decoded_events = decode_events_vec(vec![event_data]);
        assert_eq!(decoded_events.len(), 1);

        let decoded_event = &decoded_events[0];
        assert_eq!(decoded_event.event_type, EventType::TakeOrderV4);

        let DecodedEvent::TakeOrderV4(take_order) = &decoded_event.decoded_data else {
            panic!("expected TakeOrderV4 decoded data");
        };

        let take_order = take_order.as_ref();
        assert_eq!(take_order.sender, Address::from([9u8; 20]));
        assert_eq!(take_order.input, fixed_u256(1000));
        assert_eq!(take_order.output, fixed_u256(2000));

        let expected_order = create_sample_order_v4();
        assert_eq!(take_order.config.order, expected_order);
        assert_eq!(take_order.config.inputIOIndex, U256::from(0));
        assert_eq!(take_order.config.outputIOIndex, U256::from(0));

        let signed_context = &take_order.config.signedContext;
        assert_eq!(signed_context.len(), 1);
        let ctx = &signed_context[0];
        assert_eq!(ctx.signer, Address::from([10u8; 20]));
        assert_eq!(
            ctx.domainSeparator,
            alloy::primitives::B256::repeat_byte(0xd5)
        );
        assert_eq!(ctx.context, vec![fixed_u256(42), fixed_u256(43)]);
        assert_eq!(ctx.signature, Bytes::from(vec![0x11, 0x22, 0x33]));
    }

    #[test]
    fn test_withdraw_v2_decode() {
        let event_data = create_withdraw_v2_event_data();
        let decoded_events = decode_events_vec(vec![event_data]);
        assert_eq!(decoded_events.len(), 1);

        let decoded_event = &decoded_events[0];
        assert_eq!(decoded_event.event_type, EventType::WithdrawV2);

        let DecodedEvent::WithdrawV2(withdraw) = &decoded_event.decoded_data else {
            panic!("expected WithdrawV2 decoded data");
        };

        let withdraw = withdraw.as_ref();
        assert_eq!(withdraw.sender, Address::from([11u8; 20]));
        assert_eq!(withdraw.token, Address::from([12u8; 20]));
        assert_eq!(withdraw.vaultId, fixed_u256(500));
        assert_eq!(withdraw.targetAmount, fixed_u256(3000));
        assert_eq!(withdraw.withdrawAmount, fixed_u256(2500));
        assert_eq!(withdraw.withdrawAmountUint256, U256::from(2500));
    }

    #[test]
    fn test_deposit_v2_decode() {
        let event_data = create_deposit_v2_event_data();
        let decoded_events = decode_events_vec(vec![event_data]);
        assert_eq!(decoded_events.len(), 1);

        let decoded_event = &decoded_events[0];
        assert_eq!(decoded_event.event_type, EventType::DepositV2);

        let DecodedEvent::DepositV2(deposit) = &decoded_event.decoded_data else {
            panic!("expected DepositV2 decoded data");
        };

        let deposit = deposit.as_ref();
        assert_eq!(deposit.sender, Address::from([13u8; 20]));
        assert_eq!(deposit.token, Address::from([14u8; 20]));
        assert_eq!(deposit.vaultId, fixed_u256(600));
        assert_eq!(deposit.depositAmountUint256, U256::from(4000));
    }

    #[test]
    fn test_remove_order_v3_decode() {
        let event_data = create_remove_order_v3_event_data();
        let decoded_events = decode_events_vec(vec![event_data]);
        assert_eq!(decoded_events.len(), 1);

        let decoded_event = &decoded_events[0];
        assert_eq!(decoded_event.event_type, EventType::RemoveOrderV3);

        let DecodedEvent::RemoveOrderV3(remove_order) = &decoded_event.decoded_data else {
            panic!("expected RemoveOrderV3 decoded data");
        };

        let remove_order = remove_order.as_ref();
        assert_eq!(remove_order.sender, Address::from([15u8; 20]));
        assert_eq!(remove_order.orderHash, FixedBytes::<32>::from([16u8; 32]));

        let expected_order = create_sample_order_v4();
        assert_eq!(remove_order.order, expected_order);
    }

    #[test]
    fn test_set_decode() {
        let log_entry = create_store_set_log();
        let decoded = decode_events(&[log_entry]).unwrap();

        assert_eq!(decoded.len(), 1);
        let event = &decoded[0];
        assert_eq!(event.event_type, EventType::InterpreterStoreSet);

        match &event.decoded_data {
            DecodedEvent::InterpreterStoreSet(data) => {
                assert_eq!(
                    data.store_address,
                    Address::from_str("0x0123456789abcdef0123456789abcdef01234567").unwrap()
                );
                assert_eq!(data.payload.namespace, U256::from_be_bytes([0x33u8; 32]));
                assert_eq!(data.payload.key, FixedBytes::<32>::from([0x44u8; 32]));
                assert_eq!(data.payload.value, FixedBytes::<32>::from([0x55u8; 32]));
            }
            other => panic!("expected InterpreterStoreSet, got {other:?}"),
        }
    }

    #[test]
    fn test_interpreter_store_set_serializes_namespace_as_quantity() {
        let event = InterpreterStoreSetEvent {
            store_address: Address::from([0x12u8; 20]),
            payload: Set {
                namespace: U256::from(0x1234),
                key: FixedBytes::<32>::from([0x01u8; 32]),
                value: FixedBytes::<32>::from([0x02u8; 32]),
            },
        };

        let json = serde_json::to_value(&event).unwrap();
        let payload = json
            .get("payload")
            .and_then(Value::as_object)
            .expect("payload should serialize to an object");
        let namespace = payload
            .get("namespace")
            .and_then(Value::as_str)
            .expect("namespace should serialize to a string");
        assert_eq!(namespace, "0x1234");
    }

    #[test]
    fn test_clear_v3_decode() {
        let event_data = create_clear_v3_event_data();
        let decoded_events = decode_events_vec(vec![event_data]);
        assert_eq!(decoded_events.len(), 1);

        let decoded_event = &decoded_events[0];
        assert_eq!(decoded_event.event_type, EventType::ClearV3);

        let DecodedEvent::ClearV3(clear) = &decoded_event.decoded_data else {
            panic!("expected ClearV3 decoded data");
        };

        let clear = clear.as_ref();
        assert_eq!(clear.sender, Address::from([17u8; 20]));

        let expected_alice = create_sample_order_v4();
        assert_eq!(clear.alice, expected_alice);

        let expected_bob = OrderV4 {
            owner: Address::from([18u8; 20]),
            nonce: U256::from(2).into(),
            evaluable: EvaluableV4 {
                interpreter: Address::from([19u8; 20]),
                store: Address::from([20u8; 20]),
                bytecode: Bytes::from(vec![0x05, 0x06, 0x07, 0x08]),
            },
            validInputs: vec![IOV2 {
                token: Address::from([21u8; 20]),
                vaultId: U256::from(700).into(),
            }],
            validOutputs: vec![IOV2 {
                token: Address::from([22u8; 20]),
                vaultId: U256::from(800).into(),
            }],
        };
        assert_eq!(clear.bob, expected_bob);

        let expected_config = ClearConfigV2 {
            aliceInputIOIndex: U256::from(0),
            aliceOutputIOIndex: U256::from(0),
            bobInputIOIndex: U256::from(0),
            bobOutputIOIndex: U256::from(0),
            aliceBountyVaultId: U256::from(0).into(),
            bobBountyVaultId: U256::from(0).into(),
        };
        assert_eq!(clear.clearConfig, expected_config);
    }

    #[test]
    fn test_after_clear_v2_decode() {
        let event_data = create_after_clear_v2_event_data();
        let decoded_events = decode_events_vec(vec![event_data]);
        assert_eq!(decoded_events.len(), 1);

        let decoded_event = &decoded_events[0];
        assert_eq!(decoded_event.event_type, EventType::AfterClearV2);

        let DecodedEvent::AfterClearV2(after_clear) = &decoded_event.decoded_data else {
            panic!("expected AfterClearV2 decoded data");
        };

        let after_clear = after_clear.as_ref();
        assert_eq!(after_clear.sender, Address::from([23u8; 20]));
        assert_eq!(after_clear.clearStateChange.aliceInput, fixed_u256(5000));
        assert_eq!(after_clear.clearStateChange.aliceOutput, fixed_u256(6000));
        assert_eq!(after_clear.clearStateChange.bobInput, fixed_u256(7000));
        assert_eq!(after_clear.clearStateChange.bobOutput, fixed_u256(8000));
    }

    #[test]
    fn test_meta_v1_2_decode() {
        let event_data = create_meta_v1_2_event_data();
        let decoded_events = decode_events_vec(vec![event_data]);
        assert_eq!(decoded_events.len(), 1);

        let decoded_event = &decoded_events[0];
        assert_eq!(decoded_event.event_type, EventType::MetaV1_2);

        let DecodedEvent::MetaV1_2(meta_event) = &decoded_event.decoded_data else {
            panic!("expected MetaV1_2 decoded data");
        };

        let meta_event = meta_event.as_ref();
        assert_eq!(meta_event.sender, Address::from([24u8; 20]));
        let mut expected_subject = [0u8; 32];
        expected_subject[12..32].copy_from_slice(&Address::from([25u8; 20])[..]);
        assert_eq!(meta_event.subject, FixedBytes::<32>::from(expected_subject));
        assert_eq!(
            meta_event.meta,
            Bytes::from(vec![0x09, 0x0a, 0x0b, 0x0c, 0x0d])
        );
    }

    #[test]
    fn test_invalid_hex_data_decode() {
        let mut invalid_event = create_add_order_v3_event_data();
        invalid_event.data = Bytes::from(vec![0xde, 0xad]);

        let result = decode_events(&[invalid_event]);

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, DecodeError::AbiDecode(_)));
    }

    #[test]
    fn test_malformed_abi_data_decode() {
        let mut malformed_event = create_add_order_v3_event_data();
        malformed_event.data = Bytes::from(vec![0x12, 0x34]);

        let result = decode_events(&[malformed_event]);

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, DecodeError::AbiDecode(_)));
    }

    #[test]
    fn test_unknown_event_type_decode() {
        let unknown_topic = Bytes::from(vec![0xff; 32]);
        let mut unknown_event = create_add_order_v3_event_data();
        unknown_event.topics = vec![unknown_topic];
        unknown_event.data = Bytes::from(vec![0x12, 0x34, 0x56, 0x78]);

        let decoded_events = decode_events_vec(vec![unknown_event]);
        assert_eq!(decoded_events.len(), 1);

        let decoded_event = &decoded_events[0];
        assert_eq!(decoded_event.event_type, EventType::Unknown);

        let DecodedEvent::Unknown(unknown) = &decoded_event.decoded_data else {
            panic!("expected Unknown decoded data");
        };

        assert_eq!(unknown.raw_data, "0x12345678");
        assert_eq!(unknown.note, "Unknown event type - could not decode");
    }

    #[test]
    fn test_empty_events_array() {
        let decoded_events = decode_events_vec(Vec::new());
        assert!(decoded_events.is_empty());
    }

    #[test]
    fn test_event_empty_topics_array() {
        let mut event_empty_topics = create_add_order_v3_event_data();
        event_empty_topics.topics.clear();
        event_empty_topics.data = Bytes::from(vec![0x12, 0x34, 0x56, 0x78]);

        let result = decode_events(&[event_empty_topics]);
        assert!(matches!(
            result,
            Err(DecodeError::MissingRequiredField { field, index })
                if field == "topic0" && index == 0
        ));
    }

    #[test]
    fn test_event_missing_data_field() {
        let mut event_no_data = create_add_order_v3_event_data();
        event_no_data.data = Bytes::new();

        let result = decode_events(&[event_no_data]);
        assert!(matches!(
            result,
            Err(DecodeError::MissingRequiredField { field, index })
                if field == "data" && index == 0
        ));
    }

    #[test]
    fn test_event_zero_length_hex_data() {
        let mut event_no_data = create_add_order_v3_event_data();
        event_no_data.data = Bytes::default();

        let result = decode_events(&[event_no_data]);
        assert!(matches!(
            result,
            Err(DecodeError::MissingRequiredField { field, index })
                if field == "data" && index == 0
        ));
    }

    #[test]
    fn test_event_with_valid_data_missing_metadata() {
        let mut minimal_event = create_add_order_v3_event_data();
        minimal_event.block_number = U256::ZERO;
        minimal_event.block_timestamp = None;
        minimal_event.transaction_hash = B256::ZERO;
        minimal_event.log_index = U256::ZERO;

        let decoded_events = decode_events_vec(vec![minimal_event]);
        assert_eq!(decoded_events.len(), 1);

        let decoded_event = &decoded_events[0];
        assert_eq!(decoded_event.event_type, EventType::AddOrderV3);
        assert_eq!(decoded_event.block_number, U256::ZERO);
        assert_eq!(decoded_event.block_timestamp, U256::ZERO);
        assert_eq!(decoded_event.transaction_hash, B256::ZERO);
        assert_eq!(decoded_event.log_index, U256::ZERO);
    }

    #[test]
    fn test_clear_v3_alice_input_io_index_out_of_bounds() {
        let sender = Address::from([17u8; 20]);
        let alice_order = create_sample_order_v4();
        let bob_order = create_sample_order_v4();
        let clear_config = ClearConfigV2 {
            aliceInputIOIndex: U256::from(5),
            aliceOutputIOIndex: U256::from(0),
            bobInputIOIndex: U256::from(0),
            bobOutputIOIndex: U256::from(0),
            aliceBountyVaultId: U256::from(0).into(),
            bobBountyVaultId: U256::from(0).into(),
        };

        let clear_event = build_clear_log(sender, alice_order, bob_order, clear_config);
        let result = decode_events(&[clear_event]).unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].decoded_data, DecodedEvent::ClearV3(_)));
    }

    #[test]
    fn test_clear_v3_alice_output_io_index_out_of_bounds() {
        let sender = Address::from([17u8; 20]);
        let alice_order = create_sample_order_v4();
        let bob_order = create_sample_order_v4();
        let clear_config = ClearConfigV2 {
            aliceInputIOIndex: U256::from(0),
            aliceOutputIOIndex: U256::from(3),
            bobInputIOIndex: U256::from(0),
            bobOutputIOIndex: U256::from(0),
            aliceBountyVaultId: U256::from(0).into(),
            bobBountyVaultId: U256::from(0).into(),
        };

        let clear_event = build_clear_log(sender, alice_order, bob_order, clear_config);
        let result = decode_events(&[clear_event]).unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].decoded_data, DecodedEvent::ClearV3(_)));
    }

    #[test]
    fn test_clear_v3_bob_input_io_index_out_of_bounds() {
        let sender = Address::from([17u8; 20]);
        let alice_order = create_sample_order_v4();
        let bob_order = create_sample_order_v4();
        let clear_config = ClearConfigV2 {
            aliceInputIOIndex: U256::from(0),
            aliceOutputIOIndex: U256::from(0),
            bobInputIOIndex: U256::from(10),
            bobOutputIOIndex: U256::from(0),
            aliceBountyVaultId: U256::from(0).into(),
            bobBountyVaultId: U256::from(0).into(),
        };

        let clear_event = build_clear_log(sender, alice_order, bob_order, clear_config);
        let result = decode_events(&[clear_event]).unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].decoded_data, DecodedEvent::ClearV3(_)));
    }

    #[test]
    fn test_clear_v3_bob_output_io_index_out_of_bounds() {
        let sender = Address::from([17u8; 20]);
        let alice_order = create_sample_order_v4();
        let bob_order = create_sample_order_v4();
        let clear_config = ClearConfigV2 {
            aliceInputIOIndex: U256::from(0),
            aliceOutputIOIndex: U256::from(0),
            bobInputIOIndex: U256::from(0),
            bobOutputIOIndex: U256::from(7),
            aliceBountyVaultId: U256::from(0).into(),
            bobBountyVaultId: U256::from(0).into(),
        };
        let clear_event = build_clear_log(sender, alice_order, bob_order, clear_config);
        let result = decode_events(&[clear_event]).unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].decoded_data, DecodedEvent::ClearV3(_)));
    }

    #[test]
    fn test_clear_v3_empty_valid_inputs_array() {
        let sender = Address::from([17u8; 20]);
        let mut alice_order = create_sample_order_v4();
        alice_order.validInputs = vec![];
        let bob_order = create_sample_order_v4();
        let clear_config = ClearConfigV2 {
            aliceInputIOIndex: U256::from(0),
            aliceOutputIOIndex: U256::from(0),
            bobInputIOIndex: U256::from(0),
            bobOutputIOIndex: U256::from(0),
            aliceBountyVaultId: U256::from(0).into(),
            bobBountyVaultId: U256::from(0).into(),
        };
        let clear_event = build_clear_log(sender, alice_order, bob_order, clear_config);
        let result = decode_events(&[clear_event]).unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].decoded_data, DecodedEvent::ClearV3(_)));
    }

    fn mk_event(block: u64, log_index: u64, tx: B256) -> DecodedEventData<DecodedEvent> {
        DecodedEventData {
            event_type: EventType::Unknown,
            block_number: U256::from(block),
            block_timestamp: U256::ZERO,
            transaction_hash: tx,
            log_index: U256::from(log_index),
            decoded_data: DecodedEvent::Unknown(UnknownEventDecoded {
                raw_data: "0x".to_string(),
                note: "".to_string(),
            }),
        }
    }

    #[test]
    fn sort_events_by_block_and_log_orders_stably() {
        let mut events = vec![
            mk_event(
                2,
                1,
                b256!("0x0000000000000000000000000000000000000000000000000000000000000010"),
            ),
            mk_event(
                1,
                2,
                b256!("0x0000000000000000000000000000000000000000000000000000000000000020"),
            ),
            mk_event(
                1,
                1,
                b256!("0x0000000000000000000000000000000000000000000000000000000000000030"),
            ),
        ];
        sort_decoded_events_by_block_and_log(&mut events);
        assert_eq!(
            events[0].transaction_hash,
            b256!("0x0000000000000000000000000000000000000000000000000000000000000030")
        );
        assert_eq!(
            events[1].transaction_hash,
            b256!("0x0000000000000000000000000000000000000000000000000000000000000020")
        );
        assert_eq!(
            events[2].transaction_hash,
            b256!("0x0000000000000000000000000000000000000000000000000000000000000010")
        );
    }

    #[test]
    fn sort_events_preserves_relative_order_for_identical_keys() {
        let mut events = vec![
            mk_event(
                1,
                1,
                b256!("0x0000000000000000000000000000000000000000000000000000000000000001"),
            ),
            mk_event(
                1,
                1,
                b256!("0x0000000000000000000000000000000000000000000000000000000000000002"),
            ),
            mk_event(
                1,
                1,
                b256!("0x0000000000000000000000000000000000000000000000000000000000000003"),
            ),
        ];
        let expected_order: Vec<_> = events.iter().map(|event| event.transaction_hash).collect();
        sort_decoded_events_by_block_and_log(&mut events);
        let actual_order: Vec<_> = events.iter().map(|event| event.transaction_hash).collect();
        assert_eq!(actual_order, expected_order);
    }

    #[test]
    fn sort_events_orders_large_values() {
        let mut events = vec![
            mk_event(
                u64::MAX,
                0,
                b256!("0x0000000000000000000000000000000000000000000000000000000000000001"),
            ),
            mk_event(
                u64::MAX - 1,
                1,
                b256!("0x0000000000000000000000000000000000000000000000000000000000000002"),
            ),
        ];
        sort_decoded_events_by_block_and_log(&mut events);
        assert_eq!(
            events[0].transaction_hash,
            b256!("0x0000000000000000000000000000000000000000000000000000000000000002")
        );
        assert_eq!(
            events[1].transaction_hash,
            b256!("0x0000000000000000000000000000000000000000000000000000000000000001")
        );
    }
}
