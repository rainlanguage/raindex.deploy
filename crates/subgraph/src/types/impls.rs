use super::common::*;
use crate::performance::PerformanceError;
use rain_math_float::Float;

impl SgErc20 {
    pub fn get_decimals(&self) -> Result<u8, PerformanceError> {
        let decimalstr = self
            .decimals
            .as_ref()
            .map(|v| v.0.as_str())
            .ok_or(PerformanceError::MissingDecimals)?;

        let decimals = decimalstr.parse::<u8>()?;
        Ok(decimals)
    }
}

impl SgTrade {
    /// Calculates the trade's I/O ratio
    pub fn ratio(&self) -> Result<Float, PerformanceError> {
        let input = Float::from_hex(&self.input_vault_balance_change.amount.0.clone())?;
        let output = Float::from_hex(&self.output_vault_balance_change.amount.0.clone())?;

        let input = input.abs()?;
        let output = output.abs()?;

        if output.is_zero()? {
            Err(PerformanceError::DivByZero)
        } else {
            Ok((input / output)?)
        }
    }

    /// Calculates the trade's O/I ratio (inverse)
    pub fn inverse_ratio(&self) -> Result<Float, PerformanceError> {
        let input = Float::from_hex(&self.input_vault_balance_change.amount.0.clone())?;
        let output = Float::from_hex(&self.output_vault_balance_change.amount.0.clone())?;

        let input = input.abs()?;
        let output = output.abs()?;

        if output.is_zero()? {
            Err(PerformanceError::DivByZero)
        } else {
            Ok((output / input)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::{
        SgBigInt, SgBytes, SgRaindex, SgTradeEvent, SgTradeStructPartialOrder,
        SgTradeVaultBalanceChange, SgTransaction, SgVaultBalanceChangeVault,
    };
    use crate::utils::float::*;

    use alloy::primitives::Address;

    #[test]
    fn test_token_get_decimals_ok() {
        let token = SgErc20 {
            id: SgBytes(Address::from_slice(&[0x11u8; 20]).to_string()),
            address: SgBytes(Address::from_slice(&[0x11u8; 20]).to_string()),
            name: Some("Token1".to_string()),
            symbol: Some("Token1".to_string()),
            decimals: Some(SgBigInt(6.to_string())),
        };
        let result = token.get_decimals().unwrap();
        assert_eq!(result, 6);
    }

    #[test]
    fn test_token_get_decimals_err() {
        let token = SgErc20 {
            id: SgBytes(Address::from_slice(&[0x11u8; 20]).to_string()),
            address: SgBytes(Address::from_slice(&[0x11u8; 20]).to_string()),
            name: Some("Token1".to_string()),
            symbol: Some("Token1".to_string()),
            decimals: None,
        };
        let result = token.get_decimals().unwrap_err();
        assert!(matches!(result, PerformanceError::MissingDecimals));

        let token = SgErc20 {
            id: SgBytes(Address::from_slice(&[0x11u8; 20]).to_string()),
            address: SgBytes(Address::from_slice(&[0x11u8; 20]).to_string()),
            name: Some("Token1".to_string()),
            symbol: Some("Token1".to_string()),
            decimals: Some(SgBigInt("".to_string())),
        };
        let err = token.get_decimals().unwrap_err();
        assert!(matches!(err, PerformanceError::ParseIntError(_)));

        let token = SgErc20 {
            id: SgBytes(Address::from_slice(&[0x11u8; 20]).to_string()),
            address: SgBytes(Address::from_slice(&[0x11u8; 20]).to_string()),
            name: Some("Token1".to_string()),
            symbol: Some("Token1".to_string()),
            decimals: Some(SgBigInt("not a number".to_string())),
        };
        let err = token.get_decimals().unwrap_err();
        assert!(matches!(err, PerformanceError::ParseIntError(_)));

        let token = SgErc20 {
            id: SgBytes(Address::from_slice(&[0x11u8; 20]).to_string()),
            address: SgBytes(Address::from_slice(&[0x11u8; 20]).to_string()),
            name: Some("Token1".to_string()),
            symbol: Some("Token1".to_string()),
            decimals: Some(SgBigInt("-1".to_string())),
        };
        let err = token.get_decimals().unwrap_err();
        assert!(matches!(err, PerformanceError::ParseIntError(_)));
    }

    #[test]
    fn test_ratio_happy() {
        let result = get_trade().ratio().unwrap();
        assert!(
            result.eq(F0_5).unwrap(),
            "unexpected result: {}",
            result.format().unwrap()
        );
    }

    #[test]
    fn test_ratio_unhappy() {
        let mut trade = get_trade();
        let amount = Float::parse("0".to_string()).unwrap();
        let amount_str = serde_json::to_string(&amount).unwrap();
        trade.output_vault_balance_change.amount = SgBytes(amount_str);
        matches!(trade.ratio().unwrap_err(), PerformanceError::DivByZero);
    }

    #[test]
    fn test_inverse_ratio_happy() {
        let result = get_trade().inverse_ratio().unwrap();
        let expected = Float::parse("2".to_string()).unwrap();
        assert!(
            result.eq(expected).unwrap(),
            "unexpected result: {}",
            result.format().unwrap()
        );
    }

    #[test]
    fn test_inverse_ratio_unhappy() {
        let mut trade = get_trade();
        let amount = Float::parse("0".to_string()).unwrap();
        let amount_str = serde_json::to_string(&amount).unwrap();
        trade.input_vault_balance_change.amount = SgBytes(amount_str);
        matches!(
            trade.inverse_ratio().unwrap_err(),
            PerformanceError::DivByZero
        );
    }

    // helper to get trade struct
    fn get_trade() -> SgTrade {
        let token_address = Address::from_slice(&[0x11u8; 20]);
        let token = SgErc20 {
            id: SgBytes(token_address.to_string()),
            address: SgBytes(token_address.to_string()),
            name: Some("Token1".to_string()),
            symbol: Some("Token1".to_string()),
            decimals: Some(SgBigInt(6.to_string())),
        };

        let input_trade_vault_balance_change = SgTradeVaultBalanceChange {
            id: SgBytes("".to_string()),
            __typename: "".to_string(),
            amount: SgBytes(F3.as_hex()),
            new_vault_balance: SgBytes("".to_string()),
            old_vault_balance: SgBytes("".to_string()),
            vault: SgVaultBalanceChangeVault {
                id: SgBytes("".to_string()),
                vault_id: SgBytes("".to_string()),
                token: token.clone(),
            },
            timestamp: SgBigInt("".to_string()),
            transaction: SgTransaction {
                id: SgBytes("".to_string()),
                from: SgBytes("".to_string()),
                block_number: SgBigInt("".to_string()),
                timestamp: SgBigInt("".to_string()),
            },
            raindex: SgRaindex {
                id: SgBytes("".to_string()),
            },
            trade: SgTradeRef {
                trade_event: SgTradeEventTypename {
                    __typename: "TakeOrder".to_string(),
                },
            },
        };

        let output_trade_vault_balance_change = SgTradeVaultBalanceChange {
            id: SgBytes("".to_string()),
            __typename: "".to_string(),
            amount: SgBytes(NEG6.as_hex()),
            new_vault_balance: SgBytes("".to_string()),
            old_vault_balance: SgBytes("".to_string()),
            vault: SgVaultBalanceChangeVault {
                id: SgBytes("".to_string()),
                vault_id: SgBytes("".to_string()),
                token: token.clone(),
            },
            timestamp: SgBigInt("".to_string()),
            transaction: SgTransaction {
                id: SgBytes("".to_string()),
                from: SgBytes("".to_string()),
                block_number: SgBigInt("".to_string()),
                timestamp: SgBigInt("".to_string()),
            },
            raindex: SgRaindex {
                id: SgBytes("".to_string()),
            },
            trade: SgTradeRef {
                trade_event: SgTradeEventTypename {
                    __typename: "TakeOrder".to_string(),
                },
            },
        };

        SgTrade {
            id: SgBytes("".to_string()),
            trade_event: SgTradeEvent {
                id: SgBytes("".to_string()),
                __typename: "TakeOrder".to_string(),
                transaction: SgTransaction {
                    id: SgBytes("".to_string()),
                    from: SgBytes("".to_string()),
                    block_number: SgBigInt("".to_string()),
                    timestamp: SgBigInt("".to_string()),
                },
                sender: SgBytes("".to_string()),
            },
            output_vault_balance_change: output_trade_vault_balance_change,
            input_vault_balance_change: input_trade_vault_balance_change,
            order: SgTradeStructPartialOrder {
                id: SgBytes("".to_string()),
                order_hash: SgBytes("".to_string()),
                owner: SgBytes("".to_string()),
            },
            timestamp: SgBigInt("".to_string()),
            raindex: SgRaindex {
                id: SgBytes("".to_string()),
            },
        }
    }
}
