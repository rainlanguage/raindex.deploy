use super::common::*;
use alloy::primitives::{
    hex::{decode, FromHexError},
    ruint::ParseError,
    Address, B256,
};
use alloy::sol_types::SolValue;
use raindex_bindings::IRaindexV7::{OrderV4, IOV2};
use std::{num::TryFromIntError, str::FromStr};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrderDetailError {
    #[error(transparent)]
    FromHexError(#[from] FromHexError),
    #[error(transparent)]
    TryFromIntError(#[from] TryFromIntError),
    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error(transparent)]
    ParseError(#[from] ParseError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    AbiDecode(#[from] alloy::sol_types::Error),
}

impl TryInto<IOV2> for SgVault {
    type Error = OrderDetailError;

    fn try_into(self) -> Result<IOV2, OrderDetailError> {
        Ok(IOV2 {
            token: self.token.address.0.parse::<Address>()?,
            vaultId: B256::from_str(self.vault_id.0.as_str())?,
        })
    }
}

impl TryFrom<SgOrder> for OrderV4 {
    type Error = OrderDetailError;

    fn try_from(value: SgOrder) -> Result<Self, Self::Error> {
        let order = OrderV4::abi_decode(&decode(value.order_bytes.0)?)?;
        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use alloy::hex;
    use alloy::primitives::{bytes, B256};
    use raindex_bindings::IRaindexV7::EvaluableV4;
    use std::vec;

    use super::*;
    use crate::types::common::{SgBigInt, SgBytes};

    #[test]
    fn test_try_into_call() {
        let owner_address = Address::random();
        let input_token_address = Address::random();
        let input_vault_id = B256::random();
        let output_token_address = Address::random();
        let output_vault_id = B256::random();

        let order = OrderV4 {
            owner: owner_address,
            evaluable: EvaluableV4 {
                interpreter: Address::random(),
                store: Address::random(),
                bytecode: bytes!("fefefefefe"),
            },
            validInputs: vec![IOV2 {
                token: input_token_address,
                vaultId: input_vault_id,
            }],
            validOutputs: vec![IOV2 {
                token: output_token_address,
                vaultId: output_vault_id,
            }],
            nonce: B256::random(),
        };

        let order_bytes = order.abi_encode();
        let order_bytes = SgBytes(hex::encode(order_bytes));

        let order_detail = SgOrder {
            // This is a dummy order detail object
            // All of these values are actually ignored by the conversion
            id: SgBytes("1".into()),
            order_hash: SgBytes("".into()),
            add_events: vec![],
            timestamp_added: SgBigInt("0".into()),
            owner: SgBytes("".into()),
            active: true,
            inputs: vec![],
            outputs: vec![],
            meta: None,
            raindex: SgRaindex {
                id: SgBytes("1".into()),
            },
            // Only the order_bytes field is used for the conversion
            order_bytes,
            trades: vec![],
            remove_events: vec![],
        };

        let order_v4: OrderV4 = order_detail.try_into().unwrap();

        assert_eq!(order_v4.owner, owner_address);
        assert_eq!(order_v4.validInputs[0].token, input_token_address);
        assert_eq!(order_v4.validInputs[0].vaultId, input_vault_id);
        assert_eq!(order_v4.validOutputs[0].token, output_token_address);
        assert_eq!(order_v4.validOutputs[0].vaultId, output_vault_id);
    }
}
