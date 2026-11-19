use super::apy::APYDetails;
use super::vol::VolumeDetails;
use super::{PerformanceError, YEAR18};
use crate::performance::apy::{get_vaults_apy, TokenPair};
use crate::{
    performance::vol::get_vaults_vol,
    types::common::{SgErc20, SgOrder, SgTrade},
};
use alloy::primitives::U256;
use rain_math_float::Float;
use raindex_math::{BigUintMath, ONE18};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
#[cfg(target_family = "wasm")]
use wasm_bindgen_utils::{impl_wasm_traits, prelude::*};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub struct VaultPerformance {
    /// vault id
    pub id: String,
    /// vault token
    pub token: SgErc20,
    /// vault vol segment
    pub vol_details: VolumeDetails,
    /// vault apy segment
    pub apy_details: Option<APYDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub struct DenominatedPerformance {
    /// The denomination token
    pub token: SgErc20,
    /// Order's APY raw value
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub apy: U256,
    /// Determines if apy is negative or not
    pub apy_is_neg: bool,
    /// Order's net vol raw value
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub net_vol: U256,
    /// Determines if net_vol is negative or not
    pub net_vol_is_neg: bool,
    /// Order's starting capital
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub starting_capital: U256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub struct OrderPerformance {
    /// Order subgraph id
    pub order_id: String,
    /// Order hash
    pub order_hash: String,
    /// Order's raindex
    pub raindex: String,
    /// Order's measured performance as a whole
    pub denominated_performance: Option<DenominatedPerformance>,
    /// Start timestamp of the performance measring timeframe
    pub start_time: u64,
    /// End timestamp of the performance measuring timeframe
    pub end_time: u64,
    /// Order's input vaults isolated performance
    pub inputs_vaults: Vec<VaultPerformance>,
    /// Order's output vaults isolated performance
    pub outputs_vaults: Vec<VaultPerformance>,
}

#[cfg(target_family = "wasm")]
mod impls {
    use super::*;
    impl_wasm_traits!(OrderPerformance);
    impl_wasm_traits!(DenominatedPerformance);
    impl_wasm_traits!(VaultPerformance);
}

impl OrderPerformance {
    /// Given an order and its trades and optionally a timeframe, will calculates
    /// the order performance, (apy and volume)
    /// Trades must be sorted in desc order by timestamp, this is the case if
    /// queried from subgraph using this lib functionalities
    pub fn measure(
        order: &SgOrder,
        trades: &[SgTrade],
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
    ) -> Result<OrderPerformance, PerformanceError> {
        // return early if there are no trades
        if trades.is_empty() {
            return Err(PerformanceError::NoTrades);
        }
        let vaults_vol = get_vaults_vol(trades)?;
        let vaults_apy = get_vaults_apy(trades, &vaults_vol, start_timestamp, end_timestamp)?;

        // build an OrderPerformance struct
        // pick the order's whole performance timeframe from the vaults biggest timeframe
        // and put the calculated vaults vol and apy into inputs and outputs vaults fields
        let mut start_time = u64::MAX;
        let mut end_time = 0_u64;
        let mut inputs: Vec<VaultPerformance> = vec![];
        let mut outputs: Vec<VaultPerformance> = vec![];
        for (vault_apy, vault_vol) in vaults_apy.iter().zip(vaults_vol) {
            vault_apy.apy_details.inspect(|v| {
                if v.start_time < start_time {
                    start_time = v.start_time;
                }
                if v.end_time > end_time {
                    end_time = v.end_time;
                }
            });
            if order
                .inputs
                .iter()
                .any(|v| v.vault_id.0 == vault_apy.id && v.token == vault_apy.token)
            {
                inputs.push(VaultPerformance {
                    id: vault_apy.id.clone(),
                    token: vault_apy.token.clone(),
                    apy_details: vault_apy.apy_details,
                    vol_details: vault_vol.vol_details,
                });
            }
            if order
                .outputs
                .iter()
                .any(|v| v.vault_id.0 == vault_apy.id && v.token == vault_apy.token)
            {
                outputs.push(VaultPerformance {
                    id: vault_apy.id.clone(),
                    token: vault_apy.token.clone(),
                    apy_details: vault_apy.apy_details,
                    vol_details: vault_vol.vol_details,
                });
            }
        }
        let mut order_performance = OrderPerformance {
            order_id: order.id.0.clone(),
            order_hash: order.order_hash.0.clone(),
            raindex: order.raindex.id.0.clone(),
            start_time,
            end_time,
            inputs_vaults: inputs,
            outputs_vaults: outputs,
            denominated_performance: None,
        };

        // get pairs ratios
        let pair_ratio_map = get_order_pairs_ratio(order, trades);

        // try to calculate all vaults capital and volume denominated into each of
        // the order's tokens by checking if there is direct ratio between the tokens,
        // multi path ratios are ignored currently and results in None for the APY.
        // if there is a success for any of the denomination tokens, gather it in order
        // of its net vol and pick the one with highest net vol.
        // if there was no success with any of the order's tokens, simply return None
        // for the APY.
        let mut processed_tokens: Vec<&SgErc20> = vec![];
        let mut tokens_vol_list: Vec<TokenBasedVol> = vec![];
        let mut token_denominated_performance = vec![];
        for token in &vaults_apy {
            // skip if token is alreaedy processed
            if processed_tokens.contains(&&token.token) {
                continue;
            } else {
                processed_tokens.push(&token.token);
            }
            let mut noway = false;
            let mut net_vol_is_neg = false;
            let mut net_vol_rate_is_neg = false;
            let mut acc_capital = U256::ZERO;
            let mut acc_net_vol = U256::ZERO;
            let mut acc_annual_rate_vol = U256::ZERO;
            let mut current_token_vol_list: Vec<TokenBasedVol> = vec![];
            for token_vault in &vaults_apy {
                if let Some(apy_details) = token_vault.apy_details {
                    // this vault's timeframe to year ratio
                    let annual_rate = U256::from(apy_details.end_time - apy_details.start_time)
                        .saturating_mul(ONE18)
                        .div_18(YEAR18
                        .map_err(PerformanceError::from)?;

                    // sum up all token vaults' capitals and vols by using the direct ratio between the tokens
                    if token_vault.token == token.token {
                        acc_capital += apy_details.capital;
                        (acc_net_vol, net_vol_is_neg) = accumulate(
                            (apy_details.net_vol, apy_details.is_neg),
                            (acc_net_vol, net_vol_is_neg),
                        );

                        let annual_rate_vol = apy_details
                            .net_vol
                            .div_18(annual_rate)
                            .map_err(PerformanceError::from)?;
                        (acc_annual_rate_vol, net_vol_rate_is_neg) = accumulate(
                            (annual_rate_vol, apy_details.is_neg),
                            (acc_annual_rate_vol, net_vol_rate_is_neg),
                        );

                        current_token_vol_list.push(TokenBasedVol {
                            net_vol: apy_details.net_vol,
                            is_neg: apy_details.is_neg,
                            token: &token.token,
                        });
                    } else {
                        let pair = TokenPair {
                            input: token.token.clone(),
                            output: token_vault.token.clone(),
                        };
                        // convert to current denomination by the direct pair ratio if it exists
                        if let Some(Some(ratio)) = pair_ratio_map.get(&pair) {
                            let capital_converted = apy_details
                                .capital
                                .mul_18(*ratio)
                                .map_err(PerformanceError::from)?;
                            acc_capital += capital_converted;

                            let net_vol_converted = apy_details
                                .net_vol
                                .mul_18(*ratio)
                                .map_err(PerformanceError::from)?;
                            (acc_net_vol, net_vol_is_neg) = accumulate(
                                (net_vol_converted, apy_details.is_neg),
                                (acc_net_vol, net_vol_is_neg),
                            );

                            let annual_rate_vol_converted = net_vol_converted
                                .div_18(annual_rate)
                                .map_err(PerformanceError::from)?;
                            (acc_annual_rate_vol, net_vol_rate_is_neg) = accumulate(
                                (annual_rate_vol_converted, apy_details.is_neg),
                                (acc_annual_rate_vol, net_vol_rate_is_neg),
                            );

                            current_token_vol_list.push(TokenBasedVol {
                                net_vol: net_vol_converted,
                                is_neg: apy_details.is_neg,
                                token: &token_vault.token,
                            });
                        } else {
                            // if found no way to convert (there were no direct ratio between the tokens),
                            // break the loop and go to the next token and try that
                            noway = true;
                            break;
                        }
                    }
                }
            }

            // for every success apy calc in a token denomination, gather them in an array,
            // this means at the end we have all the successful apy calculated in each of
            // the order's io tokens, and will pick the one that its token had the highest
            // net vol amon all other vaults
            if !noway {
                if let Ok(apy) = acc_annual_rate_vol.div_18(acc_capital) {
                    token_denominated_performance.push(DenominatedPerformance {
                        apy,
                        apy_is_neg: net_vol_rate_is_neg,
                        token: token.token.clone(),
                        starting_capital: acc_capital,
                        net_vol: acc_net_vol,
                        net_vol_is_neg,
                    });
                }

                // if we found a way to calculate apy in the current token denomination,
                // we'll include all the tokens vaults net vol in this array to have a list
                // of tokens net vols converted to current token
                // later, sorting this array will give us the highest to lowest tokens net vols
                // to pick the denomination from
                if tokens_vol_list.is_empty() {
                    tokens_vol_list.extend(current_token_vol_list);
                }
            }
        }

        // after array is sorted, pick the denomination with highest net vol by
        // iterating over tokens with highest vol to lowest and pick the first matching one
        tokens_vol_list.sort();
        for token in tokens_vol_list.iter().rev() {
            if let Some(denominated_performance) = token_denominated_performance
                .iter()
                .find(|&v| &v.token == token.token)
            {
                order_performance.denominated_performance = Some(denominated_performance.clone());
                // return early as soon as a match is found
                return Ok(order_performance);
            }
        }

        Ok(order_performance)
    }
}

/// Calculates an order's pairs' ratios from their last trades in a given list of trades
/// Trades must be sorted in desc order by timestamp, this is the case if queried from subgraph
/// using this lib functionalities
pub fn get_order_pairs_ratio(
    order: &SgOrder,
    trades: &[SgTrade],
) -> HashMap<TokenPair, Option<U256>> {
    let mut pair_ratio_map: HashMap<TokenPair, Option<U256>> = HashMap::new();
    for input in &order.inputs {
        for output in &order.outputs {
            let pair_as_key = TokenPair {
                input: input.token.clone(),
                output: output.token.clone(),
            };
            let inverse_pair_as_key = TokenPair {
                input: output.token.clone(),
                output: input.token.clone(),
            };
            // if not same io token and ratio map doesnt already include them
            if input.token != output.token
                && !(pair_ratio_map.contains_key(&pair_as_key)
                    || pair_ratio_map.contains_key(&inverse_pair_as_key))
            {
                // find this pairs(io or oi) latest trade from list of order's
                // trades, to calculate the pair ratio (in amount/out amount) and
                // its inverse from the latest trade that involes these 2 tokens.
                let ratio = trades
                    .iter()
                    .find(|v| {
                        (v.input_vault_balance_change.vault.token == input.token
                            && v.output_vault_balance_change.vault.token == output.token)
                            || (v.output_vault_balance_change.vault.token == input.token
                                && v.input_vault_balance_change.vault.token == output.token)
                    })
                    .and_then(|latest_trade| {
                        latest_trade
                            .ratio()
                            .ok()
                            .zip(latest_trade.inverse_ratio().ok())
                            .map(|(ratio, inverse_ratio)| {
                                if latest_trade.input_vault_balance_change.vault.token
                                    == input.token
                                {
                                    [ratio, inverse_ratio]
                                } else {
                                    [inverse_ratio, ratio]
                                }
                            })
                    });

                // io
                pair_ratio_map.insert(pair_as_key, ratio.map(|v| v[0]));
                // oi
                pair_ratio_map.insert(inverse_pair_as_key, ratio.map(|v| v[1]));
            }
        }
    }

    pair_ratio_map
}

fn accumulate(new_val: (U256, bool), old_val: (U256, bool)) -> (U256, bool) {
    let mut acc = old_val.0;
    let mut sign = old_val.1;
    if new_val.1 == sign {
        acc += new_val.0;
    } else if sign {
        if new_val.0 >= acc {
            sign = false;
            acc = new_val.0 - acc;
        } else {
            acc -= new_val.0;
        }
    } else if acc >= new_val.0 {
        acc -= new_val.0;
    } else {
        sign = true;
        acc = new_val.0 - acc;
    }
    (acc, sign)
}

/// helper struct that provides sorting tokens based on a given net vol
#[derive(Debug, Clone)]
struct TokenBasedVol<'a> {
    token: &'a SgErc20,
    net_vol: Float,
    is_neg: bool,
}

impl<'a> Ord for TokenBasedVol<'a> {
    fn clamp(self, _min: Self, _max: Self) -> Self
    where
        Self: Sized,
        Self: PartialOrd,
    {
        self
    }
    fn cmp(&self, other: &Self) -> Ordering {
        if self.is_neg == other.is_neg {
            match self.net_vol.cmp(&other.net_vol) {
                Ordering::Greater => {
                    if self.is_neg {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                }
                Ordering::Less => {
                    if self.is_neg {
                        Ordering::Greater
                    } else {
                        Ordering::Less
                    }
                }
                Ordering::Equal => Ordering::Equal,
            }
        } else if self.is_neg {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }
    fn min(self, other: Self) -> Self
    where
        Self: Sized,
    {
        match self.cmp(&other) {
            Ordering::Greater => other,
            Ordering::Less => self,
            Ordering::Equal => self,
        }
    }
    fn max(self, other: Self) -> Self
    where
        Self: Sized,
    {
        match self.cmp(&other) {
            Ordering::Greater => self,
            Ordering::Less => other,
            Ordering::Equal => self,
        }
    }
}
impl<'a> PartialOrd for TokenBasedVol<'a> {
    fn ge(&self, other: &Self) -> bool {
        !matches!(self.cmp(other), Ordering::Less)
    }
    fn le(&self, other: &Self) -> bool {
        !matches!(self.cmp(other), Ordering::Greater)
    }
    fn gt(&self, other: &Self) -> bool {
        matches!(self.cmp(other), Ordering::Greater)
    }
    fn lt(&self, other: &Self) -> bool {
        matches!(self.cmp(other), Ordering::Less)
    }
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod test {
    use super::super::*;
    use super::*;
    use crate::types::common::{
        SgBigInt, SgBytes, SgOrder, SgRaindex, SgTradeEvent, SgTradeStructPartialOrder,
        SgTradeVaultBalanceChange, SgTransaction, SgVault, SgVaultBalanceChangeVault,
    };
    use alloy::primitives::{Address, B256};
    use std::str::FromStr;

    #[test]
    fn test_token_based_vol_ord_parial_ord() {
        let token_address = Address::random();
        let token = SgErc20 {
            id: SgBytes(token_address.to_string()),
            address: SgBytes(token_address.to_string()),
            name: Some("Token".to_string()),
            symbol: Some("Token".to_string()),
            decimals: Some(SgBigInt(6.to_string())),
        };

        // positive == positive
        let a = TokenBasedVol {
            token: &token,
            net_vol: U256::from(1),
            is_neg: false,
        };
        let b = TokenBasedVol {
            token: &token,
            net_vol: U256::from(1),
            is_neg: false,
        };
        assert!(matches!(a.cmp(&b), Ordering::Equal));
        assert!(matches!(a.partial_cmp(&b), Some(Ordering::Equal)));
        assert_eq!(a.clone().min(b.clone()), a);
        assert_eq!(a.clone().max(b.clone()), a);
        assert!(!a.gt(&b));
        assert!(a.ge(&b));
        assert!(a.le(&b));
        assert!(!a.lt(&b));

        // negative == negative
        let a = TokenBasedVol {
            token: &token,
            net_vol: U256::from(1),
            is_neg: true,
        };
        let b = TokenBasedVol {
            token: &token,
            net_vol: U256::from(1),
            is_neg: true,
        };
        assert!(matches!(a.cmp(&b), Ordering::Equal));
        assert!(matches!(a.partial_cmp(&b), Some(Ordering::Equal)));
        assert_eq!(a.clone().min(b.clone()), a);
        assert_eq!(a.clone().max(b.clone()), a);
        assert!(!a.gt(&b));
        assert!(a.ge(&b));
        assert!(a.le(&b));
        assert!(!a.lt(&b));

        // positive > positive
        let a = TokenBasedVol {
            token: &token,
            net_vol: U256::from(2),
            is_neg: false,
        };
        let b = TokenBasedVol {
            token: &token,
            net_vol: U256::from(1),
            is_neg: false,
        };
        assert!(matches!(a.cmp(&b), Ordering::Greater));
        assert!(matches!(a.partial_cmp(&b), Some(Ordering::Greater)));
        assert_eq!(a.clone().min(b.clone()), b);
        assert_eq!(a.clone().max(b.clone()), a);
        assert!(a.gt(&b));
        assert!(a.ge(&b));
        assert!(!a.le(&b));
        assert!(!a.lt(&b));

        // positive < positive
        let a = TokenBasedVol {
            token: &token,
            net_vol: U256::from(1),
            is_neg: false,
        };
        let b = TokenBasedVol {
            token: &token,
            net_vol: U256::from(2),
            is_neg: false,
        };
        assert!(matches!(a.cmp(&b), Ordering::Less));
        assert!(matches!(a.partial_cmp(&b), Some(Ordering::Less)));
        assert_eq!(a.clone().min(b.clone()), a);
        assert_eq!(a.clone().max(b.clone()), b);
        assert!(!a.gt(&b));
        assert!(!a.ge(&b));
        assert!(a.le(&b));
        assert!(a.lt(&b));

        // negative > negative
        let a = TokenBasedVol {
            token: &token,
            net_vol: U256::from(1),
            is_neg: true,
        };
        let b = TokenBasedVol {
            token: &token,
            net_vol: U256::from(2),
            is_neg: true,
        };
        assert!(matches!(a.cmp(&b), Ordering::Greater));
        assert!(matches!(a.partial_cmp(&b), Some(Ordering::Greater)));
        assert_eq!(a.clone().min(b.clone()), b);
        assert_eq!(a.clone().max(b.clone()), a);
        assert!(a.gt(&b));
        assert!(a.ge(&b));
        assert!(!a.le(&b));
        assert!(!a.lt(&b));

        // negative < negative
        let a = TokenBasedVol {
            token: &token,
            net_vol: U256::from(2),
            is_neg: true,
        };
        let b = TokenBasedVol {
            token: &token,
            net_vol: U256::from(1),
            is_neg: true,
        };
        assert!(matches!(a.cmp(&b), Ordering::Less));
        assert!(matches!(a.partial_cmp(&b), Some(Ordering::Less)));
        assert_eq!(a.clone().min(b.clone()), a);
        assert_eq!(a.clone().max(b.clone()), b);
        assert!(!a.gt(&b));
        assert!(!a.ge(&b));
        assert!(a.le(&b));
        assert!(a.lt(&b));
    }

    #[test]
    fn test_accumulate() {
        // both positive
        let new_val = (U256::from(1), false);
        let old_val = (U256::from(2), false);
        let result = accumulate(new_val, old_val);
        let expected = (U256::from(3), false);
        assert_eq!(result, expected);

        // both negative
        let new_val = (U256::from(1), true);
        let old_val = (U256::from(2), true);
        let result = accumulate(new_val, old_val);
        let expected = (U256::from(3), true);
        assert_eq!(result, expected);

        // negative < positive
        let new_val = (U256::from(1), true);
        let old_val = (U256::from(2), false);
        let result = accumulate(new_val, old_val);
        let expected = (U256::from(1), false);
        assert_eq!(result, expected);

        // negative > positive
        let new_val = (U256::from(2), true);
        let old_val = (U256::from(1), false);
        let result = accumulate(new_val, old_val);
        let expected = (U256::from(1), true);
        assert_eq!(result, expected);

        // positive < negative
        let new_val = (U256::from(1), false);
        let old_val = (U256::from(2), true);
        let result = accumulate(new_val, old_val);
        let expected = (U256::from(1), true);
        assert_eq!(result, expected);

        // positive > negative
        let new_val = (U256::from(2), false);
        let old_val = (U256::from(1), true);
        let result = accumulate(new_val, old_val);
        let expected = (U256::from(1), false);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_get_pairs_ratio() {
        let trades = get_trades();
        let [token1, token2] = get_tokens();
        let result = get_order_pairs_ratio(&get_order(), &trades);
        let mut expected = HashMap::new();
        expected.insert(
            TokenPair {
                input: token2.clone(),
                output: token1.clone(),
            },
            Some(U256::from_str("285714285714285714").unwrap()),
        );
        expected.insert(
            TokenPair {
                input: token1.clone(),
                output: token2.clone(),
            },
            Some(U256::from_str("3500000000000000000").unwrap()),
        );

        assert_eq!(result, expected);
    }

    #[test]
    fn test_get_pairs_ratio_unhappy() {
        let mut trades = get_trades();
        // set some corrupt value
        trades[0].input_vault_balance_change.amount = SgBytes("abcd".to_string());
        let [token1, token2] = get_tokens();
        let result = get_order_pairs_ratio(&get_order(), &trades);
        let mut expected = HashMap::new();
        expected.insert(
            TokenPair {
                input: token2.clone(),
                output: token1.clone(),
            },
            None,
        );
        expected.insert(
            TokenPair {
                input: token1.clone(),
                output: token2.clone(),
            },
            None,
        );

        assert_eq!(result, expected);
    }

    #[test]
    fn test_measure_order_performance() {
        let order = get_order();
        let trades = get_trades();
        let [token1, token2] = get_tokens();
        let [vault1, vault2] = get_vault_ids();

        let token1_perf = VaultPerformance {
            id: vault1.to_string(),
            token: token1.clone(),
            apy_details: Some(APYDetails {
                start_time: 1,
                end_time: 10000001,
                net_vol: F5,
                capital: F5,
                apy: Some(Float::parse("3.1536".to_string()).unwrap()),
                is_neg: false,
            }),
            vol_details: VolumeDetails {
                net_vol: U256::from_str("5000000000000000000").unwrap(),
                total_in: U256::from_str("7000000000000000000").unwrap(),
                total_out: U256::from_str("2000000000000000000").unwrap(),
                total_vol: U256::from_str("9000000000000000000").unwrap(),
            },
        };
        let token2_perf = VaultPerformance {
            id: vault2.to_string(),
            token: token2.clone(),
            apy_details: Some(APYDetails {
                start_time: 1,
                end_time: 10000001,
                net_vol: U256::from_str("3000000000000000000").unwrap(),
                capital: U256::from_str("5000000000000000000").unwrap(),
                apy: Some(U256::from_str("1892160000000000000").unwrap()),
                is_neg: false,
            }),
            vol_details: VolumeDetails {
                net_vol: U256::from_str("3000000000000000000").unwrap(),
                total_in: U256::from_str("5000000000000000000").unwrap(),
                total_out: U256::from_str("2000000000000000000").unwrap(),
                total_vol: U256::from_str("7000000000000000000").unwrap(),
            },
        };
        let result = OrderPerformance::measure(&order, &trades, Some(1), Some(10000001)).unwrap();
        let expected = OrderPerformance {
            order_id: "order-id".to_string(),
            order_hash: "".to_string(),
            raindex: "".to_string(),
            start_time: 1,
            end_time: 10000001,
            inputs_vaults: vec![token1_perf.clone(), token2_perf.clone()],
            outputs_vaults: vec![token1_perf.clone(), token2_perf.clone()],
            denominated_performance: Some(DenominatedPerformance {
                apy: U256::from_str("2172479999999999999").unwrap(),
                apy_is_neg: false,
                token: token2,
                net_vol: U256::from_str("4428571428571428570").unwrap(),
                starting_capital: U256::from_str("6428571428571428570").unwrap(),
                net_vol_is_neg: false,
            }),
        };

        assert_eq!(result, expected);
    }

    fn get_vault_ids() -> [B256; 2] {
        [
            B256::from_slice(&[0x11u8; 32]),
            B256::from_slice(&[0x22u8; 32]),
        ]
    }

    const DECIMALS: u8 = 18;

    fn get_tokens() -> [SgErc20; 2] {
        let token1_address = Address::from_slice(&[0x11u8; 20]);
        let token2_address = Address::from_slice(&[0x22u8; 20]);
        let token1 = SgErc20 {
            id: SgBytes(token1_address.to_string()),
            address: SgBytes(token1_address.to_string()),
            name: Some("Token1".to_string()),
            symbol: Some("Token1".to_string()),
            decimals: Some(SgBigInt(DECIMALS.to_string())),
        };
        let token2 = SgErc20 {
            id: SgBytes(token2_address.to_string()),
            address: SgBytes(token2_address.to_string()),
            name: Some("Token2".to_string()),
            symbol: Some("Token2".to_string()),
            decimals: Some(SgBigInt(DECIMALS.to_string())),
        };
        [token1, token2]
    }
    fn get_order() -> SgOrder {
        let [vault_id1, vault_id2] = get_vault_ids();
        let [token1, token2] = get_tokens();
        let vault1 = SgVault {
            id: SgBytes("".to_string()),
            owner: SgBytes("".to_string()),
            vault_id: SgBytes(vault_id1.to_string()),
            balance: SgBytes("".to_string()),
            token: token1,
            raindex: SgRaindex {
                id: SgBytes("".to_string()),
            },
            orders_as_output: vec![],
            orders_as_input: vec![],
            balance_changes: vec![],
        };
        let vault2 = SgVault {
            id: SgBytes("".to_string()),
            owner: SgBytes("".to_string()),
            vault_id: SgBytes(vault_id2.to_string()),
            balance: SgBytes("".to_string()),
            token: token2,
            raindex: SgRaindex {
                id: SgBytes("".to_string()),
            },
            orders_as_output: vec![],
            orders_as_input: vec![],
            balance_changes: vec![],
        };
        SgOrder {
            id: SgBytes("order-id".to_string()),
            order_bytes: SgBytes("".to_string()),
            order_hash: SgBytes("".to_string()),
            owner: SgBytes("".to_string()),
            outputs: vec![vault1.clone(), vault2.clone()],
            inputs: vec![vault1, vault2],
            raindex: SgRaindex {
                id: SgBytes("".to_string()),
            },
            active: true,
            timestamp_added: SgBytes("".to_string()),
            meta: None,
            add_events: vec![],
            trades: vec![],
            remove_events: vec![],
        }
    }

    fn get_trades() -> Vec<SgTrade> {
        let bytes = SgBytes("".to_string());
        let bigint = SgBigInt("".to_string());
        let [vault_id1, vault_id2] = get_vault_ids();
        let [token1, token2] = get_tokens();

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
                amount: SgBytes(F5.as_hex()),
                new_vault_balance: SgBytes(F2.as_hex()),
                old_vault_balance: bytes.clone(),
                vault: SgVaultBalanceChangeVault {
                    id: bytes.clone(),
                    token: token1.clone(),
                    vault_id: SgBytes(vault_id1.to_string()),
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
                amount: SgBytes(F5.as_hex()),
                new_vault_balance: SgBytes(F2.as_hex()),
                old_vault_balance: bytes.clone(),
                vault: SgVaultBalanceChangeVault {
                    id: bytes.clone(),
                    token: token2.clone(),
                    vault_id: SgBytes(vault_id2.to_string()),
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
                amount: SgBytes(NEG2.as_hex()),
                new_vault_balance: SgBytes(F5.as_hex()),
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
                amount: SgBytes(F7.as_hex()),
                new_vault_balance: SgBytes(F5.as_hex()),
                old_vault_balance: bytes.clone(),
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
