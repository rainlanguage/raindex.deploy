use super::decode::{DecodedEvent, DecodedEventData};
use alloy::primitives::Address;
use raindex_bindings::IRaindexV7::OrderV4;
use std::collections::BTreeSet;

pub fn collect_token_addresses(
    decoded_events: &[DecodedEventData<DecodedEvent>],
) -> BTreeSet<Address> {
    decoded_events
        .iter()
        .filter_map(|event| match &event.decoded_data {
            DecodedEvent::DepositV2(deposit) => Some(vec![deposit.token]),
            DecodedEvent::WithdrawV2(withdraw) => Some(vec![withdraw.token]),
            DecodedEvent::AddOrderV3(add) => Some(order_tokens_vec(&add.order)),
            DecodedEvent::RemoveOrderV3(remove) => Some(order_tokens_vec(&remove.order)),
            DecodedEvent::TakeOrderV4(take) => Some(order_tokens_vec(&take.config.order)),
            DecodedEvent::ClearV3(clear) => {
                let mut tokens = order_tokens_vec(&clear.alice);
                tokens.extend(order_tokens_vec(&clear.bob));
                Some(tokens)
            }
            _ => None,
        })
        .flatten()
        .collect()
}

pub fn collect_store_addresses(
    decoded_events: &[DecodedEventData<DecodedEvent>],
) -> BTreeSet<Address> {
    let mut out = BTreeSet::new();

    for event in decoded_events {
        match &event.decoded_data {
            DecodedEvent::AddOrderV3(add) => {
                out.insert(add.order.evaluable.store);
            }
            DecodedEvent::RemoveOrderV3(remove) => {
                out.insert(remove.order.evaluable.store);
            }
            DecodedEvent::TakeOrderV4(take) => {
                out.insert(take.config.order.evaluable.store);
            }
            DecodedEvent::InterpreterStoreSet(set) => {
                out.insert(set.store_address);
            }
            _ => {}
        }
    }

    out
}

fn order_tokens_vec(order: &OrderV4) -> Vec<Address> {
    order
        .validInputs
        .iter()
        .map(|input| input.token)
        .chain(order.validOutputs.iter().map(|output| output.token))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::super::decode::InterpreterStoreSetEvent;
    use super::*;
    use alloy::primitives::{b256, Address, Bytes, FixedBytes, U256};
    use raindex_bindings::IInterpreterStoreV3::Set;
    use raindex_bindings::IRaindexV7::{
        AddOrderV3, DepositV2, OrderV4, RemoveOrderV3, SignedContextV2, TakeOrderConfigV5,
        TakeOrderV4, WithdrawV2, IOV2,
    };

    fn build_event(event: DecodedEvent) -> DecodedEventData<DecodedEvent> {
        DecodedEventData {
            event_type: super::super::decode::EventType::Unknown,
            block_number: U256::ZERO,
            block_timestamp: U256::ZERO,
            transaction_hash: b256!(
                "0x0000000000000000000000000000000000000000000000000000000000000000"
            ),
            log_index: U256::ZERO,
            decoded_data: event,
        }
    }

    fn sample_order() -> OrderV4 {
        OrderV4 {
            owner: Address::from([1u8; 20]),
            nonce: U256::from(1).into(),
            evaluable: raindex_bindings::IRaindexV7::EvaluableV4 {
                interpreter: Address::from([2u8; 20]),
                store: Address::from([3u8; 20]),
                bytecode: alloy::primitives::Bytes::from(vec![]),
            },
            validInputs: vec![IOV2 {
                token: Address::from([4u8; 20]),
                vaultId: U256::from(10).into(),
            }],
            validOutputs: vec![IOV2 {
                token: Address::from([5u8; 20]),
                vaultId: U256::from(11).into(),
            }],
        }
    }

    #[test]
    fn collects_simple_transfers() {
        let deposit = DepositV2 {
            sender: Address::from([0u8; 20]),
            token: Address::from([1u8; 20]),
            vaultId: U256::from(1).into(),
            depositAmountUint256: U256::from(1),
        };
        let withdraw = WithdrawV2 {
            sender: Address::from([0u8; 20]),
            token: Address::from([2u8; 20]),
            vaultId: U256::from(1).into(),
            targetAmount: U256::from(0).into(),
            withdrawAmount: U256::from(0).into(),
            withdrawAmountUint256: U256::from(0),
        };

        let events = vec![
            build_event(DecodedEvent::DepositV2(Box::new(deposit))),
            build_event(DecodedEvent::WithdrawV2(Box::new(withdraw))),
        ];

        let tokens = collect_token_addresses(&events);
        assert!(tokens.contains(&Address::from([1u8; 20])));
        assert!(tokens.contains(&Address::from([2u8; 20])));
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn collects_orders() {
        let order = sample_order();
        let add_event = AddOrderV3 {
            sender: Address::from([6u8; 20]),
            orderHash: alloy::primitives::FixedBytes::from([0u8; 32]),
            order: order.clone(),
        };
        let remove_event = RemoveOrderV3 {
            sender: Address::from([7u8; 20]),
            orderHash: alloy::primitives::FixedBytes::from([1u8; 32]),
            order,
        };

        let events = vec![
            build_event(DecodedEvent::AddOrderV3(Box::new(add_event))),
            build_event(DecodedEvent::RemoveOrderV3(Box::new(remove_event))),
        ];

        let tokens = collect_token_addresses(&events);
        assert_eq!(tokens.len(), 2);
        assert!(tokens.contains(&Address::from([4u8; 20])));
        assert!(tokens.contains(&Address::from([5u8; 20])));
    }

    #[test]
    fn collects_from_clear() {
        let mut alice = sample_order();
        alice.validInputs.push(IOV2 {
            token: Address::from([8u8; 20]),
            vaultId: U256::from(0).into(),
        });
        let mut bob = sample_order();
        bob.validOutputs.push(IOV2 {
            token: Address::from([9u8; 20]),
            vaultId: U256::from(0).into(),
        });

        let clear = raindex_bindings::IRaindexV7::ClearV3 {
            sender: Address::from([0u8; 20]),
            alice,
            bob,
            clearConfig: raindex_bindings::IRaindexV7::ClearConfigV2 {
                aliceInputIOIndex: U256::from(0),
                aliceOutputIOIndex: U256::from(0),
                bobInputIOIndex: U256::from(0),
                bobOutputIOIndex: U256::from(0),
                aliceBountyVaultId: U256::from(0).into(),
                bobBountyVaultId: U256::from(0).into(),
            },
        };

        let events = vec![build_event(DecodedEvent::ClearV3(Box::new(clear)))];
        let tokens = collect_token_addresses(&events);
        assert!(tokens.contains(&Address::from([4u8; 20])));
        assert!(tokens.contains(&Address::from([5u8; 20])));
        assert!(tokens.contains(&Address::from([8u8; 20])));
        assert!(tokens.contains(&Address::from([9u8; 20])));
        assert_eq!(tokens.len(), 4);
    }

    #[test]
    fn collects_store_addresses() {
        let mut add_order = sample_order();
        add_order.evaluable.store = Address::from([0x11; 20]);
        let add_event = AddOrderV3 {
            sender: Address::from([0x21; 20]),
            orderHash: FixedBytes::from([0x01; 32]),
            order: add_order,
        };

        let mut remove_order = sample_order();
        remove_order.evaluable.store = Address::from([0x22; 20]);
        let remove_event = RemoveOrderV3 {
            sender: Address::from([0x22; 20]),
            orderHash: FixedBytes::from([0x02; 32]),
            order: remove_order,
        };

        let mut take_order = sample_order();
        take_order.evaluable.store = Address::from([0x33; 20]);
        let take_event = TakeOrderV4 {
            sender: Address::from([0x23; 20]),
            config: TakeOrderConfigV5 {
                order: take_order,
                inputIOIndex: U256::from(0),
                outputIOIndex: U256::from(0),
                signedContext: vec![SignedContextV2 {
                    signer: Address::from([0x24; 20]),
                    domainSeparator: alloy::primitives::B256::repeat_byte(0xd5),
                    context: vec![U256::from(1).into()],
                    signature: Bytes::from(vec![0xde, 0xad]),
                }],
            },
            input: U256::from(1).into(),
            output: U256::from(1).into(),
        };

        let store_event = InterpreterStoreSetEvent {
            store_address: Address::from([0x44; 20]),
            payload: Set {
                namespace: U256::from_be_bytes([0xaa; 32]),
                key: FixedBytes::<32>::from([0xbb; 32]),
                value: FixedBytes::<32>::from([0xcc; 32]),
            },
        };

        let events = vec![
            build_event(DecodedEvent::AddOrderV3(Box::new(add_event))),
            build_event(DecodedEvent::RemoveOrderV3(Box::new(remove_event))),
            build_event(DecodedEvent::TakeOrderV4(Box::new(take_event))),
            build_event(DecodedEvent::InterpreterStoreSet(Box::new(store_event))),
        ];

        let stores = collect_store_addresses(&events);
        assert_eq!(stores.len(), 4);
        assert!(stores
            .contains(&Address::from_str("0x1111111111111111111111111111111111111111").unwrap()));
        assert!(stores
            .contains(&Address::from_str("0x2222222222222222222222222222222222222222").unwrap()));
        assert!(stores
            .contains(&Address::from_str("0x3333333333333333333333333333333333333333").unwrap()));
        assert!(stores
            .contains(&Address::from_str("0x4444444444444444444444444444444444444444").unwrap()));
    }
}
