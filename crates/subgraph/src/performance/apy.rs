use super::{PerformanceError, YEAR18};
use crate::{
    performance::vol::VaultVolume,
    types::common::{SgErc20, SgTrade},
};
use alloy::primitives::U256;
use chrono::TimeDelta;
use rain_math_float::Float;
use raindex_math::{BigUintMath, ONE18};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
#[cfg(target_family = "wasm")]
use wasm_bindgen_utils::{impl_wasm_traits, prelude::*};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub struct APYDetails {
    pub start_time: u64,
    pub end_time: u64,
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub net_vol: Float,
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub capital: Float,
    #[cfg_attr(target_family = "wasm", tsify(type = "string | undefined"))]
    pub apy: Option<Float>,
    pub is_neg: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub struct VaultAPY {
    pub id: String,
    pub token: SgErc20,
    pub apy_details: Option<APYDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenPair {
    pub input: SgErc20,
    pub output: SgErc20,
}

#[cfg(target_family = "wasm")]
mod impls {
    use super::*;
    impl_wasm_traits!(VaultAPY);
    impl_wasm_traits!(APYDetails);
}

/// Calculates each token vault apy at the given timeframe
/// Trades must be sorted in desc order by timestamp, this is
/// the case if queried from subgraph using this lib functionalities
pub fn get_vaults_apy(
    trades: &[SgTrade],
    vols: &[VaultVolume],
    start_timestamp: Option<u64>,
    end_timestamp: Option<u64>,
) -> Result<Vec<VaultAPY>, PerformanceError> {
    let mut token_vaults_apy: Vec<VaultAPY> = vec![];
    for vol in vols {
        // this token vault trades in desc order by timestamp
        let vault_trades = trades
            .iter()
            .filter(|v| {
                (v.input_vault_balance_change.vault.vault_id.0 == vol.id
                    && v.input_vault_balance_change.vault.token == vol.token)
                    || (v.output_vault_balance_change.vault.vault_id.0 == vol.id
                        && v.output_vault_balance_change.vault.token == vol.token)
            })
            .collect::<Vec<&SgTrade>>();

        if vault_trades.is_empty() {
            token_vaults_apy.push(VaultAPY {
                id: vol.id.clone(),
                token: vol.token.clone(),
                apy_details: None,
            });
            continue;
        }

        // this token vault first trade, indictaes the start time
        // to find the end of the first day to find the starting capital
        let first_trade = vault_trades[vault_trades.len() - 1];
        let first_day_last_trade = vault_trades
            .iter()
            .filter(|v| {
                u64::from_str(&v.timestamp.0)
                    .ok()
                    .zip(u64::from_str(&first_trade.timestamp.0).ok())
                    .is_some_and(|(trade_time, first_trade_time)| {
                        trade_time <= first_trade_time + TimeDelta::days(1).num_seconds() as u64
                    })
            })
            .collect::<Vec<&&SgTrade>>()[0];

        // vaults starting capital at end of first day of its first ever trade
        // as 18 point decimals
        let vault_balance_change = if first_day_last_trade
            .input_vault_balance_change
            .vault
            .vault_id
            .0
            == vol.id
            && first_day_last_trade.input_vault_balance_change.vault.token == vol.token
        {
            &first_day_last_trade.input_vault_balance_change
        } else {
            &first_day_last_trade.output_vault_balance_change
        };
        let starting_capital = U256::from_str(&vault_balance_change.new_vault_balance.0)?
            .scale_18(vault_balance_change.vault.token.get_decimals()?)
            .map_err(PerformanceError::from)?;

        // the time range for this token vault
        let mut start = u64::from_str(&first_trade.timestamp.0)?;
        start_timestamp.inspect(|t| {
            if start > *t {
                start = *t;
            }
        });
        let end = end_timestamp.unwrap_or(chrono::Utc::now().timestamp() as u64);

        // this token vault apy in 18 decimals point
        let apy = if !starting_capital.is_zero() {
            match U256::from(end - start)
                .saturating_mul(ONE18)
                .div_18(YEAR18
            {
                Err(_) => None,
                Ok(annual_rate_18) => vol
                    .vol_details
                    .net_vol
                    .div_18(starting_capital)
                    .ok()
                    .and_then(|v| v.div_18(annual_rate_18).ok()),
            }
        } else {
            None
        };

        // this token vault apy
        token_vaults_apy.push(VaultAPY {
            id: vol.id.clone(),
            token: vol.token.clone(),
            apy_details: Some(APYDetails {
                start_time: start,
                end_time: end,
                apy,
                is_neg: vol.is_net_vol_negative(),
                net_vol: vol.vol_details.net_vol,
                capital: starting_capital,
            }),
        });
    }

    Ok(token_vaults_apy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        performance::vol::VolumeDetails,
        types::common::{
            SgBigInt, SgBytes, SgRaindex, SgTradeEvent, SgTradeStructPartialOrder,
            SgTradeVaultBalanceChange, SgTransaction, SgVaultBalanceChangeVault,
        },
    };
    use alloy::primitives::{Address, B256};

    #[test]
    fn test_get_vaults_apy_ok() {
        let tokens = get_tokens(SgBigInt(18.to_string()));
        let trades = get_trades(tokens.clone(), SgBigInt("5000000000000000000".to_string()));
        let [token1, token2] = tokens;
        let [vault1, vault2] = get_vault_ids();
        let vault_vol1 = VaultVolume {
            id: vault1.to_string(),
            token: token1.clone(),
            vol_details: VolumeDetails {
                total_in: U256::ZERO,
                total_out: U256::ZERO,
                total_vol: U256::ZERO,
                net_vol: U256::from_str("1000000000000000000").unwrap(),
            },
        };
        let vault_vol2 = VaultVolume {
            id: vault2.to_string(),
            token: token2.clone(),
            vol_details: VolumeDetails {
                total_in: U256::ZERO,
                total_out: U256::ZERO,
                total_vol: U256::ZERO,
                net_vol: U256::from_str("2000000000000000000").unwrap(),
            },
        };
        let result =
            get_vaults_apy(&trades, &[vault_vol1, vault_vol2], Some(1), Some(10000001)).unwrap();
        let expected = vec![
            VaultAPY {
                id: vault1.to_string(),
                token: token1.clone(),
                apy_details: Some(APYDetails {
                    start_time: 1,
                    end_time: 10000001,
                    net_vol: U256::from_str("1000000000000000000").unwrap(),
                    capital: U256::from_str("5000000000000000000").unwrap(),
                    // (1/5) / (10000001_end - 1_start / 31_536_00_year)
                    apy: Some(U256::from_str("630720000000000000").unwrap()),
                    is_neg: false,
                }),
            },
            VaultAPY {
                id: vault2.to_string(),
                token: token2.clone(),
                apy_details: Some(APYDetails {
                    start_time: 1,
                    end_time: 10000001,
                    net_vol: U256::from_str("2000000000000000000").unwrap(),
                    capital: U256::from_str("5000000000000000000").unwrap(),
                    // (2/5) / ((10000001_end - 1_start) / 31_536_00_year)
                    apy: Some(U256::from_str("1261440000000000000").unwrap()),
                    is_neg: false,
                }),
            },
        ];

        assert_eq!(result, expected);
    }

    #[test]
    fn test_get_vaults_apy_err() {
        let tokens = get_tokens(SgBigInt("bad".to_string()));
        let trades = get_trades(tokens.clone(), SgBigInt("2000000000000000000".to_string()));
        let [token1, token2] = tokens;
        let [vault1, vault2] = get_vault_ids();

        let zero = Float::default();
        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();

        let vault_vol1 = VaultVolume {
            id: vault1.to_string(),
            token: token1.clone(),
            vol_details: VolumeDetails {
                total_in: zero,
                total_out: zero,
                total_vol: zero,
                net_vol: one,
            },
        };
        let vault_vol2 = VaultVolume {
            id: vault2.to_string(),
            token: token2.clone(),
            vol_details: VolumeDetails {
                total_in: zero,
                total_out: zero,
                total_vol: zero,
                net_vol: two,
            },
        };

        let err = get_vaults_apy(&trades, &[vault_vol1, vault_vol2], Some(1), Some(10000001))
            .unwrap_err();

        assert!(matches!(err, PerformanceError::ParseIntError(_)));

        let tokens = get_tokens(SgBigInt("18".to_string()));
        let trades = get_trades(tokens.clone(), SgBigInt("bad".to_string()));
        let [token1, token2] = tokens;
        let [vault1, vault2] = get_vault_ids();

        let vault_vol1 = VaultVolume {
            id: vault1.to_string(),
            token: token1.clone(),
            vol_details: VolumeDetails {
                total_in: zero,
                total_out: zero,
                total_vol: zero,
                net_vol: one,
            },
        };
        let vault_vol2 = VaultVolume {
            id: vault2.to_string(),
            token: token2.clone(),
            vol_details: VolumeDetails {
                total_in: zero,
                total_out: zero,
                total_vol: zero,
                net_vol: two,
            },
        };

        let err = get_vaults_apy(&trades, &[vault_vol1, vault_vol2], Some(1), Some(10000001))
            .unwrap_err();

        assert!(matches!(err, PerformanceError::ParseUnsignedError(_)));
    }

    fn get_vault_ids() -> [B256; 2] {
        [
            B256::from_slice(&[0x11u8; 32]),
            B256::from_slice(&[0x22u8; 32]),
        ]
    }

    fn get_tokens(decimals: SgBigInt) -> [SgErc20; 2] {
        let token1_address = Address::from_slice(&[0x11u8; 20]);
        let token2_address = Address::from_slice(&[0x22u8; 20]);
        let token1 = SgErc20 {
            id: SgBytes(token1_address.to_string()),
            address: SgBytes(token1_address.to_string()),
            name: Some("Token1".to_string()),
            symbol: Some("Token1".to_string()),
            decimals: Some(decimals.clone()),
        };
        let token2 = SgErc20 {
            id: SgBytes(token2_address.to_string()),
            address: SgBytes(token2_address.to_string()),
            name: Some("Token2".to_string()),
            symbol: Some("Token2".to_string()),
            decimals: Some(decimals),
        };
        [token1, token2]
    }

    fn get_trades(tokens: [SgErc20; 2], new_vault_balance: SgBigInt) -> Vec<SgTrade> {
        let bytes = SgBytes("".to_string());
        let bigint = SgBigInt("".to_string());
        let [vault_id1, vault_id2] = get_vault_ids();
        let [token1, token2] = tokens;

        let two = Float::parse("2".to_string()).unwrap();
        let two_str = serde_json::to_string(&two).unwrap();
        let minus_two = (-two).unwrap();
        let minus_two_str = serde_json::to_string(&minus_two).unwrap();
        let five = Float::parse("5".to_string()).unwrap();
        let five_str = serde_json::to_string(&five).unwrap();
        let seven = Float::parse("7".to_string()).unwrap();
        let seven_str = serde_json::to_string(&seven).unwrap();

        let trade1 = SgTrade {
            id: bytes.clone(),
            order: SgTradeStructPartialOrder {
                id: bytes.clone(),
                order_hash: bytes.clone(),
                owner: bytes.clone(),
            },
            trade_event: SgTradeEvent {
                id: bytes.clone(),
                __typename: "TakeOrder".to_string(),
                sender: bytes.clone(),
                transaction: SgTransaction {
                    id: bytes.clone(),
                    from: bytes.clone(),
                    block_number: bigint.clone(),
                    timestamp: bigint.clone(),
                },
            },
            timestamp: SgBigInt("1".to_string()),
            raindex: SgRaindex { id: bytes.clone() },
            output_vault_balance_change: SgTradeVaultBalanceChange {
                id: bytes.clone(),
                __typename: "TradeVaultBalanceChange".to_string(),
                amount: SgBytes(minus_two_str.clone()),
                new_vault_balance: SgBytes(two_str.clone()),
                old_vault_balance: bigint.clone(),
                vault: SgVaultBalanceChangeVault {
                    id: bytes.clone(),
                    token: token1.clone(),
                    vault_id: SgBigInt(vault_id1.to_string()),
                },
                timestamp: SgBigInt("1".to_string()),
                transaction: SgTransaction {
                    id: bytes.clone(),
                    from: bytes.clone(),
                    block_number: bigint.clone(),
                    timestamp: SgBigInt("1".to_string()),
                },
                raindex: SgRaindex { id: bytes.clone() },
            },
            input_vault_balance_change: SgTradeVaultBalanceChange {
                id: bytes.clone(),
                __typename: "TradeVaultBalanceChange".to_string(),
                amount: SgBytes(five_str.clone()),
                new_vault_balance: SgBytes(two_str.clone()),
                old_vault_balance: bigint.clone(),
                vault: SgVaultBalanceChangeVault {
                    id: bytes.clone(),
                    token: token2.clone(),
                    vault_id: SgBigInt(vault_id2.to_string()),
                },
                timestamp: SgBigInt("1".to_string()),
                transaction: SgTransaction {
                    id: bytes.clone(),
                    from: bytes.clone(),
                    block_number: bigint.clone(),
                    timestamp: SgBigInt("1".to_string()),
                },
                raindex: SgRaindex { id: bytes.clone() },
            },
        };

        let trade2 = SgTrade {
            id: bytes.clone(),
            order: SgTradeStructPartialOrder {
                id: bytes.clone(),
                order_hash: bytes.clone(),
                owner: bytes.clone(),
            },
            trade_event: SgTradeEvent {
                id: bytes.clone(),
                __typename: "TakeOrder".to_string(),
                sender: bytes.clone(),
                transaction: SgTransaction {
                    id: bytes.clone(),
                    from: bytes.clone(),
                    block_number: bigint.clone(),
                    timestamp: bigint.clone(),
                },
            },
            timestamp: SgBigInt("2".to_string()),
            raindex: SgRaindex { id: bytes.clone() },
            output_vault_balance_change: SgTradeVaultBalanceChange {
                id: bytes.clone(),
                __typename: "TradeVaultBalanceChange".to_string(),
                amount: SgBytes(minus_two_str.clone()),
                new_vault_balance: new_vault_balance.clone(),
                old_vault_balance: bytes.clone(),
                vault: SgVaultBalanceChangeVault {
                    id: bytes.clone(),
                    token: token2.clone(),
                    vault_id: SgBytes(vault_id2.to_string()),
                },
                timestamp: SgBigInt("2".to_string()),
                transaction: SgTransaction {
                    id: bytes.clone(),
                    from: bytes.clone(),
                    block_number: bigint.clone(),
                    timestamp: SgBigInt("1".to_string()),
                },
                raindex: SgRaindex { id: bytes.clone() },
            },
            input_vault_balance_change: SgTradeVaultBalanceChange {
                id: bytes.clone(),
                __typename: "TradeVaultBalanceChange".to_string(),
                new_vault_balance,
                amount: SgBigInt(seven),
                old_vault_balance: bigint.clone(),
                vault: SgVaultBalanceChangeVault {
                    id: bytes.clone(),
                    token: token1.clone(),
                    vault_id: SgBytes(vault_id1.to_string()),
                },
                timestamp: SgBigInt("2".to_string()),
                transaction: SgTransaction {
                    id: bytes.clone(),
                    from: bytes.clone(),
                    block_number: bigint.clone(),
                    timestamp: SgBigInt("1".to_string()),
                },
                raindex: SgRaindex { id: bytes.clone() },
            },
        };
        vec![trade2, trade1]
    }
}
