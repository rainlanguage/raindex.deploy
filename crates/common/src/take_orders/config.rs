use super::simulation::SimulationResult;
use crate::raindex_client::RaindexError;
use alloy::primitives::{Bytes, U256};
use rain_math_float::Float;
use raindex_bindings::IRaindexV7::{TakeOrderConfigV5, TakeOrdersConfigV6};
use serde::{Deserialize, Serialize};
use wasm_bindgen_utils::{impl_wasm_traits, prelude::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum TakeOrdersMode {
    BuyExact,
    BuyUpTo,
    SpendExact,
    SpendUpTo,
}
impl_wasm_traits!(TakeOrdersMode);

#[derive(Clone, Copy, Debug)]
pub struct ParsedTakeOrdersMode {
    pub mode: TakeOrdersMode,
    pub amount: Float,
}

impl ParsedTakeOrdersMode {
    pub fn parse(mode: TakeOrdersMode, amount: &str) -> Result<Self, RaindexError> {
        let zero = Float::zero()?;
        let amount = Float::parse(amount.to_string())?;
        if amount.lte(zero)? {
            return Err(RaindexError::NonPositiveAmount);
        }
        Ok(ParsedTakeOrdersMode { mode, amount })
    }

    pub fn is_buy_mode(&self) -> bool {
        matches!(
            self.mode,
            TakeOrdersMode::BuyExact | TakeOrdersMode::BuyUpTo
        )
    }

    pub fn is_exact_mode(&self) -> bool {
        matches!(
            self.mode,
            TakeOrdersMode::BuyExact | TakeOrdersMode::SpendExact
        )
    }

    pub fn target_amount(&self) -> Float {
        self.amount
    }
}

#[derive(Clone, Debug)]
pub struct BuiltTakeOrdersConfig {
    pub config: TakeOrdersConfigV6,
    pub sim: SimulationResult,
}

pub fn build_take_orders_config_from_simulation(
    sim: SimulationResult,
    mode: ParsedTakeOrdersMode,
    price_cap: Float,
) -> Result<Option<BuiltTakeOrdersConfig>, RaindexError> {
    if sim.legs.is_empty() {
        return Ok(None);
    }

    let target = mode.target_amount();

    if mode.is_exact_mode() {
        let achieved = if mode.is_buy_mode() {
            sim.total_output
        } else {
            sim.total_input
        };
        if achieved.lt(target)? {
            return Err(RaindexError::InsufficientLiquidity {
                requested: target.format()?,
                available: achieved.format()?,
            });
        }
    }

    let zero = Float::zero()?;

    let orders: Vec<TakeOrderConfigV5> = sim
        .legs
        .iter()
        .map(|leg| TakeOrderConfigV5 {
            order: leg.candidate.order.clone(),
            inputIOIndex: U256::from(leg.candidate.input_io_index),
            outputIOIndex: U256::from(leg.candidate.output_io_index),
            signedContext: leg.candidate.signed_context.clone(),
        })
        .collect();

    let io_is_input = mode.is_buy_mode();

    let maximum_io = target;
    let minimum_io = if mode.is_exact_mode() { target } else { zero };
    let maximum_io_ratio = price_cap;

    let config = TakeOrdersConfigV6 {
        minimumIO: minimum_io.get_inner(),
        maximumIO: maximum_io.get_inner(),
        maximumIORatio: maximum_io_ratio.get_inner(),
        IOIsInput: io_is_input,
        orders,
        data: Bytes::new(),
    };

    Ok(Some(BuiltTakeOrdersConfig { config, sim }))
}

#[cfg(test)]
#[cfg(not(target_family = "wasm"))]
mod tests {
    use super::*;
    use crate::take_orders::simulation::{
        simulate_buy_over_candidates, simulate_spend_over_candidates, SelectedTakeOrderLeg,
    };
    use crate::test_helpers::candidates::make_simulation_candidate;
    use rain_math_float::Float;

    fn high_price_cap() -> Float {
        Float::parse("1000000".to_string()).unwrap()
    }

    fn buy_up_to(amount: Float) -> ParsedTakeOrdersMode {
        ParsedTakeOrdersMode {
            mode: TakeOrdersMode::BuyUpTo,
            amount,
        }
    }

    fn buy_exact(amount: Float) -> ParsedTakeOrdersMode {
        ParsedTakeOrdersMode {
            mode: TakeOrdersMode::BuyExact,
            amount,
        }
    }

    fn spend_up_to(amount: Float) -> ParsedTakeOrdersMode {
        ParsedTakeOrdersMode {
            mode: TakeOrdersMode::SpendUpTo,
            amount,
        }
    }

    fn spend_exact(amount: Float) -> ParsedTakeOrdersMode {
        ParsedTakeOrdersMode {
            mode: TakeOrdersMode::SpendExact,
            amount,
        }
    }

    #[test]
    fn test_build_config_empty_simulation() {
        let sim = SimulationResult {
            legs: vec![],
            total_input: Float::zero().unwrap(),
            total_output: Float::zero().unwrap(),
        };
        let buy_target = Float::parse("100".to_string()).unwrap();
        let price_cap = high_price_cap();
        let mode = buy_up_to(buy_target);

        let result = build_take_orders_config_from_simulation(sim, mode, price_cap).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_build_config_buy_up_to_mode() {
        let f1_75 = Float::parse("1.75".to_string()).unwrap();
        let f2_5 = Float::parse("2.5".to_string()).unwrap();
        let candidate = make_simulation_candidate(f2_5, f1_75);
        let sim = SimulationResult {
            legs: vec![SelectedTakeOrderLeg {
                candidate,
                output: f2_5,
                input: f2_5,
            }],
            total_output: f2_5,
            total_input: f2_5,
        };
        let buy_target = f2_5;
        let price_cap = Float::parse("3".to_string()).unwrap();
        let mode = buy_up_to(buy_target);

        let result = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        assert_eq!(result.config.minimumIO, Float::zero().unwrap().get_inner());
        assert_eq!(result.config.maximumIO, buy_target.get_inner());
        assert_eq!(result.config.maximumIORatio, price_cap.get_inner());
        assert!(result.config.IOIsInput);
        assert_eq!(result.config.orders.len(), 1);
    }

    #[test]
    fn test_build_config_buy_exact_mode() {
        let f1_25 = Float::parse("1.25".to_string()).unwrap();
        let f2_75 = Float::parse("2.75".to_string()).unwrap();
        let candidate = make_simulation_candidate(f2_75, f1_25);
        let sim = SimulationResult {
            legs: vec![SelectedTakeOrderLeg {
                candidate,
                output: f2_75,
                input: f2_75,
            }],
            total_output: f2_75,
            total_input: f2_75,
        };
        let buy_target = f2_75;
        let price_cap = Float::parse("2".to_string()).unwrap();
        let mode = buy_exact(buy_target);

        let result = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        assert_eq!(result.config.minimumIO, buy_target.get_inner());
        assert_eq!(result.config.maximumIO, buy_target.get_inner());
        assert_eq!(result.config.maximumIORatio, price_cap.get_inner());
        assert!(result.config.IOIsInput);
    }

    #[test]
    fn test_build_config_buy_exact_insufficient_liquidity() {
        let ratio = Float::parse("1".to_string()).unwrap();
        let available = Float::parse("50".to_string()).unwrap();
        let candidate = make_simulation_candidate(available, ratio);
        let sim = SimulationResult {
            legs: vec![SelectedTakeOrderLeg {
                candidate,
                output: available,
                input: available,
            }],
            total_output: available,
            total_input: available,
        };
        let buy_target = Float::parse("100".to_string()).unwrap();
        let price_cap = high_price_cap();
        let mode = buy_exact(buy_target);

        let result = build_take_orders_config_from_simulation(sim, mode, price_cap);

        assert!(matches!(
            result,
            Err(RaindexError::InsufficientLiquidity { .. })
        ));
    }

    #[test]
    fn test_build_config_buy_up_to_allows_underfill() {
        let ratio = Float::parse("1".to_string()).unwrap();
        let available = Float::parse("50".to_string()).unwrap();
        let candidate = make_simulation_candidate(available, ratio);
        let sim = SimulationResult {
            legs: vec![SelectedTakeOrderLeg {
                candidate,
                output: available,
                input: available,
            }],
            total_output: available,
            total_input: available,
        };
        let buy_target = Float::parse("100".to_string()).unwrap();
        let price_cap = high_price_cap();
        let mode = buy_up_to(buy_target);

        let result = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        assert_eq!(result.config.maximumIO, buy_target.get_inner());
        assert_eq!(result.config.minimumIO, Float::zero().unwrap().get_inner());
    }

    #[test]
    fn test_build_config_uses_price_cap_not_worst_price() {
        let f1_5 = Float::parse("1.5".to_string()).unwrap();
        let f2_75 = Float::parse("2.75".to_string()).unwrap();
        let f3_25 = Float::parse("3.25".to_string()).unwrap();
        let cheap_candidate = make_simulation_candidate(f1_5, f1_5);
        let expensive_candidate = make_simulation_candidate(f1_5, f2_75);
        let sim = SimulationResult {
            legs: vec![
                SelectedTakeOrderLeg {
                    candidate: cheap_candidate,
                    output: f1_5,
                    input: f1_5,
                },
                SelectedTakeOrderLeg {
                    candidate: expensive_candidate,
                    output: f1_5,
                    input: f2_75,
                },
            ],
            total_output: f3_25,
            total_input: Float::parse("4.25".to_string()).unwrap(),
        };
        let buy_target = f3_25;
        let price_cap = Float::parse("5".to_string()).unwrap();
        let mode = buy_up_to(buy_target);

        let result = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        assert_eq!(
            result.config.maximumIORatio,
            price_cap.get_inner(),
            "Should use price_cap, not worst price from simulation"
        );
        assert_eq!(result.config.orders.len(), 2);
    }

    #[test]
    fn test_build_config_preserves_order() {
        let f1_25 = Float::parse("1.25".to_string()).unwrap();
        let f2_5 = Float::parse("2.5".to_string()).unwrap();
        let f3_75 = Float::parse("3.75".to_string()).unwrap();
        let candidate1 = make_simulation_candidate(f1_25, f1_25);
        let candidate2 = make_simulation_candidate(f1_25, f2_5);
        let sim = SimulationResult {
            legs: vec![
                SelectedTakeOrderLeg {
                    candidate: candidate1.clone(),
                    output: f1_25,
                    input: f1_25,
                },
                SelectedTakeOrderLeg {
                    candidate: candidate2.clone(),
                    output: f1_25,
                    input: f2_5,
                },
            ],
            total_output: f2_5,
            total_input: f3_75,
        };
        let buy_target = f2_5;
        let price_cap = high_price_cap();
        let mode = buy_up_to(buy_target);

        let result = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        assert_eq!(result.config.orders.len(), 2);
        assert_eq!(
            result.config.orders[0].inputIOIndex,
            alloy::primitives::U256::from(0)
        );
        assert_eq!(
            result.config.orders[1].inputIOIndex,
            alloy::primitives::U256::from(0)
        );
    }

    #[test]
    fn test_simulate_and_build_config_buy_mode_integration() {
        let ratio_1 = Float::parse("1".to_string()).unwrap();
        let ratio_2 = Float::parse("2".to_string()).unwrap();
        let ratio_3 = Float::parse("3".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let candidate_cheap = make_simulation_candidate(max_output, ratio_1);
        let candidate_mid = make_simulation_candidate(max_output, ratio_2);
        let candidate_expensive = make_simulation_candidate(max_output, ratio_3);

        let candidates = vec![candidate_expensive, candidate_mid, candidate_cheap];
        let buy_target = Float::parse("125".to_string()).unwrap();
        let price_cap = Float::parse("3".to_string()).unwrap();
        let mode = buy_up_to(buy_target);

        let sim = simulate_buy_over_candidates(candidates, buy_target, price_cap).unwrap();

        let built = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        assert_eq!(
            built.config.maximumIORatio,
            price_cap.get_inner(),
            "maximumIORatio should be price_cap"
        );
        assert_eq!(
            built.config.orders.len(),
            2,
            "orders length should match legs length"
        );
        assert_eq!(
            built.config.maximumIO,
            buy_target.get_inner(),
            "maximumIO should equal buy_target"
        );
        assert!(
            built.config.IOIsInput,
            "IOIsInput should be true for buy mode"
        );
    }

    #[test]
    fn test_build_config_spend_up_to_mode() {
        let f2 = Float::parse("2".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();
        let candidate = make_simulation_candidate(max_output, f2);
        let spend_budget = Float::parse("20".to_string()).unwrap();
        let expected_output = Float::parse("10".to_string()).unwrap();
        let sim = SimulationResult {
            legs: vec![SelectedTakeOrderLeg {
                candidate,
                input: spend_budget,
                output: expected_output,
            }],
            total_input: spend_budget,
            total_output: expected_output,
        };
        let price_cap = Float::parse("3".to_string()).unwrap();
        let mode = spend_up_to(spend_budget);

        let result = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        assert_eq!(result.config.minimumIO, Float::zero().unwrap().get_inner());
        assert_eq!(result.config.maximumIO, spend_budget.get_inner());
        assert_eq!(result.config.maximumIORatio, price_cap.get_inner());
        assert!(!result.config.IOIsInput);
        assert_eq!(result.config.orders.len(), 1);
    }

    #[test]
    fn test_build_config_spend_exact_mode() {
        let f2 = Float::parse("2".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();
        let candidate = make_simulation_candidate(max_output, f2);
        let spend_budget = Float::parse("20".to_string()).unwrap();
        let expected_output = Float::parse("10".to_string()).unwrap();
        let sim = SimulationResult {
            legs: vec![SelectedTakeOrderLeg {
                candidate,
                input: spend_budget,
                output: expected_output,
            }],
            total_input: spend_budget,
            total_output: expected_output,
        };
        let price_cap = Float::parse("3".to_string()).unwrap();
        let mode = spend_exact(spend_budget);

        let result = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        assert_eq!(result.config.minimumIO, spend_budget.get_inner());
        assert_eq!(result.config.maximumIO, spend_budget.get_inner());
        assert_eq!(result.config.maximumIORatio, price_cap.get_inner());
        assert!(!result.config.IOIsInput);
    }

    #[test]
    fn test_build_config_spend_exact_insufficient_liquidity() {
        let ratio = Float::parse("2".to_string()).unwrap();
        let max_output = Float::parse("25".to_string()).unwrap();
        let candidate = make_simulation_candidate(max_output, ratio);
        let available_input = Float::parse("50".to_string()).unwrap();
        let sim = SimulationResult {
            legs: vec![SelectedTakeOrderLeg {
                candidate,
                input: available_input,
                output: max_output,
            }],
            total_input: available_input,
            total_output: max_output,
        };
        let spend_target = Float::parse("100".to_string()).unwrap();
        let price_cap = high_price_cap();
        let mode = spend_exact(spend_target);

        let result = build_take_orders_config_from_simulation(sim, mode, price_cap);

        assert!(matches!(
            result,
            Err(RaindexError::InsufficientLiquidity { .. })
        ));
    }

    #[test]
    fn test_build_config_spend_up_to_allows_underfill() {
        let ratio = Float::parse("2".to_string()).unwrap();
        let max_output = Float::parse("25".to_string()).unwrap();
        let candidate = make_simulation_candidate(max_output, ratio);
        let available_input = Float::parse("50".to_string()).unwrap();
        let sim = SimulationResult {
            legs: vec![SelectedTakeOrderLeg {
                candidate,
                input: available_input,
                output: max_output,
            }],
            total_input: available_input,
            total_output: max_output,
        };
        let spend_target = Float::parse("100".to_string()).unwrap();
        let price_cap = high_price_cap();
        let mode = spend_up_to(spend_target);

        let result = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        assert_eq!(result.config.maximumIO, spend_target.get_inner());
        assert_eq!(result.config.minimumIO, Float::zero().unwrap().get_inner());
        assert!(!result.config.IOIsInput);
    }

    #[test]
    fn test_simulate_and_build_config_spend_mode_integration() {
        let ratio_1 = Float::parse("1".to_string()).unwrap();
        let ratio_2 = Float::parse("2".to_string()).unwrap();
        let ratio_3 = Float::parse("3".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let candidate_cheap = make_simulation_candidate(max_output, ratio_1);
        let candidate_mid = make_simulation_candidate(max_output, ratio_2);
        let candidate_expensive = make_simulation_candidate(max_output, ratio_3);

        let candidates = vec![candidate_expensive, candidate_mid, candidate_cheap];
        let spend_budget = Float::parse("150".to_string()).unwrap();
        let price_cap = Float::parse("3".to_string()).unwrap();
        let mode = spend_up_to(spend_budget);

        let sim = simulate_spend_over_candidates(candidates, spend_budget, price_cap).unwrap();

        let built = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        assert_eq!(
            built.config.maximumIORatio,
            price_cap.get_inner(),
            "maximumIORatio should be price_cap"
        );
        assert_eq!(
            built.config.maximumIO,
            spend_budget.get_inner(),
            "maximumIO should equal spend_budget"
        );
        assert!(
            !built.config.IOIsInput,
            "IOIsInput should be false for spend mode"
        );
    }

    #[test]
    fn test_parsed_take_orders_mode_is_buy_mode() {
        let amount = Float::parse("10".to_string()).unwrap();
        let buy_exact = buy_exact(amount);
        let buy_up_to = buy_up_to(amount);
        let spend_exact = spend_exact(amount);
        let spend_up_to = spend_up_to(amount);

        assert!(buy_exact.is_buy_mode());
        assert!(buy_up_to.is_buy_mode());
        assert!(!spend_exact.is_buy_mode());
        assert!(!spend_up_to.is_buy_mode());
    }

    #[test]
    fn test_parsed_take_orders_mode_is_exact_mode() {
        let amount = Float::parse("10".to_string()).unwrap();
        let buy_exact = buy_exact(amount);
        let buy_up_to = buy_up_to(amount);
        let spend_exact = spend_exact(amount);
        let spend_up_to = spend_up_to(amount);

        assert!(buy_exact.is_exact_mode());
        assert!(!buy_up_to.is_exact_mode());
        assert!(spend_exact.is_exact_mode());
        assert!(!spend_up_to.is_exact_mode());
    }

    #[test]
    fn test_parsed_take_orders_mode_target_amount() {
        let amount = Float::parse("42".to_string()).unwrap();
        let m_buy_exact = buy_exact(amount);
        let m_buy_up_to = buy_up_to(amount);
        let m_spend_exact = spend_exact(amount);
        let m_spend_up_to = spend_up_to(amount);

        assert!(m_buy_exact.target_amount().eq(amount).unwrap());
        assert!(m_buy_up_to.target_amount().eq(amount).unwrap());
        assert!(m_spend_exact.target_amount().eq(amount).unwrap());
        assert!(m_spend_up_to.target_amount().eq(amount).unwrap());
    }

    #[test]
    fn test_take_orders_mode_parse_valid() {
        assert!(ParsedTakeOrdersMode::parse(TakeOrdersMode::BuyExact, "100").is_ok());
        assert!(ParsedTakeOrdersMode::parse(TakeOrdersMode::BuyUpTo, "200").is_ok());
        assert!(ParsedTakeOrdersMode::parse(TakeOrdersMode::SpendExact, "300").is_ok());
        assert!(ParsedTakeOrdersMode::parse(TakeOrdersMode::SpendUpTo, "400").is_ok());
    }

    #[test]
    fn test_take_orders_mode_parse_zero_amount() {
        let result = ParsedTakeOrdersMode::parse(TakeOrdersMode::BuyExact, "0");
        assert!(matches!(result, Err(RaindexError::NonPositiveAmount)));
    }

    #[test]
    fn test_take_orders_mode_parse_negative_amount() {
        let result = ParsedTakeOrdersMode::parse(TakeOrdersMode::SpendUpTo, "-10");
        assert!(matches!(result, Err(RaindexError::NonPositiveAmount)));
    }

    #[test]
    fn test_take_orders_mode_parse_invalid_amount() {
        let result = ParsedTakeOrdersMode::parse(TakeOrdersMode::BuyUpTo, "not-a-number");
        assert!(matches!(result, Err(RaindexError::Float(_))));
    }
}
