use super::candidates::TakeOrderCandidate;
use super::config::ParsedTakeOrdersMode;
use crate::raindex_client::RaindexError;
use crate::utils::float::cmp_float;
use crate::utils::timing::Timing;
use alloy::primitives::{keccak256, B256};
use alloy::sol_types::SolValue;
use rain_math_float::Float;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::ops::{Add, Div, Mul, Sub};
use tracing::{debug, info, Level};

const MAX_INFO_PRICE_CAP_REJECTION_LOGS: usize = 100;

fn format_float(value: Float) -> String {
    value
        .format()
        .unwrap_or_else(|_| "<format_error>".to_string())
}

fn order_hash(candidate: &TakeOrderCandidate) -> B256 {
    B256::from(keccak256(candidate.order.abi_encode()))
}

#[derive(Clone, Debug)]
pub struct SelectedTakeOrderLeg {
    pub candidate: TakeOrderCandidate,
    pub input: Float,
    pub output: Float,
}

#[derive(Clone, Debug)]
pub struct SimulationResult {
    pub legs: Vec<SelectedTakeOrderLeg>,
    pub total_input: Float,
    pub total_output: Float,
}

pub fn simulate_candidates(
    candidates: Vec<TakeOrderCandidate>,
    mode: ParsedTakeOrdersMode,
    price_cap: Float,
) -> Result<SimulationResult, RaindexError> {
    if mode.is_buy_mode() {
        simulate_buy_over_candidates(candidates, mode.target_amount(), price_cap)
    } else {
        simulate_spend_over_candidates(candidates, mode.target_amount(), price_cap)
    }
}

fn sort_candidates_by_price(candidates: &mut [TakeOrderCandidate]) -> Result<(), RaindexError> {
    let comparison_error: RefCell<Option<RaindexError>> = RefCell::new(None);
    candidates.sort_by(|a, b| {
        if comparison_error.borrow().is_some() {
            return Ordering::Equal;
        }
        match cmp_float(&a.ratio, &b.ratio) {
            Ok(ord) => ord,
            Err(e) => {
                *comparison_error.borrow_mut() = Some(e.into());
                Ordering::Equal
            }
        }
    });
    if let Some(e) = comparison_error.into_inner() {
        return Err(e);
    }
    Ok(())
}

fn take_leg_for_buy(
    candidate: TakeOrderCandidate,
    remaining_output: Float,
) -> Result<Option<SelectedTakeOrderLeg>, RaindexError> {
    let zero = Float::zero()?;
    let price = candidate.ratio;
    let max_output = candidate.max_output;

    let output = if max_output.lte(remaining_output)? {
        max_output
    } else {
        remaining_output
    };

    if output.lte(zero)? {
        return Ok(None);
    }

    let input = output.mul(price)?;

    Ok(Some(SelectedTakeOrderLeg {
        candidate,
        input,
        output,
    }))
}

fn take_leg_for_spend(
    candidate: TakeOrderCandidate,
    remaining_input: Float,
) -> Result<Option<SelectedTakeOrderLeg>, RaindexError> {
    let zero = Float::zero()?;
    let price = candidate.ratio;
    let max_output = candidate.max_output;

    if price.eq(zero)? {
        if max_output.lte(zero)? {
            return Ok(None);
        }
        return Ok(Some(SelectedTakeOrderLeg {
            candidate,
            input: zero,
            output: max_output,
        }));
    }

    let max_input_for_candidate = max_output.mul(price)?;

    let input = if max_input_for_candidate.lte(remaining_input)? {
        max_input_for_candidate
    } else {
        remaining_input
    };

    if input.lte(zero)? {
        return Ok(None);
    }

    let output = input.div(price)?;

    Ok(Some(SelectedTakeOrderLeg {
        candidate,
        input,
        output,
    }))
}

struct BuySimulationTotals {
    remaining_output: Float,
    total_input: Float,
    total_output: Float,
}

struct SpendSimulationTotals {
    remaining_input: Float,
    total_input: Float,
    total_output: Float,
}

fn apply_leg_for_buy(
    leg: &SelectedTakeOrderLeg,
    totals: &mut BuySimulationTotals,
) -> Result<(), RaindexError> {
    totals.remaining_output = totals.remaining_output.sub(leg.output)?;
    totals.total_input = totals.total_input.add(leg.input)?;
    totals.total_output = totals.total_output.add(leg.output)?;
    Ok(())
}

fn apply_leg_for_spend(
    leg: &SelectedTakeOrderLeg,
    totals: &mut SpendSimulationTotals,
) -> Result<(), RaindexError> {
    totals.remaining_input = totals.remaining_input.sub(leg.input)?;
    totals.total_input = totals.total_input.add(leg.input)?;
    totals.total_output = totals.total_output.add(leg.output)?;
    Ok(())
}

fn filter_candidates_by_price_cap(
    candidates: Vec<TakeOrderCandidate>,
    price_cap: Float,
) -> Result<Vec<TakeOrderCandidate>, RaindexError> {
    let log_rejections = tracing::enabled!(Level::DEBUG);
    let mut filtered = Vec::new();
    let mut logged_rejections = 0usize;
    let mut omitted_rejections = 0usize;

    for candidate in candidates {
        let ratio = candidate.ratio;
        if ratio.lte(price_cap)? {
            filtered.push(candidate);
        } else if log_rejections && logged_rejections < MAX_INFO_PRICE_CAP_REJECTION_LOGS {
            debug!(
                raindex = %candidate.raindex,
                order_hash = %order_hash(&candidate),
                input_io_index = candidate.input_io_index,
                output_io_index = candidate.output_io_index,
                max_output = %format_float(candidate.max_output),
                ratio = %format_float(candidate.ratio),
                price_cap = %format_float(price_cap),
                decision = "rejected",
                reason = "above_price_cap",
                "take-order candidate decision"
            );
            logged_rejections += 1;
        } else {
            omitted_rejections += 1;
        }
    }

    if omitted_rejections > 0 {
        info!(
            logged_rejections,
            omitted_rejections, "omitted additional price-cap candidate rejection logs"
        );
    }

    Ok(filtered)
}

pub fn simulate_buy_over_candidates(
    candidates: Vec<TakeOrderCandidate>,
    buy_target: Float,
    price_cap: Float,
) -> Result<SimulationResult, RaindexError> {
    let started_at = Timing::now();
    let initial_candidate_count = candidates.len();
    let mut filtered = filter_candidates_by_price_cap(candidates, price_cap)?;
    let filtered_candidate_count = filtered.len();
    sort_candidates_by_price(&mut filtered)?;

    let zero = Float::zero()?;
    let mut totals = BuySimulationTotals {
        remaining_output: buy_target,
        total_input: zero,
        total_output: zero,
    };
    let mut legs: Vec<SelectedTakeOrderLeg> = Vec::new();

    for candidate in filtered.into_iter() {
        if totals.remaining_output.lte(zero)? {
            break;
        }

        if let Some(leg) = take_leg_for_buy(candidate, totals.remaining_output)? {
            apply_leg_for_buy(&leg, &mut totals)?;
            legs.push(leg);
        }
    }

    let result = SimulationResult {
        legs,
        total_input: totals.total_input,
        total_output: totals.total_output,
    };
    info!(
        mode = "buy",
        initial_candidate_count,
        candidates_after_price_cap = filtered_candidate_count,
        selected_legs_count = result.legs.len(),
        total_input = %format_float(result.total_input),
        total_output = %format_float(result.total_output),
        fully_filled = result.total_output.eq(buy_target).unwrap_or(false),
        worst_selected_ratio = %result
            .legs
            .last()
            .map(|leg| format_float(leg.candidate.ratio))
            .unwrap_or_else(|| "none".to_string()),
        duration_ms = started_at.elapsed_ms(),
        "simulated buy over candidates"
    );
    Ok(result)
}

pub fn simulate_spend_over_candidates(
    candidates: Vec<TakeOrderCandidate>,
    spend_budget: Float,
    price_cap: Float,
) -> Result<SimulationResult, RaindexError> {
    let started_at = Timing::now();
    let initial_candidate_count = candidates.len();
    let mut filtered = filter_candidates_by_price_cap(candidates, price_cap)?;
    let filtered_candidate_count = filtered.len();
    sort_candidates_by_price(&mut filtered)?;

    let zero = Float::zero()?;
    let mut totals = SpendSimulationTotals {
        remaining_input: spend_budget,
        total_input: zero,
        total_output: zero,
    };
    let mut legs: Vec<SelectedTakeOrderLeg> = Vec::new();

    for candidate in filtered.into_iter() {
        if totals.remaining_input.lte(zero)? {
            break;
        }

        if let Some(leg) = take_leg_for_spend(candidate, totals.remaining_input)? {
            apply_leg_for_spend(&leg, &mut totals)?;
            legs.push(leg);
        }
    }

    let result = SimulationResult {
        legs,
        total_input: totals.total_input,
        total_output: totals.total_output,
    };
    info!(
        mode = "spend",
        initial_candidate_count,
        candidates_after_price_cap = filtered_candidate_count,
        selected_legs_count = result.legs.len(),
        total_input = %format_float(result.total_input),
        total_output = %format_float(result.total_output),
        fully_filled = result.total_input.eq(spend_budget).unwrap_or(false),
        worst_selected_ratio = %result
            .legs
            .last()
            .map(|leg| format_float(leg.candidate.ratio))
            .unwrap_or_else(|| "none".to_string()),
        duration_ms = started_at.elapsed_ms(),
        "simulated spend over candidates"
    );
    Ok(result)
}

#[cfg(test)]
#[cfg(not(target_family = "wasm"))]
mod tests {
    use super::*;
    use crate::test_helpers::candidates::make_simulation_candidate;
    use rain_math_float::Float;

    fn high_price_cap() -> Float {
        Float::parse("1000000".to_string()).unwrap()
    }

    #[test]
    fn test_simulate_buy_empty_candidates() {
        let candidates: Vec<TakeOrderCandidate> = vec![];
        let buy_target = Float::parse("1.5".to_string()).unwrap();

        let result =
            simulate_buy_over_candidates(candidates, buy_target, high_price_cap()).unwrap();

        assert!(result.legs.is_empty());
        assert!(result.total_output.eq(Float::zero().unwrap()).unwrap());
        assert!(result.total_input.eq(Float::zero().unwrap()).unwrap());
    }

    #[test]
    fn test_simulate_buy_single_candidate_full_fill() {
        let f1_5 = Float::parse("1.5".to_string()).unwrap();
        let f2_25 = Float::parse("2.25".to_string()).unwrap();
        let candidate = make_simulation_candidate(f2_25, f1_5);
        let candidates = vec![candidate];
        let buy_target = f2_25;

        let result =
            simulate_buy_over_candidates(candidates, buy_target, high_price_cap()).unwrap();

        assert_eq!(result.legs.len(), 1);
        assert!(result.total_output.eq(f2_25).unwrap());
        let expected_input = Float::parse("3.375".to_string()).unwrap();
        assert!(result.total_input.eq(expected_input).unwrap());
    }

    #[test]
    fn test_simulate_buy_single_candidate_partial_fill() {
        let f1_25 = Float::parse("1.25".to_string()).unwrap();
        let f4_5 = Float::parse("4.5".to_string()).unwrap();
        let f2_0 = Float::parse("2".to_string()).unwrap();
        let candidate = make_simulation_candidate(f4_5, f1_25);
        let candidates = vec![candidate];
        let buy_target = f2_0;

        let result =
            simulate_buy_over_candidates(candidates, buy_target, high_price_cap()).unwrap();

        assert_eq!(result.legs.len(), 1);
        assert!(result.total_output.eq(f2_0).unwrap());
        let expected_input = Float::parse("2.5".to_string()).unwrap();
        assert!(result.total_input.eq(expected_input).unwrap());
    }

    #[test]
    fn test_simulate_buy_multiple_candidates_sorted_by_price() {
        let f1_5 = Float::parse("1.5".to_string()).unwrap();
        let f2_75 = Float::parse("2.75".to_string()).unwrap();
        let f3_25 = Float::parse("3.25".to_string()).unwrap();
        let expensive = make_simulation_candidate(f2_75, f3_25);
        let cheap = make_simulation_candidate(f2_75, f1_5);
        let candidates = vec![expensive, cheap];
        let buy_target = f2_75;

        let result =
            simulate_buy_over_candidates(candidates, buy_target, high_price_cap()).unwrap();

        assert_eq!(result.legs.len(), 1);
        assert!(
            result.legs[0].candidate.ratio.eq(f1_5).unwrap(),
            "Should use cheapest candidate first"
        );
        assert!(result.total_output.eq(f2_75).unwrap());
        let expected_input = Float::parse("4.125".to_string()).unwrap();
        assert!(result.total_input.eq(expected_input).unwrap());
    }

    #[test]
    fn test_simulate_buy_multiple_candidates_uses_multiple() {
        let f1_25 = Float::parse("1.25".to_string()).unwrap();
        let f2_5 = Float::parse("2.5".to_string()).unwrap();
        let f3_75 = Float::parse("3.75".to_string()).unwrap();
        let cheap = make_simulation_candidate(f1_25, f1_25);
        let expensive = make_simulation_candidate(f2_5, f2_5);
        let candidates = vec![expensive, cheap];
        let buy_target = f3_75;

        let result =
            simulate_buy_over_candidates(candidates, buy_target, high_price_cap()).unwrap();

        assert_eq!(result.legs.len(), 2, "Should use both candidates");
        assert!(
            result.legs[0].candidate.ratio.eq(f1_25).unwrap(),
            "First leg should be cheapest"
        );
        assert!(
            result.legs[1].candidate.ratio.eq(f2_5).unwrap(),
            "Second leg should be more expensive"
        );
        assert!(result.total_output.eq(f3_75).unwrap());
    }

    #[test]
    fn test_simulate_buy_zero_target() {
        let f2_5 = Float::parse("2.5".to_string()).unwrap();
        let f1_75 = Float::parse("1.75".to_string()).unwrap();
        let candidate = make_simulation_candidate(f2_5, f1_75);
        let candidates = vec![candidate];
        let buy_target = Float::zero().unwrap();

        let result =
            simulate_buy_over_candidates(candidates, buy_target, high_price_cap()).unwrap();

        assert!(result.legs.is_empty());
        assert!(result.total_output.eq(Float::zero().unwrap()).unwrap());
        assert!(result.total_input.eq(Float::zero().unwrap()).unwrap());
    }

    #[test]
    fn test_simulate_buy_with_different_ratio() {
        let f0_5 = Float::parse("0.5".to_string()).unwrap();
        let f4_5 = Float::parse("4.5".to_string()).unwrap();
        let candidate = make_simulation_candidate(f4_5, f0_5);
        let candidates = vec![candidate];
        let buy_target = f4_5;

        let result =
            simulate_buy_over_candidates(candidates, buy_target, high_price_cap()).unwrap();

        assert_eq!(result.legs.len(), 1);
        assert!(result.total_output.eq(f4_5).unwrap());
        let expected_input = Float::parse("2.25".to_string()).unwrap();
        assert!(result.total_input.eq(expected_input).unwrap());
    }

    #[test]
    fn test_simulate_buy_multi_leg_partial_fill_second_leg() {
        let ratio_1 = Float::parse("1".to_string()).unwrap();
        let ratio_2 = Float::parse("2".to_string()).unwrap();
        let ratio_3 = Float::parse("3".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let candidate_cheap = make_simulation_candidate(max_output, ratio_1);
        let candidate_mid = make_simulation_candidate(max_output, ratio_2);
        let candidate_expensive = make_simulation_candidate(max_output, ratio_3);

        let candidates = vec![candidate_expensive, candidate_mid, candidate_cheap];
        let buy_target = Float::parse("125".to_string()).unwrap();

        let result =
            simulate_buy_over_candidates(candidates, buy_target, high_price_cap()).unwrap();

        assert_eq!(result.legs.len(), 2, "Should use exactly 2 legs");

        assert!(
            result.legs[0].candidate.ratio.eq(ratio_1).unwrap(),
            "First leg should be cheapest (ratio=1)"
        );
        assert!(
            result.legs[1].candidate.ratio.eq(ratio_2).unwrap(),
            "Second leg should be mid-price (ratio=2)"
        );

        let expected_leg1_output = Float::parse("100".to_string()).unwrap();
        let expected_leg1_input = Float::parse("100".to_string()).unwrap();
        assert!(
            result.legs[0].output.eq(expected_leg1_output).unwrap(),
            "Leg 1 output should be 100 (full fill)"
        );
        assert!(
            result.legs[0].input.eq(expected_leg1_input).unwrap(),
            "Leg 1 input should be 100"
        );

        let expected_leg2_output = Float::parse("25".to_string()).unwrap();
        let expected_leg2_input = Float::parse("50".to_string()).unwrap();
        assert!(
            result.legs[1].output.eq(expected_leg2_output).unwrap(),
            "Leg 2 output should be 25 (partial)"
        );
        assert!(
            result.legs[1].input.eq(expected_leg2_input).unwrap(),
            "Leg 2 input should be 50 (25 * 2)"
        );

        let expected_total_output = Float::parse("125".to_string()).unwrap();
        let expected_total_input = Float::parse("150".to_string()).unwrap();
        assert!(
            result.total_output.eq(expected_total_output).unwrap(),
            "total_output should be 125 (100 + 25)"
        );
        assert!(
            result.total_input.eq(expected_total_input).unwrap(),
            "total_input should be 150 (100 + 50)"
        );
    }

    #[test]
    fn test_take_leg_for_buy_full_fill() {
        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_simulation_candidate(max_output, ratio);
        let remaining_output = Float::parse("200".to_string()).unwrap();

        let leg = take_leg_for_buy(candidate, remaining_output)
            .unwrap()
            .unwrap();

        assert!(
            leg.output.eq(max_output).unwrap(),
            "Full fill: output should equal max_output"
        );
        let expected_input = Float::parse("200".to_string()).unwrap();
        assert!(
            leg.input.eq(expected_input).unwrap(),
            "Full fill: input should be max_output * ratio"
        );
    }

    #[test]
    fn test_take_leg_for_buy_partial_fill() {
        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_simulation_candidate(max_output, ratio);
        let remaining_output = Float::parse("50".to_string()).unwrap();

        let leg = take_leg_for_buy(candidate, remaining_output)
            .unwrap()
            .unwrap();

        assert!(
            leg.output.eq(remaining_output).unwrap(),
            "Partial fill: output should equal remaining_output"
        );
        let expected_input = Float::parse("100".to_string()).unwrap();
        assert!(
            leg.input.eq(expected_input).unwrap(),
            "Partial fill: input should be output * ratio"
        );
    }

    #[test]
    fn test_take_leg_for_buy_zero_price_full_fill() {
        let max_output = Float::parse("100".to_string()).unwrap();
        let zero_ratio = Float::zero().unwrap();
        let candidate = make_simulation_candidate(max_output, zero_ratio);
        let remaining_output = Float::parse("200".to_string()).unwrap();

        let leg = take_leg_for_buy(candidate, remaining_output)
            .unwrap()
            .unwrap();

        assert!(
            leg.output.eq(max_output).unwrap(),
            "Zero-price: output should equal max_output (capped by capacity)"
        );
        assert!(
            leg.input.eq(Float::zero().unwrap()).unwrap(),
            "Zero-price: input should be 0"
        );
    }

    #[test]
    fn test_take_leg_for_buy_zero_remaining() {
        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_simulation_candidate(max_output, ratio);
        let remaining_output = Float::zero().unwrap();

        let result = take_leg_for_buy(candidate, remaining_output).unwrap();

        assert!(result.is_none(), "Zero remaining should return None");
    }

    #[test]
    fn test_sort_candidates_by_price() {
        let ratio_1 = Float::parse("1".to_string()).unwrap();
        let ratio_2 = Float::parse("2".to_string()).unwrap();
        let ratio_3 = Float::parse("3".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let mut candidates = vec![
            make_simulation_candidate(max_output, ratio_3),
            make_simulation_candidate(max_output, ratio_1),
            make_simulation_candidate(max_output, ratio_2),
        ];

        sort_candidates_by_price(&mut candidates).unwrap();

        assert!(
            candidates[0].ratio.eq(ratio_1).unwrap(),
            "First candidate should have lowest ratio"
        );
        assert!(
            candidates[1].ratio.eq(ratio_2).unwrap(),
            "Second candidate should have middle ratio"
        );
        assert!(
            candidates[2].ratio.eq(ratio_3).unwrap(),
            "Third candidate should have highest ratio"
        );
    }

    #[test]
    fn test_simulate_buy_zero_price_candidate_included() {
        let zero_ratio = Float::zero().unwrap();
        let ratio_2 = Float::parse("2".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let zero_price_candidate = make_simulation_candidate(max_output, zero_ratio);
        let normal_candidate = make_simulation_candidate(max_output, ratio_2);

        let candidates = vec![normal_candidate, zero_price_candidate];
        let buy_target = Float::parse("200".to_string()).unwrap();

        let result =
            simulate_buy_over_candidates(candidates, buy_target, high_price_cap()).unwrap();

        assert_eq!(result.legs.len(), 2, "Both candidates should be used");
        assert!(
            result.legs[0].candidate.ratio.eq(zero_ratio).unwrap(),
            "First leg should be zero-price (sorted first)"
        );
        assert!(
            result.legs[1].candidate.ratio.eq(ratio_2).unwrap(),
            "Second leg should be normal candidate with ratio=2"
        );
        let expected_total_output = Float::parse("200".to_string()).unwrap();
        let expected_total_input = Float::parse("200".to_string()).unwrap();
        assert!(
            result.total_output.eq(expected_total_output).unwrap(),
            "total_output should equal 200 (100 + 100)"
        );
        assert!(
            result.total_input.eq(expected_total_input).unwrap(),
            "total_input should be 200 (0 for zero-price + 200 for normal)"
        );
    }

    #[test]
    fn test_apply_leg_for_buy_updates_totals() {
        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_simulation_candidate(max_output, ratio);
        let output = Float::parse("50".to_string()).unwrap();
        let input = Float::parse("100".to_string()).unwrap();

        let leg = SelectedTakeOrderLeg {
            candidate,
            input,
            output,
        };

        let mut totals = BuySimulationTotals {
            remaining_output: Float::parse("200".to_string()).unwrap(),
            total_output: Float::parse("25".to_string()).unwrap(),
            total_input: Float::parse("50".to_string()).unwrap(),
        };

        apply_leg_for_buy(&leg, &mut totals).unwrap();

        let expected_remaining = Float::parse("150".to_string()).unwrap();
        let expected_total_output = Float::parse("75".to_string()).unwrap();
        let expected_total_input = Float::parse("150".to_string()).unwrap();

        assert!(
            totals.remaining_output.eq(expected_remaining).unwrap(),
            "remaining_output should be reduced by output"
        );
        assert!(
            totals.total_output.eq(expected_total_output).unwrap(),
            "total_output should be increased by output"
        );
        assert!(
            totals.total_input.eq(expected_total_input).unwrap(),
            "total_input should be increased by input"
        );
    }

    #[test]
    fn test_filter_candidates_by_price_cap() {
        let ratio_1 = Float::parse("1".to_string()).unwrap();
        let ratio_2 = Float::parse("2".to_string()).unwrap();
        let ratio_3 = Float::parse("3".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let candidate_1 = make_simulation_candidate(max_output, ratio_1);
        let candidate_2 = make_simulation_candidate(max_output, ratio_2);
        let candidate_3 = make_simulation_candidate(max_output, ratio_3);

        let candidates = vec![candidate_1, candidate_2, candidate_3];
        let price_cap = Float::parse("2".to_string()).unwrap();

        let filtered = filter_candidates_by_price_cap(candidates, price_cap).unwrap();

        assert_eq!(
            filtered.len(),
            2,
            "Only candidates with ratio <= 2 should pass"
        );
        assert!(filtered[0].ratio.eq(ratio_1).unwrap());
        assert!(filtered[1].ratio.eq(ratio_2).unwrap());
    }

    #[test]
    fn test_simulate_buy_with_price_cap_filters_expensive() {
        let ratio_1 = Float::parse("1".to_string()).unwrap();
        let ratio_2 = Float::parse("2".to_string()).unwrap();
        let ratio_3 = Float::parse("3".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let candidate_1 = make_simulation_candidate(max_output, ratio_1);
        let candidate_2 = make_simulation_candidate(max_output, ratio_2);
        let candidate_3 = make_simulation_candidate(max_output, ratio_3);

        let candidates = vec![candidate_3, candidate_2, candidate_1];
        let buy_target = Float::parse("300".to_string()).unwrap();
        let price_cap = Float::parse("2".to_string()).unwrap();

        let result = simulate_buy_over_candidates(candidates, buy_target, price_cap).unwrap();

        assert_eq!(
            result.legs.len(),
            2,
            "Only 2 candidates should be used (ratio <= 2)"
        );
        assert!(
            result.legs[0].candidate.ratio.eq(ratio_1).unwrap(),
            "First leg should be cheapest"
        );
        assert!(
            result.legs[1].candidate.ratio.eq(ratio_2).unwrap(),
            "Second leg should be at price cap"
        );
        let expected_total_output = Float::parse("200".to_string()).unwrap();
        assert!(
            result.total_output.eq(expected_total_output).unwrap(),
            "total_output should be 200 (100 + 100), not 300"
        );
    }

    #[test]
    fn test_simulate_buy_price_cap_zero_only_free_orders() {
        let zero_ratio = Float::zero().unwrap();
        let ratio_1 = Float::parse("1".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let free_candidate = make_simulation_candidate(max_output, zero_ratio);
        let paid_candidate = make_simulation_candidate(max_output, ratio_1);

        let candidates = vec![paid_candidate, free_candidate];
        let buy_target = Float::parse("200".to_string()).unwrap();
        let price_cap = Float::zero().unwrap();

        let result = simulate_buy_over_candidates(candidates, buy_target, price_cap).unwrap();

        assert_eq!(
            result.legs.len(),
            1,
            "Only zero-price candidate should be used"
        );
        assert!(result.legs[0].candidate.ratio.eq(zero_ratio).unwrap());
        let expected_total_output = Float::parse("100".to_string()).unwrap();
        assert!(result.total_output.eq(expected_total_output).unwrap());
        assert!(result.total_input.eq(Float::zero().unwrap()).unwrap());
    }

    #[test]
    fn test_simulate_buy_all_candidates_above_price_cap() {
        let ratio_5 = Float::parse("5".to_string()).unwrap();
        let ratio_10 = Float::parse("10".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let candidate_1 = make_simulation_candidate(max_output, ratio_5);
        let candidate_2 = make_simulation_candidate(max_output, ratio_10);

        let candidates = vec![candidate_1, candidate_2];
        let buy_target = Float::parse("100".to_string()).unwrap();
        let price_cap = Float::parse("2".to_string()).unwrap();

        let result = simulate_buy_over_candidates(candidates, buy_target, price_cap).unwrap();

        assert!(
            result.legs.is_empty(),
            "No candidates should pass the price cap filter"
        );
        assert!(result.total_output.eq(Float::zero().unwrap()).unwrap());
        assert!(result.total_input.eq(Float::zero().unwrap()).unwrap());
    }

    #[test]
    fn test_simulate_spend_single_candidate_full_spend() {
        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_simulation_candidate(max_output, ratio);
        let candidates = vec![candidate];
        let spend_budget = Float::parse("200".to_string()).unwrap();

        let result =
            simulate_spend_over_candidates(candidates, spend_budget, high_price_cap()).unwrap();

        assert_eq!(result.legs.len(), 1);
        assert!(result.total_input.eq(spend_budget).unwrap());
        assert!(result.total_output.eq(max_output).unwrap());
    }

    #[test]
    fn test_simulate_spend_single_candidate_partial_spend() {
        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_simulation_candidate(max_output, ratio);
        let candidates = vec![candidate];
        let spend_budget = Float::parse("100".to_string()).unwrap();

        let result =
            simulate_spend_over_candidates(candidates, spend_budget, high_price_cap()).unwrap();

        assert_eq!(result.legs.len(), 1);
        assert!(result.total_input.eq(spend_budget).unwrap());
        let expected_output = Float::parse("50".to_string()).unwrap();
        assert!(result.total_output.eq(expected_output).unwrap());
    }

    #[test]
    fn test_simulate_spend_multiple_candidates() {
        let ratio_1 = Float::parse("1".to_string()).unwrap();
        let ratio_2 = Float::parse("2".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let cheap = make_simulation_candidate(max_output, ratio_1);
        let expensive = make_simulation_candidate(max_output, ratio_2);
        let candidates = vec![expensive, cheap];
        let spend_budget = Float::parse("150".to_string()).unwrap();

        let result =
            simulate_spend_over_candidates(candidates, spend_budget, high_price_cap()).unwrap();

        assert_eq!(result.legs.len(), 2, "Should use both candidates");
        assert!(
            result.legs[0].candidate.ratio.eq(ratio_1).unwrap(),
            "First leg should be cheapest"
        );

        let expected_total_input = Float::parse("150".to_string()).unwrap();
        assert!(result.total_input.eq(expected_total_input).unwrap());

        let expected_total_output = Float::parse("125".to_string()).unwrap();
        assert!(
            result.total_output.eq(expected_total_output).unwrap(),
            "total_output should be 125 (100 from cheap + 25 from expensive)"
        );
    }

    #[test]
    fn test_simulate_spend_with_price_cap() {
        let ratio_1 = Float::parse("1".to_string()).unwrap();
        let ratio_3 = Float::parse("3".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let cheap = make_simulation_candidate(max_output, ratio_1);
        let expensive = make_simulation_candidate(max_output, ratio_3);
        let candidates = vec![expensive, cheap];
        let spend_budget = Float::parse("200".to_string()).unwrap();
        let price_cap = Float::parse("2".to_string()).unwrap();

        let result = simulate_spend_over_candidates(candidates, spend_budget, price_cap).unwrap();

        assert_eq!(result.legs.len(), 1, "Only cheap candidate should be used");
        let expected_input = Float::parse("100".to_string()).unwrap();
        assert!(
            result.total_input.eq(expected_input).unwrap(),
            "Should only spend 100 (max for cheap candidate)"
        );
    }

    #[test]
    fn test_simulate_spend_empty_candidates() {
        let candidates: Vec<TakeOrderCandidate> = vec![];
        let spend_budget = Float::parse("100".to_string()).unwrap();

        let result =
            simulate_spend_over_candidates(candidates, spend_budget, high_price_cap()).unwrap();

        assert!(result.legs.is_empty());
        assert!(result.total_input.eq(Float::zero().unwrap()).unwrap());
        assert!(result.total_output.eq(Float::zero().unwrap()).unwrap());
    }

    #[test]
    fn test_simulate_spend_zero_budget() {
        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_simulation_candidate(max_output, ratio);
        let candidates = vec![candidate];
        let spend_budget = Float::zero().unwrap();

        let result =
            simulate_spend_over_candidates(candidates, spend_budget, high_price_cap()).unwrap();

        assert!(result.legs.is_empty());
        assert!(result.total_input.eq(Float::zero().unwrap()).unwrap());
        assert!(result.total_output.eq(Float::zero().unwrap()).unwrap());
    }

    #[test]
    fn test_simulate_spend_zero_price_candidate_included() {
        let zero_ratio = Float::zero().unwrap();
        let ratio_2 = Float::parse("2".to_string()).unwrap();
        let max_output = Float::parse("100".to_string()).unwrap();

        let zero_price_candidate = make_simulation_candidate(max_output, zero_ratio);
        let normal_candidate = make_simulation_candidate(max_output, ratio_2);

        let candidates = vec![normal_candidate, zero_price_candidate];
        let spend_budget = Float::parse("100".to_string()).unwrap();

        let result =
            simulate_spend_over_candidates(candidates, spend_budget, high_price_cap()).unwrap();

        assert_eq!(result.legs.len(), 2, "Both candidates should be used");
        assert!(
            result.legs[0].candidate.ratio.eq(zero_ratio).unwrap(),
            "First leg should be zero-price (sorted first)"
        );
        assert!(
            result.legs[1].candidate.ratio.eq(ratio_2).unwrap(),
            "Second leg should be normal candidate with ratio=2"
        );
        let expected_total_output = Float::parse("150".to_string()).unwrap();
        assert!(
            result.total_output.eq(expected_total_output).unwrap(),
            "total_output should be 150 (100 free + 50 from spending 100 at ratio 2)"
        );
        assert!(
            result.total_input.eq(spend_budget).unwrap(),
            "total_input should be 100 (0 for zero-price + 100 for normal)"
        );
    }
}
