use crate::raindex_client::order_quotes::{
    get_order_quotes_batch_with_injector, RaindexOrderQuote,
};
use crate::raindex_client::orders::RaindexOrder;
use crate::raindex_client::RaindexError;
use crate::utils::timing::Timing;
use alloy::primitives::{keccak256, Address, B256};
use alloy::sol_types::SolValue;
use rain_math_float::Float;
use raindex_bindings::IRaindexV7::{OrderV4, SignedContextV2};
use raindex_quote::SignedContextInjector;
#[cfg(target_family = "wasm")]
use std::str::FromStr;
use tracing::{debug, enabled, info, Level};

const MAX_INFO_CANDIDATE_DECISION_LOGS: usize = 100;
const MAX_LOGGED_ERROR_CHARS: usize = 512;

fn indices_in_bounds(order: &OrderV4, input_index: u32, output_index: u32) -> bool {
    (input_index as usize) < order.validInputs.len()
        && (output_index as usize) < order.validOutputs.len()
}

fn matches_direction(
    order: &OrderV4,
    input_index: u32,
    output_index: u32,
    input_token: Address,
    output_token: Address,
) -> bool {
    let order_input_token = order.validInputs[input_index as usize].token;
    let order_output_token = order.validOutputs[output_index as usize].token;
    order_input_token == input_token && order_output_token == output_token
}

fn has_capacity(
    data: &crate::raindex_client::order_quotes::RaindexOrderQuoteValue,
) -> Result<bool, RaindexError> {
    Ok(data.max_output.gt(Float::zero()?)?)
}

fn format_float(value: Float) -> String {
    value
        .format()
        .unwrap_or_else(|_| "<format_error>".to_string())
}

fn order_hash(order: &OrderV4) -> B256 {
    B256::from(keccak256(order.abi_encode()))
}

fn truncate_error(value: &str) -> String {
    if value.chars().count() <= MAX_LOGGED_ERROR_CHARS {
        value.to_string()
    } else {
        format!(
            "{}...",
            value
                .chars()
                .take(MAX_LOGGED_ERROR_CHARS)
                .collect::<String>()
        )
    }
}

#[derive(Clone, Debug)]
pub struct TakeOrderCandidate {
    pub raindex: Address,
    pub order: OrderV4,
    pub input_io_index: u32,
    pub output_io_index: u32,
    pub max_output: Float,
    pub ratio: Float,
    /// Signed context data attached to this candidate.
    ///
    /// Composition order is `[oracle..., injected...]`: any entries fetched
    /// from the order's oracle endpoint come first, followed by entries
    /// contributed by a caller-supplied [`SignedContextInjector`]. Strategies
    /// that verify signed context by index must be aware of this ordering.
    pub signed_context: Vec<SignedContextV2>,
}

#[derive(Default)]
struct CandidateBuildStats {
    quote_count: usize,
    quote_failures: usize,
    skipped_missing_data: usize,
    skipped_out_of_bounds_io_indices: usize,
    skipped_wrong_direction: usize,
    skipped_zero_or_non_positive_capacity: usize,
    info_rejection_logs: usize,
    omitted_info_rejection_logs: usize,
}

struct CandidateDecisionLog<'a> {
    stats: &'a mut CandidateBuildStats,
    raindex: Address,
    order: &'a OrderV4,
    quote: &'a RaindexOrderQuote,
    requested_input_token: Address,
    requested_output_token: Address,
    decision: &'static str,
    reason: &'static str,
}

fn log_candidate_decision(params: CandidateDecisionLog<'_>) {
    let CandidateDecisionLog {
        stats,
        raindex,
        order,
        quote,
        requested_input_token,
        requested_output_token,
        decision,
        reason,
    } = params;

    let is_accepted = decision == "accepted";
    if is_accepted {
        if !enabled!(Level::DEBUG) {
            return;
        }
    } else if stats.info_rejection_logs >= MAX_INFO_CANDIDATE_DECISION_LOGS {
        stats.omitted_info_rejection_logs += 1;
        return;
    }

    let input = order.validInputs.get(quote.pair.input_index as usize);
    let output = order.validOutputs.get(quote.pair.output_index as usize);
    let quoted_max_output = quote
        .data
        .as_ref()
        .map(|data| format_float(data.max_output))
        .unwrap_or_else(|| "none".to_string());
    let quoted_ratio = quote
        .data
        .as_ref()
        .map(|data| format_float(data.ratio))
        .unwrap_or_else(|| "none".to_string());
    let quote_error = quote.error.as_deref().map(truncate_error);

    if is_accepted {
        debug!(
            raindex = %raindex,
            order_hash = %order_hash(order),
            input_io_index = quote.pair.input_index,
            output_io_index = quote.pair.output_index,
            input_token = %input.map(|io| io.token).unwrap_or(Address::ZERO),
            output_token = %output.map(|io| io.token).unwrap_or(Address::ZERO),
            input_vault_id = %input.map(|io| io.vaultId).unwrap_or_default(),
            output_vault_id = %output.map(|io| io.vaultId).unwrap_or_default(),
            requested_input_token = %requested_input_token,
            requested_output_token = %requested_output_token,
            quote_success = quote.success,
            quote_error = ?quote_error,
            quoted_max_output = %quoted_max_output,
            quoted_ratio = %quoted_ratio,
            signed_context_count = quote.signed_context.len(),
            decision,
            reason,
            "take-order candidate decision"
        );
    } else {
        info!(
            raindex = %raindex,
            order_hash = %order_hash(order),
            input_io_index = quote.pair.input_index,
            output_io_index = quote.pair.output_index,
            input_token = %input.map(|io| io.token).unwrap_or(Address::ZERO),
            output_token = %output.map(|io| io.token).unwrap_or(Address::ZERO),
            input_vault_id = %input.map(|io| io.vaultId).unwrap_or_default(),
            output_vault_id = %output.map(|io| io.vaultId).unwrap_or_default(),
            requested_input_token = %requested_input_token,
            requested_output_token = %requested_output_token,
            quote_success = quote.success,
            quote_error = ?quote_error,
            quoted_max_output = %quoted_max_output,
            quoted_ratio = %quoted_ratio,
            signed_context_count = quote.signed_context.len(),
            decision,
            reason,
            "take-order candidate decision"
        );
        stats.info_rejection_logs += 1;
    }
}

fn get_raindex_address(order: &RaindexOrder) -> Result<Address, RaindexError> {
    #[cfg(target_family = "wasm")]
    {
        Ok(Address::from_str(&order.raindex())?)
    }
    #[cfg(not(target_family = "wasm"))]
    {
        Ok(order.raindex())
    }
}

pub async fn build_take_order_candidates_for_pair(
    orders: &[RaindexOrder],
    input_token: Address,
    output_token: Address,
    block_number: Option<u64>,
    chunk_size: Option<u32>,
    counterparty: Address,
    injector: &dyn SignedContextInjector,
) -> Result<Vec<TakeOrderCandidate>, RaindexError> {
    let started_at = Timing::now();
    // Oracle fetch and injector invocation both happen inside the quote
    // pipeline now: `get_order_quotes_batch_with_injector` composes them into
    // `QuoteV3.signedContext` before issuing the quote RPC, so gated orders
    // whose `calculate-io` asserts on signed context do not revert during
    // quoting. Each returned quote carries the exact composed context that
    // the quote call saw, which we propagate straight into the candidate.
    let all_quotes = get_order_quotes_batch_with_injector(
        orders,
        block_number,
        chunk_size,
        counterparty,
        injector,
    )
    .await?;

    let mut all_candidates = vec![];
    let mut stats = CandidateBuildStats::default();
    for (order, quotes) in orders.iter().zip(all_quotes) {
        let order_v4: OrderV4 = order.try_into()?;
        let raindex = get_raindex_address(order)?;

        for quote in &quotes {
            stats.quote_count += 1;
            if let Some(candidate) = try_build_candidate(
                raindex,
                &order_v4,
                quote,
                input_token,
                output_token,
                &mut stats,
            )? {
                all_candidates.push(candidate);
            }
        }
    }

    info!(
        input_order_count = orders.len(),
        quote_count = stats.quote_count,
        candidate_count = all_candidates.len(),
        quote_failures = stats.quote_failures,
        skipped_missing_data = stats.skipped_missing_data,
        skipped_out_of_bounds_io_indices = stats.skipped_out_of_bounds_io_indices,
        skipped_wrong_direction = stats.skipped_wrong_direction,
        skipped_zero_or_non_positive_capacity = stats.skipped_zero_or_non_positive_capacity,
        info_rejection_logs = stats.info_rejection_logs,
        omitted_info_rejection_logs = stats.omitted_info_rejection_logs,
        input_token = %input_token,
        output_token = %output_token,
        block_number = ?block_number,
        duration_ms = started_at.elapsed_ms(),
        "built take-order candidates for pair"
    );

    Ok(all_candidates)
}

fn try_build_candidate(
    raindex: Address,
    order: &OrderV4,
    quote: &RaindexOrderQuote,
    input_token: Address,
    output_token: Address,
    stats: &mut CandidateBuildStats,
) -> Result<Option<TakeOrderCandidate>, RaindexError> {
    let data = match (quote.success, &quote.data) {
        (true, Some(d)) => d,
        (false, _) => {
            stats.quote_failures += 1;
            log_candidate_decision(CandidateDecisionLog {
                stats,
                raindex,
                order,
                quote,
                requested_input_token: input_token,
                requested_output_token: output_token,
                decision: "rejected",
                reason: "quote_failed",
            });
            debug!(
                raindex = %raindex,
                input_index = quote.pair.input_index,
                output_index = quote.pair.output_index,
                error = ?quote.error,
                "skipping failed quote"
            );
            return Ok(None);
        }
        (true, None) => {
            stats.skipped_missing_data += 1;
            log_candidate_decision(CandidateDecisionLog {
                stats,
                raindex,
                order,
                quote,
                requested_input_token: input_token,
                requested_output_token: output_token,
                decision: "rejected",
                reason: "missing_quote_data",
            });
            debug!(
                raindex = %raindex,
                input_index = quote.pair.input_index,
                output_index = quote.pair.output_index,
                "skipping quote with missing data"
            );
            return Ok(None);
        }
    };

    let input_io_index = quote.pair.input_index;
    let output_io_index = quote.pair.output_index;

    if !indices_in_bounds(order, input_io_index, output_io_index) {
        stats.skipped_out_of_bounds_io_indices += 1;
        log_candidate_decision(CandidateDecisionLog {
            stats,
            raindex,
            order,
            quote,
            requested_input_token: input_token,
            requested_output_token: output_token,
            decision: "rejected",
            reason: "out_of_bounds_io_indices",
        });
        debug!(raindex = %raindex, input_io_index, output_io_index, "skipping quote with out-of-bounds IO indices");
        return Ok(None);
    }

    if !matches_direction(
        order,
        input_io_index,
        output_io_index,
        input_token,
        output_token,
    ) {
        stats.skipped_wrong_direction += 1;
        log_candidate_decision(CandidateDecisionLog {
            stats,
            raindex,
            order,
            quote,
            requested_input_token: input_token,
            requested_output_token: output_token,
            decision: "rejected",
            reason: "wrong_direction",
        });
        debug!(raindex = %raindex, input_io_index, output_io_index, "skipping quote with wrong direction");
        return Ok(None);
    }

    if !has_capacity(data)? {
        stats.skipped_zero_or_non_positive_capacity += 1;
        log_candidate_decision(CandidateDecisionLog {
            stats,
            raindex,
            order,
            quote,
            requested_input_token: input_token,
            requested_output_token: output_token,
            decision: "rejected",
            reason: "zero_or_non_positive_capacity",
        });
        debug!(raindex = %raindex, input_io_index, output_io_index, "skipping quote with zero or non-positive capacity");
        return Ok(None);
    }

    log_candidate_decision(CandidateDecisionLog {
        stats,
        raindex,
        order,
        quote,
        requested_input_token: input_token,
        requested_output_token: output_token,
        decision: "accepted",
        reason: "eligible",
    });

    // Defer clone until the candidate survives all filters.
    Ok(Some(TakeOrderCandidate {
        raindex,
        order: order.clone(),
        input_io_index,
        output_io_index,
        max_output: data.max_output,
        ratio: data.ratio,
        signed_context: quote.signed_context.clone(),
    }))
}

#[cfg(test)]
#[cfg(not(target_family = "wasm"))]
mod tests {
    use super::*;
    use crate::test_helpers::orders::make_basic_order;
    use crate::test_helpers::quotes::{make_quote, make_quote_value};
    use alloy::primitives::Address;
    use rain_math_float::Float;

    #[test]
    fn test_indices_in_bounds_valid() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let order = make_basic_order(token_a, token_b);
        assert!(indices_in_bounds(&order, 0, 0));
    }

    #[test]
    fn test_indices_in_bounds_input_out_of_bounds() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let order = make_basic_order(token_a, token_b);
        assert!(!indices_in_bounds(&order, 99, 0));
    }

    #[test]
    fn test_indices_in_bounds_output_out_of_bounds() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let order = make_basic_order(token_a, token_b);
        assert!(!indices_in_bounds(&order, 0, 99));
    }

    #[test]
    fn test_indices_in_bounds_both_out_of_bounds() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let order = make_basic_order(token_a, token_b);
        assert!(!indices_in_bounds(&order, 99, 99));
    }

    #[test]
    fn test_matches_direction_correct() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let order = make_basic_order(token_a, token_b);
        assert!(matches_direction(&order, 0, 0, token_a, token_b));
    }

    #[test]
    fn test_matches_direction_wrong_input() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let token_c = Address::from([6u8; 20]);
        let order = make_basic_order(token_a, token_b);
        assert!(!matches_direction(&order, 0, 0, token_c, token_b));
    }

    #[test]
    fn test_matches_direction_wrong_output() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let token_c = Address::from([6u8; 20]);
        let order = make_basic_order(token_a, token_b);
        assert!(!matches_direction(&order, 0, 0, token_a, token_c));
    }

    #[test]
    fn test_matches_direction_reversed() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let order = make_basic_order(token_a, token_b);
        assert!(!matches_direction(&order, 0, 0, token_b, token_a));
    }

    fn make_quote_value_for_capacity_test(
        max_output: Float,
    ) -> crate::raindex_client::order_quotes::RaindexOrderQuoteValue {
        let zero = Float::zero().unwrap();
        crate::raindex_client::order_quotes::RaindexOrderQuoteValue {
            max_output,
            formatted_max_output: "0".to_string(),
            max_input: zero,
            formatted_max_input: "0".to_string(),
            ratio: zero,
            formatted_ratio: "0".to_string(),
            inverse_ratio: zero,
            formatted_inverse_ratio: "0".to_string(),
            formatted_max_output_as_percent_of_vault: None,
        }
    }

    #[test]
    fn test_has_capacity_positive() {
        let positive = Float::parse("100".to_string()).unwrap();
        let data = make_quote_value_for_capacity_test(positive);
        assert!(has_capacity(&data).unwrap());
    }

    #[test]
    fn test_has_capacity_zero() {
        let zero = Float::zero().unwrap();
        let data = make_quote_value_for_capacity_test(zero);
        assert!(!has_capacity(&data).unwrap());
    }

    #[test]
    fn test_has_capacity_negative() {
        let negative = Float::parse("-1".to_string()).unwrap();
        let data = make_quote_value_for_capacity_test(negative);
        assert!(!has_capacity(&data).unwrap());
    }

    #[test]
    fn test_try_build_candidate_wrong_direction() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let raindex = Address::from([0xAAu8; 20]);

        let order = make_basic_order(token_a, token_b);
        let f1 = Float::parse("1".to_string()).unwrap();
        let quote = make_quote(0, 0, Some(make_quote_value(f1, f1, f1)), true);

        let result = try_build_candidate(
            raindex,
            &order,
            &quote,
            token_b,
            token_a,
            &mut CandidateBuildStats::default(),
        )
        .unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_try_build_candidate_zero_capacity() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let raindex = Address::from([0xAAu8; 20]);

        let order = make_basic_order(token_a, token_b);
        let zero = Float::zero().unwrap();
        let f1 = Float::parse("1".to_string()).unwrap();
        let quote = make_quote(0, 0, Some(make_quote_value(zero, zero, f1)), true);

        let result = try_build_candidate(
            raindex,
            &order,
            &quote,
            token_a,
            token_b,
            &mut CandidateBuildStats::default(),
        )
        .unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_try_build_candidate_success() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let raindex = Address::from([0xAAu8; 20]);

        let order = make_basic_order(token_a, token_b);
        let f1 = Float::parse("1".to_string()).unwrap();
        let f2 = Float::parse("2".to_string()).unwrap();
        let quote = make_quote(0, 0, Some(make_quote_value(f2, f1, f1)), true);

        let result = try_build_candidate(
            raindex,
            &order,
            &quote,
            token_a,
            token_b,
            &mut CandidateBuildStats::default(),
        )
        .unwrap();

        assert!(result.is_some());
        let candidate = result.unwrap();
        assert_eq!(candidate.raindex, raindex);
        assert_eq!(candidate.input_io_index, 0);
        assert_eq!(candidate.output_io_index, 0);
        assert!(candidate.max_output.eq(f2).unwrap());
    }

    #[test]
    fn test_try_build_candidate_failed_quote() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let raindex = Address::from([0xAAu8; 20]);

        let order = make_basic_order(token_a, token_b);
        let quote = make_quote(0, 0, None, false);

        let result = try_build_candidate(
            raindex,
            &order,
            &quote,
            token_a,
            token_b,
            &mut CandidateBuildStats::default(),
        );

        assert!(
            result.is_ok(),
            "Failed quote must not cause an error, got: {:?}",
            result.unwrap_err()
        );
        assert!(
            result.unwrap().is_none(),
            "Failed quote must not produce a candidate"
        );
    }

    #[tokio::test]
    async fn test_noop_injector_returns_empty() {
        use raindex_quote::injector::{NoopInjector, SignedContextInjector};

        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let order = make_basic_order(token_a, token_b);

        let injector = NoopInjector;
        let ctxs = injector.contexts_for(&order, 0, 0, Address::ZERO).await;
        assert!(ctxs.is_empty());
    }

    #[tokio::test]
    async fn test_try_build_candidate_propagates_signed_context() {
        use alloy::primitives::{FixedBytes, U256};

        // A candidate built from a quote that already carries signed context
        // (as produced by the quote pipeline) should inherit that exact list.
        let oracle_entry = SignedContextV2 {
            signer: Address::from([0xAAu8; 20]),
            domainSeparator: alloy::primitives::B256::repeat_byte(0xd5),
            context: vec![FixedBytes::<32>::from(U256::from(1u64).to_be_bytes::<32>())],
            signature: alloy::primitives::Bytes::from(vec![0x01]),
        };
        let injected_entry = SignedContextV2 {
            signer: Address::from([0xBBu8; 20]),
            domainSeparator: alloy::primitives::B256::repeat_byte(0xd5),
            context: vec![FixedBytes::<32>::from(U256::from(2u64).to_be_bytes::<32>())],
            signature: alloy::primitives::Bytes::from(vec![0x02]),
        };

        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let raindex = Address::from([0xAAu8; 20]);

        let order = make_basic_order(token_a, token_b);
        let f1 = Float::parse("1".to_string()).unwrap();
        let f2 = Float::parse("2".to_string()).unwrap();
        let mut quote = make_quote(0, 0, Some(make_quote_value(f2, f1, f1)), true);
        quote.signed_context = vec![oracle_entry.clone(), injected_entry.clone()];

        let result = try_build_candidate(
            raindex,
            &order,
            &quote,
            token_a,
            token_b,
            &mut CandidateBuildStats::default(),
        )
        .unwrap()
        .expect("candidate should be built");

        assert_eq!(result.signed_context.len(), 2);
        assert_eq!(result.signed_context[0].signer, oracle_entry.signer);
        assert_eq!(result.signed_context[1].signer, injected_entry.signer);
    }

    #[test]
    fn test_try_build_candidate_out_of_bounds_indices() {
        let token_a = Address::from([4u8; 20]);
        let token_b = Address::from([5u8; 20]);
        let raindex = Address::from([0xAAu8; 20]);

        let order = make_basic_order(token_a, token_b);
        let f1 = Float::parse("1".to_string()).unwrap();

        let quote_bad_input_index = make_quote(99, 0, Some(make_quote_value(f1, f1, f1)), true);
        let result = try_build_candidate(
            raindex,
            &order,
            &quote_bad_input_index,
            token_a,
            token_b,
            &mut CandidateBuildStats::default(),
        );
        assert!(
            result.is_ok(),
            "Out-of-bounds input index must not cause an error"
        );
        assert!(
            result.unwrap().is_none(),
            "Out-of-bounds input index must not produce a candidate"
        );

        let quote_bad_output_index = make_quote(0, 99, Some(make_quote_value(f1, f1, f1)), true);
        let result = try_build_candidate(
            raindex,
            &order,
            &quote_bad_output_index,
            token_a,
            token_b,
            &mut CandidateBuildStats::default(),
        );
        assert!(
            result.is_ok(),
            "Out-of-bounds output index must not cause an error"
        );
        assert!(
            result.unwrap().is_none(),
            "Out-of-bounds output index must not produce a candidate"
        );
    }
}
