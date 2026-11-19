pub(crate) mod approval;
mod request;
pub(crate) mod result;
mod selection;
pub mod single;

#[cfg(all(test, not(target_family = "wasm")))]
mod e2e_tests;

#[cfg(all(test, not(target_family = "wasm")))]
mod single_tests;

pub use request::TakeOrdersRequest;
pub use result::{ApprovalInfo, TakeOrderEstimate, TakeOrdersCalldataResult, TakeOrdersInfo};
pub use single::{build_candidate_from_quote, estimate_take_order, execute_single_take};

use super::{RaindexClient, RaindexError};
use crate::rpc_client::RpcClient;
use crate::take_orders::preflight::{diagnose_take_orders_failure, TakeOrdersFailureDiagnosis};
use crate::take_orders::{
    build_take_orders_config_from_simulation, simulate_take_orders, BuiltTakeOrdersConfig,
    NoopInjector, ParsedTakeOrdersMode, SelectedTakeOrderLeg, SignedContextInjector,
    TakeOrderCandidate,
};
use crate::utils::timing::Timing;
use alloy::primitives::{keccak256, B256};
use alloy::sol_types::SolValue;
use approval::{check_approval_needed, ApprovalCheckParams};
use raindex_bindings::provider::mk_read_provider;
use tracing::{error, info, info_span, warn, Instrument};
use wasm_bindgen_utils::prelude::*;
use wasm_bindgen_utils::wasm_export;

const MAX_INFO_SELECTED_LEG_LOGS: usize = 100;

macro_rules! emit_selected_leg {
    ($level:ident, $leg_index:expr, $leg:expr, $orderbook:expr, $block_number:expr, $message:expr) => {{
        let order = &$leg.candidate.order;
        let input_index = $leg.candidate.input_io_index as usize;
        let output_index = $leg.candidate.output_io_index as usize;
        let input = order.validInputs.get(input_index);
        let output = order.validOutputs.get(output_index);

        $level!(
            orderbook = %$orderbook,
            block_number = $block_number,
            leg_index = $leg_index,
            order_hash = %order_hash_for_leg($leg),
            input_io_index = $leg.candidate.input_io_index,
            output_io_index = $leg.candidate.output_io_index,
            input_token = %input.map(|io| io.token).unwrap_or(alloy::primitives::Address::ZERO),
            output_token = %output.map(|io| io.token).unwrap_or(alloy::primitives::Address::ZERO),
            input_vault_id = %input.map(|io| io.vaultId).unwrap_or_default(),
            output_vault_id = %output.map(|io| io.vaultId).unwrap_or_default(),
            selected_input = %format_float_for_log($leg.input),
            selected_output = %format_float_for_log($leg.output),
            ratio = %format_float_for_log($leg.candidate.ratio),
            event = $message,
            "take-order leg"
        );
    }};
}

fn truncate_error(value: &str) -> String {
    const MAX_LEN: usize = 512;
    if value.chars().count() <= MAX_LEN {
        value.to_string()
    } else {
        format!("{}...", value.chars().take(MAX_LEN).collect::<String>())
    }
}

fn format_float_for_log(value: rain_math_float::Float) -> String {
    value
        .format()
        .unwrap_or_else(|_| "<format_error>".to_string())
}

fn order_hash_for_leg(leg: &SelectedTakeOrderLeg) -> B256 {
    B256::from(keccak256(leg.candidate.order.abi_encode()))
}

fn same_candidate(
    left: &TakeOrderCandidate,
    right: &TakeOrderCandidate,
) -> Result<bool, RaindexError> {
    Ok(left.raindex == right.raindex
        && left.order == right.order
        && left.input_io_index == right.input_io_index
        && left.output_io_index == right.output_io_index
        && left.signed_context == right.signed_context
        && left.max_output.eq(right.max_output)?
        && left.ratio.eq(right.ratio)?)
}

fn rebuild_without_failing_leg(
    candidates: &mut Vec<TakeOrderCandidate>,
    failing_leg: &SelectedTakeOrderLeg,
    mode: ParsedTakeOrdersMode,
    price_cap: rain_math_float::Float,
) -> Result<Option<(alloy::primitives::Address, BuiltTakeOrdersConfig)>, RaindexError> {
    let mut failing_candidate_index = None;
    for (index, candidate) in candidates.iter().enumerate() {
        if same_candidate(candidate, &failing_leg.candidate)? {
            failing_candidate_index = Some(index);
            break;
        }
    }
    let failing_candidate_index = failing_candidate_index.ok_or_else(|| {
        RaindexError::PreflightError(
            "failing preflight candidate was not found in the remaining candidate set".to_string(),
        )
    })?;
    candidates.remove(failing_candidate_index);

    build_best_remaining_candidates(candidates, mode, price_cap)
}

fn rebuild_without_raindex(
    candidates: &mut Vec<TakeOrderCandidate>,
    failed_raindex: alloy::primitives::Address,
    mode: ParsedTakeOrdersMode,
    price_cap: rain_math_float::Float,
) -> Result<Option<(alloy::primitives::Address, BuiltTakeOrdersConfig)>, RaindexError> {
    candidates.retain(|candidate| candidate.raindex != failed_raindex);
    build_best_remaining_candidates(candidates, mode, price_cap)
}

fn build_best_remaining_candidates(
    candidates: &[TakeOrderCandidate],
    mode: ParsedTakeOrdersMode,
    price_cap: rain_math_float::Float,
) -> Result<Option<(alloy::primitives::Address, BuiltTakeOrdersConfig)>, RaindexError> {
    if candidates.is_empty() {
        return Ok(None);
    }

    let (raindex, simulation) =
        selection::select_best_raindex_simulation(candidates.to_vec(), mode, price_cap)?;

    Ok(
        build_take_orders_config_from_simulation(simulation, mode, price_cap)?
            .map(|built| (raindex, built)),
    )
}

fn log_selected_leg(
    leg_index: usize,
    leg: &SelectedTakeOrderLeg,
    orderbook: alloy::primitives::Address,
    block_number: u64,
    message: &'static str,
) {
    emit_selected_leg!(info, leg_index, leg, orderbook, block_number, message);
}

fn warn_selected_leg(
    leg_index: usize,
    leg: &SelectedTakeOrderLeg,
    orderbook: alloy::primitives::Address,
    block_number: u64,
    message: &'static str,
) {
    emit_selected_leg!(warn, leg_index, leg, orderbook, block_number, message);
}

fn log_selected_legs(
    orderbook: alloy::primitives::Address,
    block_number: u64,
    legs: &[SelectedTakeOrderLeg],
    message: &'static str,
) {
    for (leg_index, leg) in legs.iter().take(MAX_INFO_SELECTED_LEG_LOGS).enumerate() {
        log_selected_leg(leg_index, leg, orderbook, block_number, message);
    }

    let omitted = legs.len().saturating_sub(MAX_INFO_SELECTED_LEG_LOGS);
    if omitted > 0 {
        info!(
            orderbook = %orderbook,
            block_number,
            logged_leg_count = MAX_INFO_SELECTED_LEG_LOGS,
            omitted_leg_count = omitted,
            event = message,
            "omitted additional take-order leg logs"
        );
    }
}

#[wasm_export]
impl RaindexClient {
    /// Generates calldata for `IRaindexV7.takeOrders5` using a mode + price-cap policy.
    ///
    /// This method includes preflight simulation to validate the transaction will succeed
    /// and automatically removes failing orders from the config.
    ///
    /// The request object contains:
    /// - `taker`: Address of the account that will execute the takeOrders transaction
    /// - `chainId`: Chain ID of the target network
    /// - `sellToken`: Token address the taker will GIVE
    /// - `buyToken`: Token address the taker will RECEIVE
    /// - `mode`: One of `buyExact`, `buyUpTo`, `spendExact`, or `spendUpTo`
    /// - `amount`: Target amount (output tokens for buy modes, input tokens for spend modes)
    /// - `priceCap`: human-readable decimal string for max sell per 1 buy
    ///
    /// Returns calldata plus pricing info:
    /// - `calldata`: ABI-encoded bytes for `takeOrders5`.
    /// - `effectivePrice`: expected blended sell per 1 buy from the simulation.
    /// - `prices`: per-leg ratios, best→worst.
    /// - `expectedSell`: simulated sell at current quotes.
    /// - `maxSellCap`: `amount * priceCap` for buy modes, `amount` for spend modes (worst-case on-chain spend cap).
    ///
    /// ## Example (JS)
    /// ```javascript
    /// const res = await client.getTakeOrdersCalldata({
    ///   chainId: 137,
    ///   taker: "0xTAKER...",
    ///   sellToken: "0xSELL...",
    ///   buyToken: "0xBUY...",
    ///   mode: "buyUpTo",
    ///   amount: "10",
    ///   priceCap: "1.2",
    /// });
    /// if (res.error) {
    ///   console.error(res.error.readableMsg);
    /// } else {
    ///   const { calldata, effectivePrice, expectedSell, maxSellCap, prices, raindex } = res.value;
    /// }
    /// ```
    #[wasm_export(
        js_name = "getTakeOrdersCalldata",
        return_description = "Encoded takeOrders5 calldata and price information",
        unchecked_return_type = "TakeOrdersCalldataResult",
        preserve_js_class
    )]
    pub async fn get_take_orders_calldata(
        &self,
        #[wasm_export(
            js_name = "request",
            param_description = "Take orders request parameters"
        )]
        request: TakeOrdersRequest,
    ) -> Result<TakeOrdersCalldataResult, RaindexError> {
        self.get_take_orders_calldata_with_injector(request, &NoopInjector)
            .await
    }
}

impl RaindexClient {
    /// Non-wasm variant of [`Self::get_take_orders_calldata`] that accepts a
    /// caller-supplied [`SignedContextInjector`]. The injector contributes
    /// additional `SignedContextV2` entries appended after any oracle-fetched
    /// contexts for each candidate (composition order: `[oracle..., injected...]`).
    pub async fn get_take_orders_calldata_with_injector(
        &self,
        request: TakeOrdersRequest,
        injector: &dyn SignedContextInjector,
    ) -> Result<TakeOrdersCalldataResult, RaindexError> {
        let span = info_span!(
            "get_take_orders_calldata",
            chain_id = request.chain_id,
            taker = %request.taker,
            sell_token = %request.sell_token,
            buy_token = %request.buy_token,
            mode = ?request.mode,
            amount = %request.amount,
            price_cap = %request.price_cap,
        );
        let result = async move {
            let started_at = Timing::now();
            let req = {
            let step = info_span!("take_orders.request_parsing");
            async {
                let parsed = request::parse_request(&request)?;
                info!(
                    chain_id = request.chain_id,
                    taker = %parsed.taker,
                    sell_token = %parsed.sell_token,
                    buy_token = %parsed.buy_token,
                    mode = ?parsed.mode.mode,
                    amount = %parsed.mode.amount.format().unwrap_or_else(|_| "<format_error>".to_string()),
                    price_cap = %parsed.price_cap.format().unwrap_or_else(|_| "<format_error>".to_string()),
                    "parsed take-orders request"
                );
                Ok::<_, RaindexError>(parsed)
            }
            .instrument(step)
            .await?
        };

        let orders = self
            .fetch_orders_for_pair(request.chain_id, req.sell_token, req.buy_token)
            .instrument(info_span!("take_orders.fetching_orders"))
            .await?;
        let orders_count = orders.len();

        let rpc_urls = {
            let _span = info_span!("take_orders.resolving_rpc_urls").entered();
            let urls = self.get_rpc_urls_for_chain(request.chain_id)?;
            info!(chain_id = request.chain_id, rpc_url_count = urls.len(), "resolved RPC URLs");
            urls
        };
        let rpc_client = RpcClient::new_with_urls(rpc_urls.clone())?;
        let block_number = rpc_client
            .get_latest_block_number()
            .instrument(info_span!("take_orders.fetching_latest_block"))
            .await?;
        info!(chain_id = request.chain_id, block_number, "fetched latest block number");

        let candidates = selection::build_candidates_for_chain(
            &orders,
            req.sell_token,
            req.buy_token,
            Some(block_number),
            None,
            req.taker,
            injector,
        )
        .instrument(info_span!("take_orders.building_candidates"))
        .await?;
        let candidates_count = candidates.len();

        let (mut best_raindex, best_sim) = {
            let _span = info_span!("take_orders.selecting_best_raindex").entered();
            selection::select_best_raindex_simulation(candidates.clone(), req.mode, req.price_cap)?
        };
        let mut remaining_candidates = candidates;

        let mut built = {
            let _span = info_span!("take_orders.building_config", raindex = %best_raindex).entered();
            build_take_orders_config_from_simulation(best_sim.clone(), req.mode, req.price_cap)?
                .ok_or(RaindexError::NoLiquidity)?
        };
        info!(
            raindex = %best_raindex,
            legs_count = built.sim.legs.len(),
            orders_count = built.config.orders.len(),
            "built take-orders config"
        );
        log_selected_legs(
            best_raindex,
            block_number,
            &built.sim.legs,
            "selected take-orders candidate leg",
        );

        let provider =
            mk_read_provider(&rpc_urls).map_err(|e| RaindexError::PreflightError(e.to_string()))?;

        let mut removed_orders_count = 0usize;
        let mut skipped_raindexes_count = 0usize;
        let mut approval_checked_raindex = None;
        let max_iterations = remaining_candidates.len();
        for iteration in 0..max_iterations {
            if approval_checked_raindex != Some(best_raindex) {
                let approval_params = ApprovalCheckParams {
                    rpc_urls: rpc_urls.clone(),
                    sell_token: req.sell_token,
                    taker: req.taker,
                    raindex: best_raindex,
                    mode: req.mode,
                    price_cap: req.price_cap,
                };
                if let Some(approval_result) = check_approval_needed(&approval_params)
                    .instrument(info_span!("take_orders.checking_approval", raindex = %best_raindex))
                    .await?
                {
                    info!(
                        result_type = "approval_required",
                        orders_count,
                        candidates_count,
                        selected_raindex = %best_raindex,
                        selected_legs_count = built.sim.legs.len(),
                        block_number,
                        preflight_iterations = iteration,
                        removed_orders_count,
                        duration_ms = started_at.elapsed_ms(),
                        "take-orders calldata requires approval"
                    );
                    return Ok(approval_result);
                }
                approval_checked_raindex = Some(best_raindex);
            }

            let iteration_started_at = Timing::now();
            let sim_result = simulate_take_orders(
                &provider,
                best_raindex,
                req.taker,
                &built.config,
                Some(block_number),
            )
            .instrument(info_span!(
                "take_orders.preflight_iteration",
                raindex = %best_raindex,
                taker = %req.taker,
                block_number,
                orders_count = built.config.orders.len(),
                iteration
            ))
            .await;

            match sim_result {
                Ok(()) => {
                    log_selected_legs(
                        best_raindex,
                        block_number,
                        &built.sim.legs,
                        "final take-orders transaction leg",
                    );
                    info!(
                        result_type = "take_orders",
                        orders_count,
                        candidates_count,
                        selected_raindex = %best_raindex,
                        selected_legs_count = built.sim.legs.len(),
                        block_number,
                        preflight_iterations = iteration + 1,
                        removed_orders_count,
                        skipped_raindexes_count,
                        duration_ms = started_at.elapsed_ms(),
                        "take-orders calldata ready"
                    );
                    return result::build_calldata_result(
                        best_raindex,
                        built,
                        req.mode,
                        req.price_cap,
                    );
                }
                Err(sim_error) => {
                    warn!(
                        raindex = %best_raindex,
                        taker = %req.taker,
                        block_number,
                        orders_count = built.config.orders.len(),
                        iteration,
                        error = %truncate_error(&sim_error),
                        duration_ms = iteration_started_at.elapsed_ms(),
                        "preflight simulation failed"
                    );
                    let diagnosis = diagnose_take_orders_failure(
                        &provider,
                        best_raindex,
                        req.taker,
                        &built.config,
                        Some(block_number),
                    )
                    .instrument(info_span!(
                        "take_orders.find_failing_order",
                        raindex = %best_raindex,
                        taker = %req.taker,
                        block_number,
                        order_count = built.config.orders.len(),
                        iteration
                    ))
                    .await;
                    match diagnosis {
                        TakeOrdersFailureDiagnosis::Recovered => {
                            log_selected_legs(
                                best_raindex,
                                block_number,
                                &built.sim.legs,
                                "final take-orders transaction leg",
                            );
                            info!(
                                result_type = "take_orders",
                                orders_count,
                                candidates_count,
                                selected_raindex = %best_raindex,
                                selected_legs_count = built.sim.legs.len(),
                                block_number,
                                preflight_iterations = iteration + 1,
                                removed_orders_count,
                                skipped_raindexes_count,
                                duration_ms = started_at.elapsed_ms(),
                                "take-orders calldata ready after recovered preflight"
                            );
                            return result::build_calldata_result(
                                best_raindex,
                                built,
                                req.mode,
                                req.price_cap,
                            );
                        }
                        TakeOrdersFailureDiagnosis::FailingOrder(failing_idx) => {
                            let failed_raindex = best_raindex;
                            let failing_leg =
                                built.sim.legs.get(failing_idx).ok_or_else(|| {
                                    RaindexError::PreflightError(format!(
                                        "failing order index {failing_idx} has no matching simulation leg"
                                    ))
                                })?;
                            warn_selected_leg(
                                failing_idx,
                                failing_leg,
                                best_raindex,
                                block_number,
                                "removed failing preflight leg",
                            );

                            removed_orders_count += 1;
                            let rebuilt = rebuild_without_failing_leg(
                                &mut remaining_candidates,
                                failing_leg,
                                req.mode,
                                req.price_cap,
                            )?;
                            let Some((rebuilt_raindex, rebuilt)) = rebuilt else {
                                error!(
                                    raindex = %best_raindex,
                                    iteration,
                                    failing_order_index = failing_idx,
                                    error = %truncate_error(&sim_error),
                                    "all orders failed preflight simulation"
                                );
                                return Err(RaindexError::PreflightError(format!(
                                    "All orders failed simulation. Last error: {}",
                                    sim_error
                                )));
                            };
                            best_raindex = rebuilt_raindex;
                            built = rebuilt;
                            warn!(
                                failed_raindex = %failed_raindex,
                                selected_raindex = %best_raindex,
                                iteration,
                                failing_order_index = failing_idx,
                                orders_remaining = built.config.orders.len(),
                                candidates_remaining = remaining_candidates.len(),
                                removed_orders_count,
                                "rebuilt take-orders config after removing failing order"
                            );
                            log_selected_legs(
                                best_raindex,
                                block_number,
                                &built.sim.legs,
                                "reselected take-orders candidate leg",
                            );
                        }
                        TakeOrdersFailureDiagnosis::Unidentified => {
                            let failed_raindex = best_raindex;
                            let rebuilt = rebuild_without_raindex(
                                &mut remaining_candidates,
                                failed_raindex,
                                req.mode,
                                req.price_cap,
                            )?;
                            let Some((rebuilt_raindex, rebuilt)) = rebuilt else {
                                error!(
                                    raindex = %failed_raindex,
                                    iteration,
                                    error = %truncate_error(&sim_error),
                                    "preflight failed without identifiable order or fallback raindex"
                                );
                                return Err(RaindexError::PreflightError(format!(
                                    "Simulation failed but could not identify failing order: {}",
                                    sim_error
                                )));
                            };
                            skipped_raindexes_count += 1;
                            best_raindex = rebuilt_raindex;
                            built = rebuilt;
                            warn!(
                                failed_raindex = %failed_raindex,
                                selected_raindex = %best_raindex,
                                iteration,
                                error = %truncate_error(&sim_error),
                                candidates_remaining = remaining_candidates.len(),
                                skipped_raindexes_count,
                                "skipped raindex after aggregate preflight failure"
                            );
                            log_selected_legs(
                                best_raindex,
                                block_number,
                                &built.sim.legs,
                                "reselected take-orders candidate leg",
                            );
                        }
                    }
                }
            }
        }

        error!(
            raindex = %best_raindex,
            max_iterations,
            removed_orders_count,
            skipped_raindexes_count,
            "exceeded maximum preflight iterations"
        );
            Err(RaindexError::PreflightError(
                "Exceeded maximum preflight iterations".to_string(),
            ))
        }
        .instrument(span)
        .await;
        result
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_family = "wasm"))]
    mod native_tests {
        use super::super::*;
        use crate::take_orders::{simulate_candidates, TakeOrdersMode};
        use crate::test_helpers::candidates::make_candidate;
        use alloy::primitives::{Address, Bytes, FixedBytes, U256};
        use rain_math_float::Float;
        use raindex_bindings::IRaindexV7::SignedContextV2;

        fn float(value: &str) -> Float {
            Float::parse(value.to_string()).unwrap()
        }

        fn candidate(max_output: &str, ratio: &str, nonce: u64) -> TakeOrderCandidate {
            let mut candidate =
                make_candidate(Address::from([0x11u8; 20]), float(max_output), float(ratio));
            candidate.order.nonce = U256::from(nonce).into();
            candidate
        }

        fn mode(mode: TakeOrdersMode, amount: &str) -> ParsedTakeOrdersMode {
            ParsedTakeOrdersMode {
                mode,
                amount: float(amount),
            }
        }

        fn signed_context(value: u8) -> SignedContextV2 {
            SignedContextV2 {
                signer: Address::from([value; 20]),
                domainSeparator: alloy::primitives::B256::repeat_byte(0xd5),
                context: vec![FixedBytes::<32>::from(
                    U256::from(value).to_be_bytes::<32>(),
                )],
                signature: Bytes::from(vec![value]),
            }
        }

        #[test]
        fn rebuild_updates_partial_spend_totals_after_removal() {
            let candidates = vec![candidate("5", "1", 1), candidate("7.5", "2", 2)];
            let mode = mode(TakeOrdersMode::SpendUpTo, "20");
            let price_cap = float("10");
            let initial = simulate_candidates(candidates.clone(), mode, price_cap).unwrap();
            assert!(initial.total_input.eq(float("20")).unwrap());

            let mut remaining = candidates;
            let rebuilt =
                rebuild_without_failing_leg(&mut remaining, &initial.legs[0], mode, price_cap)
                    .unwrap()
                    .unwrap()
                    .1;

            assert_eq!(rebuilt.sim.legs.len(), 1);
            assert_eq!(rebuilt.config.orders.len(), 1);
            assert!(rebuilt.sim.total_input.eq(float("15")).unwrap());
            assert!(rebuilt.sim.total_output.eq(float("7.5")).unwrap());

            let result = result::build_calldata_result(
                Address::from([0x11u8; 20]),
                rebuilt,
                mode,
                price_cap,
            )
            .unwrap();
            assert!(result
                .take_orders_info()
                .unwrap()
                .expected_sell()
                .eq(float("15"))
                .unwrap());
        }

        #[test]
        fn rebuild_reports_incident_executable_total_after_removal() {
            let failing_output =
                "0.9190873838009207986009475041066889213119489421507552037575367421589";
            let failing_ratio =
                "0.007502722698831713208597944445410618026068357798898643535155918523172";
            let executable_output =
                "4.142551979932675829857451653658053577877751406982938626050220986676";
            let executable_ratio =
                "0.010405262850569590820343337005442828611770549820812206944645896242997";
            let candidates = vec![
                candidate(failing_output, failing_ratio, 1),
                candidate(executable_output, executable_ratio, 2),
            ];
            let mode = mode(TakeOrdersMode::SpendUpTo, "0.05");
            let price_cap =
                float("0.010507140312878644839541586318708372026337414326508889434455784294694");
            let initial = simulate_candidates(candidates.clone(), mode, price_cap).unwrap();
            assert!(initial.total_input.lt(float("0.05")).unwrap());
            assert!(initial.total_input.gt(float("0.049")).unwrap());

            let mut remaining = candidates;
            let rebuilt =
                rebuild_without_failing_leg(&mut remaining, &initial.legs[0], mode, price_cap)
                    .unwrap()
                    .unwrap()
                    .1;
            let rebuilt_total_input = rebuilt.sim.total_input;
            let result = result::build_calldata_result(
                Address::from([0x11u8; 20]),
                rebuilt,
                mode,
                price_cap,
            )
            .unwrap();
            let expected_sell = result.take_orders_info().unwrap().expected_sell();

            assert!(expected_sell.eq(rebuilt_total_input).unwrap());
            assert!(expected_sell.gt(float("0.043")).unwrap());
            assert!(expected_sell.lt(float("0.044")).unwrap());
        }

        #[test]
        fn rebuild_selects_previously_unused_fallback_candidate() {
            let candidates = vec![candidate("20", "1", 1), candidate("20", "2", 2)];
            let mode = mode(TakeOrdersMode::SpendUpTo, "20");
            let price_cap = float("10");
            let initial = simulate_candidates(candidates.clone(), mode, price_cap).unwrap();
            assert_eq!(initial.legs.len(), 1);

            let mut remaining = candidates;
            let rebuilt =
                rebuild_without_failing_leg(&mut remaining, &initial.legs[0], mode, price_cap)
                    .unwrap()
                    .unwrap()
                    .1;

            assert_eq!(rebuilt.sim.legs.len(), 1);
            assert!(rebuilt.sim.total_input.eq(float("20")).unwrap());
            assert!(rebuilt.sim.total_output.eq(float("10")).unwrap());
            assert!(rebuilt.sim.legs[0].candidate.ratio.eq(float("2")).unwrap());
        }

        #[test]
        fn rebuild_removes_only_candidate_with_matching_signed_context() {
            let mut failing = candidate("20", "1", 1);
            failing.signed_context = vec![signed_context(1)];
            let mut fallback = failing.clone();
            fallback.ratio = float("2");
            fallback.signed_context = vec![signed_context(2)];
            let candidates = vec![failing, fallback];
            let mode = mode(TakeOrdersMode::SpendUpTo, "20");
            let price_cap = float("10");
            let initial = simulate_candidates(candidates.clone(), mode, price_cap).unwrap();

            let mut remaining = candidates;
            let rebuilt =
                rebuild_without_failing_leg(&mut remaining, &initial.legs[0], mode, price_cap)
                    .unwrap()
                    .unwrap()
                    .1;

            assert_eq!(remaining.len(), 1);
            assert_eq!(remaining[0].signed_context, vec![signed_context(2)]);
            assert_eq!(rebuilt.sim.legs.len(), 1);
            assert!(rebuilt.sim.legs[0].candidate.ratio.eq(float("2")).unwrap());
        }

        #[test]
        fn rebuild_removes_only_matching_quote_for_same_executable_order() {
            let failing = candidate("20", "1", 1);
            let mut fallback = failing.clone();
            fallback.max_output = float("10");
            fallback.ratio = float("2");
            let candidates = vec![failing, fallback];
            let mode = mode(TakeOrdersMode::SpendUpTo, "20");
            let price_cap = float("10");
            let initial = simulate_candidates(candidates.clone(), mode, price_cap).unwrap();

            let mut remaining = candidates;
            let (_, rebuilt) =
                rebuild_without_failing_leg(&mut remaining, &initial.legs[0], mode, price_cap)
                    .unwrap()
                    .unwrap();

            assert_eq!(remaining.len(), 1);
            assert!(remaining[0].max_output.eq(float("10")).unwrap());
            assert!(remaining[0].ratio.eq(float("2")).unwrap());
            assert_eq!(rebuilt.sim.legs.len(), 1);
        }

        #[test]
        fn rebuild_updates_partial_buy_totals_after_removal() {
            let candidates = vec![candidate("5", "1", 1), candidate("15", "2", 2)];
            let mode = mode(TakeOrdersMode::BuyUpTo, "20");
            let price_cap = float("10");
            let initial = simulate_candidates(candidates.clone(), mode, price_cap).unwrap();
            assert!(initial.total_output.eq(float("20")).unwrap());

            let mut remaining = candidates;
            let rebuilt =
                rebuild_without_failing_leg(&mut remaining, &initial.legs[0], mode, price_cap)
                    .unwrap()
                    .unwrap()
                    .1;

            assert!(rebuilt.sim.total_input.eq(float("30")).unwrap());
            assert!(rebuilt.sim.total_output.eq(float("15")).unwrap());
        }

        #[test]
        fn rebuild_reselects_across_raindexes_after_failure() {
            let failing_raindex = Address::from([0x11u8; 20]);
            let fallback_raindex = Address::from([0x22u8; 20]);
            let mut failing = candidate("20", "1", 1);
            failing.raindex = failing_raindex;
            let mut fallback = candidate("20", "2", 2);
            fallback.raindex = fallback_raindex;
            let candidates = vec![failing, fallback];
            let mode = mode(TakeOrdersMode::SpendUpTo, "20");
            let price_cap = float("10");
            let (initial_raindex, initial) =
                selection::select_best_raindex_simulation(candidates.clone(), mode, price_cap)
                    .unwrap();
            assert_eq!(initial_raindex, failing_raindex);

            let mut remaining = candidates;
            let (rebuilt_raindex, rebuilt) =
                rebuild_without_failing_leg(&mut remaining, &initial.legs[0], mode, price_cap)
                    .unwrap()
                    .unwrap();

            assert_eq!(rebuilt_raindex, fallback_raindex);
            assert_eq!(rebuilt.sim.legs.len(), 1);
            assert!(rebuilt.sim.total_input.eq(float("20")).unwrap());
            assert!(rebuilt.sim.total_output.eq(float("10")).unwrap());
        }

        #[test]
        fn rebuild_exact_spend_uses_healthy_leg_and_unused_fallback() {
            let candidates = vec![
                candidate("4", "1", 1),
                candidate("4", "1.5", 2),
                candidate("3", "2", 3),
            ];
            let mode = mode(TakeOrdersMode::SpendExact, "10");
            let price_cap = float("10");
            let initial = simulate_candidates(candidates.clone(), mode, price_cap).unwrap();
            assert_eq!(initial.legs.len(), 2);
            assert!(initial.total_input.eq(float("10")).unwrap());

            let mut remaining = candidates;
            let (_, rebuilt) =
                rebuild_without_failing_leg(&mut remaining, &initial.legs[1], mode, price_cap)
                    .unwrap()
                    .unwrap();

            assert_eq!(rebuilt.sim.legs.len(), 2);
            assert!(rebuilt.sim.total_input.eq(float("10")).unwrap());
            assert!(rebuilt.sim.legs[0].candidate.ratio.eq(float("1")).unwrap());
            assert!(rebuilt.sim.legs[1].candidate.ratio.eq(float("2")).unwrap());
        }

        #[test]
        fn rebuild_skips_unidentifiable_raindex_and_selects_another() {
            let failed_raindex = Address::from([0x11u8; 20]);
            let fallback_raindex = Address::from([0x22u8; 20]);
            let mut failed_a = candidate("5", "1", 1);
            failed_a.raindex = failed_raindex;
            let mut failed_b = candidate("5", "2", 2);
            failed_b.raindex = failed_raindex;
            let mut fallback = candidate("10", "3", 3);
            fallback.raindex = fallback_raindex;
            let mut remaining = vec![failed_a, failed_b, fallback];
            let mode = mode(TakeOrdersMode::SpendUpTo, "10");
            let price_cap = float("10");

            let (selected, rebuilt) =
                rebuild_without_raindex(&mut remaining, failed_raindex, mode, price_cap)
                    .unwrap()
                    .unwrap();

            assert_eq!(selected, fallback_raindex);
            assert_eq!(remaining.len(), 1);
            assert!(remaining
                .iter()
                .all(|candidate| candidate.raindex == fallback_raindex));
            assert_eq!(rebuilt.config.orders.len(), 1);
        }

        #[test]
        fn rebuild_rejects_exact_mode_when_remaining_liquidity_is_insufficient() {
            let candidates = vec![candidate("5", "1", 1), candidate("7.5", "2", 2)];
            let mode = mode(TakeOrdersMode::SpendExact, "20");
            let price_cap = float("10");
            let initial = simulate_candidates(candidates.clone(), mode, price_cap).unwrap();

            let mut remaining = candidates;
            let result =
                rebuild_without_failing_leg(&mut remaining, &initial.legs[0], mode, price_cap);

            assert!(matches!(
                result,
                Err(RaindexError::InsufficientLiquidity {
                    requested,
                    available
                }) if requested == "20" && available == "15"
            ));
        }
    }

    #[cfg(target_family = "wasm")]
    mod wasm_tests {
        use crate::take_orders::TakeOrdersMode;
        use wasm_bindgen_test::wasm_bindgen_test;
        use wasm_bindgen_utils::prelude::{from_js_value, to_js_value};

        #[wasm_bindgen_test]
        fn test_take_orders_mode_serialization() {
            let buy_up_to = TakeOrdersMode::BuyUpTo;
            let buy_exact = TakeOrdersMode::BuyExact;
            let spend_up_to = TakeOrdersMode::SpendUpTo;
            let spend_exact = TakeOrdersMode::SpendExact;

            let buy_up_to_js = to_js_value(&buy_up_to).unwrap();
            let buy_exact_js = to_js_value(&buy_exact).unwrap();
            let spend_up_to_js = to_js_value(&spend_up_to).unwrap();
            let spend_exact_js = to_js_value(&spend_exact).unwrap();

            let buy_up_to_back: TakeOrdersMode = from_js_value(buy_up_to_js).unwrap();
            let buy_exact_back: TakeOrdersMode = from_js_value(buy_exact_js).unwrap();
            let spend_up_to_back: TakeOrdersMode = from_js_value(spend_up_to_js).unwrap();
            let spend_exact_back: TakeOrdersMode = from_js_value(spend_exact_js).unwrap();

            assert_eq!(buy_up_to_back, TakeOrdersMode::BuyUpTo);
            assert_eq!(buy_exact_back, TakeOrdersMode::BuyExact);
            assert_eq!(spend_up_to_back, TakeOrdersMode::SpendUpTo);
            assert_eq!(spend_exact_back, TakeOrdersMode::SpendExact);
        }
    }
}
