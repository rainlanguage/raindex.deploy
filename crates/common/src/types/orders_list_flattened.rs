use crate::types::vault::NO_SYMBOL;
use crate::{csv::TryIntoCsv, utils::timestamp::format_bigint_timestamp_display};
use alloy::dyn_abi::SolType;
use alloy::primitives::hex::{decode, encode};
use raindex_bindings::IRaindexV7::OrderV4;
use raindex_subgraph_client::types::common::*;
use serde::{Deserialize, Serialize};

use super::FlattenError;

pub const LIST_DELIMITER: &str = ", ";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrderFlattened {
    pub id: String,
    pub timestamp: SgBigInt,
    pub timestamp_display: String,
    pub owner: SgBytes,
    pub order_active: bool,
    pub interpreter: SgBytes,
    pub interpreter_store: SgBytes,
    pub transaction: String,
    pub valid_inputs_vaults: String,
    pub valid_outputs_vaults: String,
    pub valid_inputs_token_symbols_display: String,
    pub valid_outputs_token_symbols_display: String,
    pub trades: String,
}

impl TryFrom<SgOrder> for OrderFlattened {
    type Error = FlattenError;

    fn try_from(val: SgOrder) -> Result<Self, Self::Error> {
        let order = OrderV4::abi_decode(&decode(&val.order_bytes.0)?)?;
        Ok(Self {
            id: val.id.0,
            timestamp: val.timestamp_added.clone(),
            timestamp_display: format_bigint_timestamp_display(val.timestamp_added.0)?,
            owner: val.owner,
            order_active: val.active,
            interpreter: SgBytes(encode(order.evaluable.interpreter.0)),
            interpreter_store: SgBytes(encode(order.evaluable.store.0)),
            transaction: val
                .add_events
                .first()
                .map(|event| event.transaction.id.0.clone())
                .ok_or(FlattenError::MissingAddEvent)?,
            valid_inputs_vaults: val
                .inputs
                .clone()
                .into_iter()
                .map(|v| v.vault_id.0)
                .collect::<Vec<String>>()
                .join(LIST_DELIMITER),
            valid_outputs_vaults: val
                .outputs
                .clone()
                .into_iter()
                .map(|v| v.vault_id.0)
                .collect::<Vec<String>>()
                .join(LIST_DELIMITER),
            valid_inputs_token_symbols_display: val
                .inputs
                .into_iter()
                .map(|vault| vault.token.symbol.unwrap_or(NO_SYMBOL.into()))
                .collect::<Vec<String>>()
                .join(LIST_DELIMITER),
            valid_outputs_token_symbols_display: val
                .outputs
                .into_iter()
                .map(|vault| vault.token.symbol.unwrap_or(NO_SYMBOL.into()))
                .collect::<Vec<String>>()
                .join(LIST_DELIMITER),
            trades: val
                .trades
                .into_iter()
                .map(|trade| trade.id.0)
                .collect::<Vec<String>>()
                .join(LIST_DELIMITER),
        })
    }
}

impl TryIntoCsv<OrderFlattened> for Vec<OrderFlattened> {}

#[cfg(test)]
mod tests {
    use alloy::{
        hex::FromHexError,
        primitives::{Address, Bytes, FixedBytes, B256, U256},
        sol_types::SolValue,
    };
    use raindex_bindings::IRaindexV7::{EvaluableV4, OrderV4, IOV2};
    use raindex_subgraph_client::types::common::{
        SgAddOrder, SgBigInt, SgBytes, SgErc20, SgOrderStructPartialTrade, SgRaindex,
        SgTransaction, SgVault,
    };
    use std::str::FromStr;

    use super::*;
    use crate::types::vault::NO_SYMBOL;

    fn mock_sg_order_default() -> SgOrder {
        let evaluable = EvaluableV4 {
            interpreter: Address::repeat_byte(0x01),
            store: Address::repeat_byte(0x02),
            bytecode: Bytes::from_str("0x").unwrap(),
        };
        let valid_inputs = vec![IOV2 {
            token: Address::repeat_byte(0x11),
            vaultId: B256::from(U256::from(111)),
        }];
        let valid_outputs = vec![IOV2 {
            token: Address::repeat_byte(0x22),
            vaultId: B256::from(U256::from(222)),
        }];

        let order_v4 = OrderV4 {
            owner: Address::repeat_byte(0x0a),
            nonce: FixedBytes::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            )
            .unwrap(),
            validInputs: valid_inputs.clone(),
            validOutputs: valid_outputs.clone(),
            evaluable,
        };
        let order_bytes_hex = alloy::primitives::hex::encode(order_v4.abi_encode());

        SgOrder {
            id: SgBytes("order-id-default".into()),
            order_bytes: SgBytes(order_bytes_hex),
            order_hash: SgBytes("0xhashdefault".into()),
            owner: SgBytes(format!("{:?}", Address::repeat_byte(0x0a))),
            outputs: vec![SgVault {
                id: SgBytes("vault-out-id".into()),
                owner: SgBytes("vault-owner".into()),
                vault_id: SgBytes("222".into()),
                balance: SgBytes("1000".into()),
                token: SgErc20 {
                    id: SgBytes("token-out-id".into()),
                    address: SgBytes(format!("{:?}", Address::repeat_byte(0x22))),
                    name: Some("TokenOut".into()),
                    symbol: Some("TOUT".into()),
                    decimals: Some(SgBigInt("18".into())),
                },
                raindex: SgRaindex {
                    id: SgBytes("raindex-id".into()),
                },
                orders_as_output: vec![],
                orders_as_input: vec![],
                balance_changes: vec![],
            }],
            inputs: vec![SgVault {
                id: SgBytes("vault-in-id".into()),
                owner: SgBytes("vault-owner".into()),
                vault_id: SgBytes("111".into()),
                balance: SgBytes("1000".into()),
                token: SgErc20 {
                    id: SgBytes("token-in-id".into()),
                    address: SgBytes(format!("{:?}", Address::repeat_byte(0x11))),
                    name: Some("TokenIn".into()),
                    symbol: Some("TIN".into()),
                    decimals: Some(SgBigInt("18".into())),
                },
                raindex: SgRaindex {
                    id: SgBytes("raindex-id".into()),
                },
                orders_as_output: vec![],
                orders_as_input: vec![],
                balance_changes: vec![],
            }],
            raindex: SgRaindex {
                id: SgBytes("raindex-id".into()),
            },
            active: true,
            timestamp_added: SgBigInt("1678886400".into()),
            meta: None,
            add_events: vec![SgAddOrder {
                transaction: SgTransaction {
                    id: SgBytes("tx-id-default".into()),
                    from: SgBytes("tx-from-default".into()),
                    block_number: SgBigInt("100".into()),
                    timestamp: SgBigInt("1678886300".into()),
                },
            }],
            trades: vec![SgOrderStructPartialTrade {
                id: SgBytes("trade-id-default".into()),
            }],
            remove_events: vec![],
        }
    }

    #[test]
    fn test_try_from_sg_order_normal_case() {
        let sg_order = mock_sg_order_default();
        let order_flattened = OrderFlattened::try_from(sg_order.clone()).unwrap();

        assert_eq!(order_flattened.id, sg_order.id.0);
        assert_eq!(order_flattened.timestamp, sg_order.timestamp_added);
        assert_eq!(order_flattened.owner, sg_order.owner);
        assert_eq!(order_flattened.order_active, sg_order.active);

        let expected_interpreter =
            SgBytes(alloy::primitives::hex::encode(Address::repeat_byte(0x01).0));
        let expected_store = SgBytes(alloy::primitives::hex::encode(Address::repeat_byte(0x02).0));
        assert_eq!(order_flattened.interpreter, expected_interpreter);
        assert_eq!(order_flattened.interpreter_store, expected_store);

        assert_eq!(
            order_flattened.transaction,
            sg_order.add_events[0].transaction.id.0
        );
        assert_eq!(order_flattened.valid_inputs_vaults, "111");
        assert_eq!(order_flattened.valid_outputs_vaults, "222");
        assert_eq!(order_flattened.valid_inputs_token_symbols_display, "TIN");
        assert_eq!(order_flattened.valid_outputs_token_symbols_display, "TOUT");
        assert_eq!(order_flattened.trades, "trade-id-default");
    }

    #[test]
    fn test_try_from_empty_lists() {
        let mut sg_order = mock_sg_order_default();
        sg_order.inputs = vec![];
        sg_order.outputs = vec![];
        sg_order.trades = vec![];

        let order_flattened = OrderFlattened::try_from(sg_order).unwrap();

        assert_eq!(order_flattened.valid_inputs_vaults, "");
        assert_eq!(order_flattened.valid_outputs_vaults, "");
        assert_eq!(order_flattened.valid_inputs_token_symbols_display, "");
        assert_eq!(order_flattened.valid_outputs_token_symbols_display, "");
        assert_eq!(order_flattened.trades, "");
    }

    #[test]
    fn test_try_from_multiple_items_in_lists() {
        let mut sg_order = mock_sg_order_default();
        sg_order.inputs.push(SgVault {
            id: SgBytes("vault-in-id-2".into()),
            owner: SgBytes("vault-owner-2".into()),
            balance: SgBytes("1000".into()),
            raindex: SgRaindex {
                id: SgBytes("raindex-id".into()),
            },
            orders_as_output: vec![],
            orders_as_input: vec![],
            balance_changes: vec![],
            vault_id: SgBytes("112".into()),
            token: SgErc20 {
                address: SgBytes("0x112".into()),
                symbol: Some("TIN2".into()),
                name: Some("TokenIn2".into()),
                decimals: Some(SgBigInt("18".into())),
                id: SgBytes("token-in-id-2".into()),
            },
        });
        sg_order.outputs.push(SgVault {
            id: SgBytes("vault-out-id-2".into()),
            owner: SgBytes("vault-owner-2".into()),
            balance: SgBytes("1000".into()),
            raindex: SgRaindex {
                id: SgBytes("raindex-id".into()),
            },
            orders_as_output: vec![],
            orders_as_input: vec![],
            balance_changes: vec![],
            vault_id: SgBytes("223".into()),
            token: SgErc20 {
                address: SgBytes("0x223".into()),
                symbol: None,
                name: None,
                decimals: None,
                id: SgBytes("token-out-id-2".into()),
            },
        });
        sg_order.trades.push(SgOrderStructPartialTrade {
            id: SgBytes("trade-id-another".into()),
        });

        let order_flattened = OrderFlattened::try_from(sg_order).unwrap();

        assert_eq!(order_flattened.valid_inputs_vaults, "111, 112");
        assert_eq!(order_flattened.valid_outputs_vaults, "222, 223");
        assert_eq!(
            order_flattened.valid_inputs_token_symbols_display,
            "TIN, TIN2"
        );
        assert_eq!(
            order_flattened.valid_outputs_token_symbols_display,
            format!("TOUT, {}", NO_SYMBOL)
        );
        assert_eq!(order_flattened.trades, "trade-id-default, trade-id-another");
    }

    #[test]
    fn test_try_from_invalid_order_bytes_hex() {
        let mut sg_order = mock_sg_order_default();
        sg_order.order_bytes = SgBytes("invalid-hex".into());
        let result = OrderFlattened::try_from(sg_order);
        assert!(
            matches!(
                result,
                Err(FlattenError::FromHexError(FromHexError::OddLength))
            ),
            "Expected AbiDecodeError for invalid order bytes hex, got {:?}",
            result
        );
    }

    #[test]
    fn test_try_from_invalid_order_bytes_abi() {
        let mut sg_order = mock_sg_order_default();
        sg_order.order_bytes = SgBytes("0x1234".into()); // Valid hex, invalid ABI
        let result = OrderFlattened::try_from(sg_order);
        assert!(
            matches!(result, Err(FlattenError::AbiDecodeError(_))),
            "Expected AbiDecodeError for invalid order bytes abi, got {:?}",
            result
        );
    }

    #[test]
    fn test_try_from_invalid_timestamp_format() {
        let mut sg_order = mock_sg_order_default();
        sg_order.timestamp_added = SgBigInt("not-a-timestamp".into());
        let result = OrderFlattened::try_from(sg_order);
        assert!(
            matches!(result, Err(FlattenError::FormatTimestampDisplayError(_))),
            "Expected FormatTimestampDisplayError for invalid timestamp format, got {:?}",
            result
        );
    }

    #[test]
    fn test_try_from_empty_add_events_returns_error() {
        let mut sg_order = mock_sg_order_default();
        sg_order.add_events = vec![];
        let result = OrderFlattened::try_from(sg_order);
        assert!(
            matches!(result, Err(FlattenError::MissingAddEvent)),
            "Expected MissingAddEvent error, got {:?}",
            result
        );
    }

    #[test]
    fn test_try_from_all_token_symbols_none() {
        let mut sg_order = mock_sg_order_default();
        sg_order.inputs = vec![SgVault {
            id: SgBytes("vault-in-id".into()),
            owner: SgBytes("vault-owner".into()),
            balance: SgBytes("1000".into()),
            raindex: SgRaindex {
                id: SgBytes("raindex-id".into()),
            },
            orders_as_output: vec![],
            orders_as_input: vec![],
            balance_changes: vec![],
            vault_id: SgBytes("111".into()),
            token: SgErc20 {
                symbol: None,
                id: SgBytes("token-in-id".into()),
                address: SgBytes("0x111".into()),
                name: Some("TokenIn".into()),
                decimals: Some(SgBigInt("18".into())),
            },
        }];
        sg_order.outputs = vec![SgVault {
            id: SgBytes("vault-out-id".into()),
            owner: SgBytes("vault-owner".into()),
            balance: SgBytes("1000".into()),
            raindex: SgRaindex {
                id: SgBytes("raindex-id".into()),
            },
            vault_id: SgBytes("222".into()),
            token: SgErc20 {
                symbol: None,
                id: SgBytes("token-out-id".into()),
                address: SgBytes("0x222".into()),
                name: Some("TokenOut".into()),
                decimals: Some(SgBigInt("18".into())),
            },
            orders_as_output: vec![],
            orders_as_input: vec![],
            balance_changes: vec![],
        }];

        let order_flattened = OrderFlattened::try_from(sg_order).unwrap();

        assert_eq!(
            order_flattened.valid_inputs_token_symbols_display,
            NO_SYMBOL
        );
        assert_eq!(
            order_flattened.valid_outputs_token_symbols_display,
            NO_SYMBOL
        );
    }

    #[test]
    fn test_try_from_order_inactive() {
        let mut sg_order = mock_sg_order_default();
        sg_order.active = false;
        let order_flattened = OrderFlattened::try_from(sg_order).unwrap();
        assert!(!order_flattened.order_active);
    }
}
