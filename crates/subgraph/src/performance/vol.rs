use crate::{
    performance::PerformanceError,
    types::common::{SgErc20, SgTrade},
};
use rain_math_float::Float;
use serde::{Deserialize, Serialize};
use wasm_bindgen_utils::{impl_wasm_traits, prelude::*};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub struct VolumeDetails {
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub total_in: Float,
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub total_out: Float,
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub total_vol: Float,
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub net_vol: Float,
}

#[derive(Debug, Clone, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct VaultVolume {
    pub id: String,
    pub token: SgErc20,
    pub vol_details: VolumeDetails,
}
impl_wasm_traits!(VaultVolume);

#[cfg(target_family = "wasm")]
mod impls {
    use super::*;
    impl_wasm_traits!(VolumeDetails);
}

fn update_volume_details(
    vol_details: &mut VolumeDetails,
    amount_hex: &str,
) -> Result<(), PerformanceError> {
    let amount = Float::from_hex(amount_hex)?;
    let zero = Float::default();

    if amount.lt(zero)? {
        let abs_amount = (-amount)?;
        vol_details.total_out = (vol_details.total_out + abs_amount)?;
    } else {
        vol_details.total_in = (vol_details.total_in + amount)?;
    }

    // Derive totals to prevent drift.
    vol_details.total_vol = (vol_details.total_in + vol_details.total_out)?;
    vol_details.net_vol = (vol_details.total_in - vol_details.total_out)?;

    Ok(())
}

fn create_volume_details(amount_hex: &str) -> Result<VolumeDetails, PerformanceError> {
    let amount = Float::from_hex(amount_hex)?;
    let zero = Float::default();

    let (total_in, total_out) = if amount.lt(zero)? {
        (zero, (-amount)?)
    } else {
        (amount, zero)
    };

    let total_vol = (total_in + total_out)?;
    let net_vol = (total_in - total_out)?;

    Ok(VolumeDetails {
        total_in,
        total_out,
        total_vol,
        net_vol,
    })
}

/// Helper function to process a vault balance change
fn process_vault_balance_change(
    vaults_vol: &mut Vec<VaultVolume>,
    vault_id: &str,
    token: &SgErc20,
    amount: &str,
) -> Result<(), PerformanceError> {
    if let Some(vault_vol) = vaults_vol
        .iter_mut()
        .find(|v| v.id == vault_id && v.token.address.0 == token.address.0)
    {
        update_volume_details(&mut vault_vol.vol_details, amount)?;
    } else {
        vaults_vol.push(VaultVolume {
            id: vault_id.to_string(),
            token: token.clone(),
            vol_details: create_volume_details(amount)?,
        });
    }
    Ok(())
}

/// Get the vaults volume from array of trades of an order
pub fn get_vaults_vol(trades: &[SgTrade]) -> Result<Vec<VaultVolume>, PerformanceError> {
    let mut vaults_vol: Vec<VaultVolume> = vec![];
    for trade in trades {
        process_vault_balance_change(
            &mut vaults_vol,
            &trade.input_vault_balance_change.vault.vault_id.0,
            &trade.input_vault_balance_change.vault.token,
            &trade.input_vault_balance_change.amount.0,
        )?;

        process_vault_balance_change(
            &mut vaults_vol,
            &trade.output_vault_balance_change.vault.vault_id.0,
            &trade.output_vault_balance_change.vault.token,
            &trade.output_vault_balance_change.amount.0,
        )?;
    }
    Ok(vaults_vol)
}

impl VaultVolume {
    pub fn is_net_vol_negative(&self) -> Result<bool, PerformanceError> {
        Ok(self.vol_details.total_in.lt(self.vol_details.total_out)?)
    }
}

#[cfg(test)]
mod tests {
    use alloy::primitives::{Address, B256, U256};

    use super::*;
    use crate::types::common::{
        SgBigInt, SgBytes, SgRaindex, SgTradeEvent, SgTradeEventTypename, SgTradeRef,
        SgTradeStructPartialOrder, SgTradeVaultBalanceChange, SgTransaction,
        SgVaultBalanceChangeVault,
    };
    use crate::utils::float::*;
    use rain_math_float::Float;

    #[test]
    fn test_is_net_vol_negative() {
        let token_address = Address::random();
        let decimals = 6;

        let token = SgErc20 {
            id: SgBytes(token_address.to_string()),
            address: SgBytes(token_address.to_string()),
            name: Some("Token".to_string()),
            symbol: Some("Token".to_string()),
            decimals: Some(SgBigInt(decimals.to_string())),
        };

        let total_in = Float::from_fixed_decimal(U256::from(20_500_000), decimals).unwrap();
        let total_out = Float::from_fixed_decimal(U256::from(30_000_000), decimals).unwrap();
        let total_vol = Float::from_fixed_decimal(U256::from(50_500_000), decimals).unwrap();
        let net_vol = (total_in - total_out).unwrap();

        let vault_vol = VaultVolume {
            id: "vault-id".to_string(),
            token: token.clone(),
            vol_details: VolumeDetails {
                total_in,
                total_out,
                total_vol,
                net_vol,
            },
        };
        assert!(vault_vol.is_net_vol_negative().unwrap());

        let total_in = Float::from_fixed_decimal(U256::from(40_500_000), decimals).unwrap();
        let total_out = Float::from_fixed_decimal(U256::from(30_000_000), decimals).unwrap();
        let total_vol = Float::from_fixed_decimal(U256::from(70_500_000), decimals).unwrap();
        let net_vol = (total_in - total_out).unwrap();

        let vault_vol = VaultVolume {
            id: "vault-id".to_string(),
            token: token.clone(),
            vol_details: VolumeDetails {
                total_in,
                total_out,
                total_vol,
                net_vol,
            },
        };
        assert!(!vault_vol.is_net_vol_negative().unwrap());

        let total_in = Float::from_fixed_decimal(U256::from(30_000_000), decimals).unwrap();
        let total_out = Float::from_fixed_decimal(U256::from(30_000_000), decimals).unwrap();
        let total_vol = Float::from_fixed_decimal(U256::from(60_000_000), decimals).unwrap();
        let net_vol = (total_in - total_out).unwrap();

        let vault_vol = VaultVolume {
            id: "vault-id".to_string(),
            token: token.clone(),
            vol_details: VolumeDetails {
                total_in,
                total_out,
                total_vol,
                net_vol,
            },
        };
        assert!(!vault_vol.is_net_vol_negative().unwrap());
    }

    #[test]
    fn test_vaults_vol() {
        let bytes = SgBytes("".to_string());
        let bigint = SgBigInt("".to_string());
        let token1_address = Address::random();
        let token2_address = Address::random();
        let vault_id1 = B256::random();
        let vault_id2 = B256::random();
        let token1 = SgErc20 {
            id: SgBytes(token1_address.to_string()),
            address: SgBytes(token1_address.to_string()),
            name: Some("Token1".to_string()),
            symbol: Some("Token1".to_string()),
            decimals: Some(SgBigInt(18.to_string())),
        };
        let token2 = SgErc20 {
            id: SgBytes(token2_address.to_string()),
            address: SgBytes(token2_address.to_string()),
            name: Some("Token2".to_string()),
            symbol: Some("Token2".to_string()),
            decimals: Some(SgBigInt(18.to_string())),
        };

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
            timestamp: bigint.clone(),
            raindex: SgRaindex { id: bytes.clone() },
            output_vault_balance_change: SgTradeVaultBalanceChange {
                id: bytes.clone(),
                __typename: "TradeVaultBalanceChange".to_string(),
                amount: SgBytes(NEG2.as_hex()),
                new_vault_balance: bytes.clone(),
                old_vault_balance: bytes.clone(),
                vault: SgVaultBalanceChangeVault {
                    id: bytes.clone(),
                    token: token1.clone(),
                    vault_id: SgBytes(vault_id1.to_string()),
                },
                timestamp: bigint.clone(),
                transaction: SgTransaction {
                    id: bytes.clone(),
                    from: bytes.clone(),
                    block_number: bigint.clone(),
                    timestamp: bigint.clone(),
                },
                raindex: SgRaindex { id: bytes.clone() },
                trade: SgTradeRef {
                    trade_event: SgTradeEventTypename {
                        __typename: "TradeEvent".to_string(),
                    },
                },
            },
            input_vault_balance_change: SgTradeVaultBalanceChange {
                id: bytes.clone(),
                __typename: "TradeVaultBalanceChange".to_string(),
                amount: SgBytes(F5.as_hex()),
                new_vault_balance: bytes.clone(),
                old_vault_balance: bytes.clone(),
                vault: SgVaultBalanceChangeVault {
                    id: bytes.clone(),
                    token: token2.clone(),
                    vault_id: SgBytes(vault_id2.to_string()),
                },
                timestamp: bigint.clone(),
                transaction: SgTransaction {
                    id: bytes.clone(),
                    from: bytes.clone(),
                    block_number: bigint.clone(),
                    timestamp: bigint.clone(),
                },
                raindex: SgRaindex { id: bytes.clone() },
                trade: SgTradeRef {
                    trade_event: SgTradeEventTypename {
                        __typename: "TradeEvent".to_string(),
                    },
                },
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
            timestamp: bigint.clone(),
            raindex: SgRaindex { id: bytes.clone() },
            output_vault_balance_change: SgTradeVaultBalanceChange {
                id: bytes.clone(),
                __typename: "TradeVaultBalanceChange".to_string(),
                amount: SgBytes(NEG7.as_hex()),
                new_vault_balance: bytes.clone(),
                old_vault_balance: bytes.clone(),
                vault: SgVaultBalanceChangeVault {
                    id: bytes.clone(),
                    token: token2.clone(),
                    vault_id: SgBytes(vault_id2.to_string()),
                },
                timestamp: bigint.clone(),
                transaction: SgTransaction {
                    id: bytes.clone(),
                    from: bytes.clone(),
                    block_number: bigint.clone(),
                    timestamp: bigint.clone(),
                },
                raindex: SgRaindex { id: bytes.clone() },
                trade: SgTradeRef {
                    trade_event: SgTradeEventTypename {
                        __typename: "TradeEvent".to_string(),
                    },
                },
            },
            input_vault_balance_change: SgTradeVaultBalanceChange {
                id: bytes.clone(),
                __typename: "TradeVaultBalanceChange".to_string(),
                amount: SgBytes(F3.as_hex()),
                new_vault_balance: bytes.clone(),
                old_vault_balance: bytes.clone(),
                vault: SgVaultBalanceChangeVault {
                    id: bytes.clone(),
                    token: token1.clone(),
                    vault_id: SgBytes(vault_id1.to_string()),
                },
                timestamp: bigint.clone(),
                transaction: SgTransaction {
                    id: bytes.clone(),
                    from: bytes.clone(),
                    block_number: bigint.clone(),
                    timestamp: bigint.clone(),
                },
                raindex: SgRaindex { id: bytes.clone() },
                trade: SgTradeRef {
                    trade_event: SgTradeEventTypename {
                        __typename: "TradeEvent".to_string(),
                    },
                },
            },
        };

        let result = get_vaults_vol(&[trade1, trade2]).unwrap();
        let expected = [
            VaultVolume {
                id: vault_id2.to_string(),
                token: token2,
                vol_details: VolumeDetails {
                    total_in: F5,
                    total_out: F7,
                    total_vol: F12,
                    net_vol: NEG2,
                },
            },
            VaultVolume {
                id: vault_id1.to_string(),
                token: token1,
                vol_details: VolumeDetails {
                    total_in: F3,
                    total_out: F2,
                    total_vol: F5,
                    net_vol: F1,
                },
            },
        ];

        assert_eq!(result.len(), expected.len());

        assert_eq!(result[0].id, expected[0].id);
        assert_eq!(result[0].token, expected[0].token);
        assert!(result[0]
            .vol_details
            .total_in
            .eq(expected[0].vol_details.total_in)
            .unwrap());
        assert!(result[0]
            .vol_details
            .total_out
            .eq(expected[0].vol_details.total_out)
            .unwrap());
        assert!(result[0]
            .vol_details
            .total_vol
            .eq(expected[0].vol_details.total_vol)
            .unwrap());
        assert!(result[0]
            .vol_details
            .net_vol
            .eq(expected[0].vol_details.net_vol)
            .unwrap());

        assert_eq!(result[1].id, expected[1].id);
        assert_eq!(result[1].token, expected[1].token);
        assert!(result[1]
            .vol_details
            .total_in
            .eq(expected[1].vol_details.total_in)
            .unwrap());
        assert!(result[1]
            .vol_details
            .total_out
            .eq(expected[1].vol_details.total_out)
            .unwrap());
        assert!(result[1]
            .vol_details
            .total_vol
            .eq(expected[1].vol_details.total_vol)
            .unwrap());
        assert!(result[1]
            .vol_details
            .net_vol
            .eq(expected[1].vol_details.net_vol)
            .unwrap());
    }

    #[test]
    fn test_update_volume_details() {
        let mut vol_details = VolumeDetails {
            total_in: F1,
            total_out: F5,
            total_vol: F6,
            net_vol: (F1 - F5).unwrap(),
        };

        update_volume_details(&mut vol_details, &F20.as_hex()).unwrap();
        assert!(vol_details.total_in.eq((F1 + F20).unwrap()).unwrap());
        assert!(vol_details.total_out.eq(F5).unwrap());
        assert!(vol_details.total_vol.eq((F6 + F20).unwrap()).unwrap());
        let expected_net = ((F1 + F20).unwrap() - F5).unwrap();
        assert!(vol_details.net_vol.eq(expected_net).unwrap());

        update_volume_details(&mut vol_details, &NEG5.as_hex()).unwrap();
        assert!(vol_details.total_in.eq((F1 + F20).unwrap()).unwrap());
        assert!(vol_details.total_out.eq(F10).unwrap());
    }

    #[test]
    fn test_create_volume_details() {
        let vol_details = create_volume_details(&F20.as_hex()).unwrap();
        assert!(vol_details.total_in.eq(F20).unwrap());
        assert!(vol_details.total_out.eq(F0).unwrap());
        assert!(vol_details.total_vol.eq(F20).unwrap());
        assert!(vol_details.net_vol.eq(F20).unwrap());

        let vol_details = create_volume_details(&NEG5.as_hex()).unwrap();
        assert!(vol_details.total_in.eq(F0).unwrap());
        assert!(vol_details.total_out.eq(F5).unwrap());
        assert!(vol_details.total_vol.eq(F5).unwrap());
        assert!(vol_details.net_vol.eq(NEG5).unwrap());

        let err = create_volume_details("bad_hex").unwrap_err();
        assert!(matches!(err, PerformanceError::FloatError(_)));
    }

    #[test]
    fn test_process_vault_balance_change() {
        let mut vaults_vol = Vec::new();
        let token = SgErc20 {
            id: SgBytes("token1".to_string()),
            address: SgBytes("token1".to_string()),
            name: Some("Token1".to_string()),
            symbol: Some("Token1".to_string()),
            decimals: Some(SgBigInt(18.to_string())),
        };

        process_vault_balance_change(&mut vaults_vol, "vault1", &token, &F20.as_hex()).unwrap();
        assert_eq!(vaults_vol.len(), 1);
        assert_eq!(vaults_vol[0].id, "vault1");
        assert!(vaults_vol[0].vol_details.total_in.eq(F20).unwrap());

        process_vault_balance_change(&mut vaults_vol, "vault1", &token, &NEG5.as_hex()).unwrap();
        assert_eq!(vaults_vol.len(), 1);
        assert!(vaults_vol[0].vol_details.total_in.eq(F20).unwrap());
        assert!(vaults_vol[0].vol_details.total_out.eq(F5).unwrap());
    }

    // TODO: APY related logic - overflow test for Float arithmetic
    // #[test]
    // fn test_vaults_vol_overflow() {
    //     let bytes = SgBytes("".to_string());
    //     let bigint = SgBigInt("".to_string());
    //     let token1_address = Address::random();
    //     let token2_address = Address::random();
    //     let vault_id1 = B256::random();
    //     let vault_id2 = B256::random();
    //
    //     let token1 = SgErc20 {
    //         id: SgBytes(token1_address.to_string()),
    //         address: SgBytes(token1_address.to_string()),
    //         name: Some("Token1".to_string()),
    //         symbol: Some("Token1".to_string()),
    //         decimals: Some(SgBigInt(18.to_string())),
    //     };
    //
    //     let token2 = SgErc20 {
    //         id: SgBytes(token2_address.to_string()),
    //         address: SgBytes(token2_address.to_string()),
    //         name: Some("Token2".to_string()),
    //         symbol: Some("Token2".to_string()),
    //         decimals: Some(SgBigInt(18.to_string())),
    //     };
    //
    //     let trade1 = SgTrade {
    //         id: bytes.clone(),
    //         order: SgTradeStructPartialOrder {
    //             id: bytes.clone(),
    //             order_hash: bytes.clone(),
    //         },
    //         trade_event: SgTradeEvent {
    //             sender: bytes.clone(),
    //             transaction: SgTransaction {
    //                 id: bytes.clone(),
    //                 from: bytes.clone(),
    //                 block_number: bigint.clone(),
    //                 timestamp: bigint.clone(),
    //             },
    //         },
    //         timestamp: bigint.clone(),
    //         raindex: SgRaindex { id: bytes.clone() },
    //         output_vault_balance_change: SgTradeVaultBalanceChange {
    //             id: bytes.clone(),
    //             __typename: "TradeVaultBalanceChange".to_string(),
    //             amount: SgBytes(NEG1.as_hex()),
    //             new_vault_balance: bytes.clone(),
    //             old_vault_balance: bytes.clone(),
    //             vault: SgVaultBalanceChangeVault {
    //                 id: bytes.clone(),
    //                 token: token1.clone(),
    //                 vault_id: SgBytes(vault_id1.to_string()),
    //             },
    //             timestamp: bigint.clone(),
    //             transaction: SgTransaction {
    //                 id: bytes.clone(),
    //                 from: bytes.clone(),
    //                 block_number: bigint.clone(),
    //                 timestamp: bigint.clone(),
    //             },
    //             raindex: SgRaindex { id: bytes.clone() },
    //         },
    //         input_vault_balance_change: SgTradeVaultBalanceChange {
    //             id: bytes.clone(),
    //             __typename: "TradeVaultBalanceChange".to_string(),
    //             amount: SgBytes(FMAX.as_hex()),
    //             new_vault_balance: bytes.clone(),
    //             old_vault_balance: bytes.clone(),
    //             vault: SgVaultBalanceChangeVault {
    //                 id: bytes.clone(),
    //                 token: token2.clone(),
    //                 vault_id: SgBytes(vault_id2.to_string()),
    //             },
    //             timestamp: bigint.clone(),
    //             transaction: SgTransaction {
    //                 id: bytes.clone(),
    //                 from: bytes.clone(),
    //                 block_number: bigint.clone(),
    //                 timestamp: bigint.clone(),
    //             },
    //             raindex: SgRaindex { id: bytes.clone() },
    //         },
    //     };
    //
    //     let trade2 = SgTrade {
    //         id: bytes.clone(),
    //         order: SgTradeStructPartialOrder {
    //             id: bytes.clone(),
    //             order_hash: bytes.clone(),
    //         },
    //         trade_event: SgTradeEvent {
    //             sender: bytes.clone(),
    //             transaction: SgTransaction {
    //                 id: bytes.clone(),
    //                 from: bytes.clone(),
    //                 block_number: bigint.clone(),
    //                 timestamp: bigint.clone(),
    //             },
    //         },
    //         timestamp: bigint.clone(),
    //         raindex: SgRaindex { id: bytes.clone() },
    //         output_vault_balance_change: SgTradeVaultBalanceChange {
    //             id: bytes.clone(),
    //             __typename: "TradeVaultBalanceChange".to_string(),
    //             amount: SgBytes(F1.as_hex()),
    //             new_vault_balance: bytes.clone(),
    //             old_vault_balance: bytes.clone(),
    //             vault: SgVaultBalanceChangeVault {
    //                 id: bytes.clone(),
    //                 token: token2.clone(),
    //                 vault_id: SgBytes(vault_id2.to_string()),
    //             },
    //             timestamp: bigint.clone(),
    //             transaction: SgTransaction {
    //                 id: bytes.clone(),
    //                 from: bytes.clone(),
    //                 block_number: bigint.clone(),
    //                 timestamp: bigint.clone(),
    //             },
    //             raindex: SgRaindex { id: bytes.clone() },
    //         },
    //         input_vault_balance_change: SgTradeVaultBalanceChange {
    //             id: bytes.clone(),
    //             __typename: "TradeVaultBalanceChange".to_string(),
    //             amount: SgBytes(F1.as_hex()),
    //             new_vault_balance: bytes.clone(),
    //             old_vault_balance: bytes.clone(),
    //             vault: SgVaultBalanceChangeVault {
    //                 id: bytes.clone(),
    //                 token: token1.clone(),
    //                 vault_id: SgBytes(vault_id1.to_string()),
    //             },
    //             timestamp: bigint.clone(),
    //             transaction: SgTransaction {
    //                 id: bytes.clone(),
    //                 from: bytes.clone(),
    //                 block_number: bigint.clone(),
    //                 timestamp: bigint.clone(),
    //             },
    //             raindex: SgRaindex { id: bytes.clone() },
    //         },
    //     };
    //
    //     let err = get_vaults_vol(&[trade1, trade2]).unwrap_err();
    //     assert!(matches!(err, PerformanceError::FloatError(_)));
    // }
}
