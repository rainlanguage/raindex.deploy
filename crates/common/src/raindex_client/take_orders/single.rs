use crate::raindex_client::order_quotes::{RaindexOrderQuote, RaindexOrderQuoteValue};
use crate::raindex_client::orders::RaindexOrder;
use crate::raindex_client::RaindexError;
use crate::take_orders::simulation::{SelectedTakeOrderLeg, SimulationResult};
use crate::take_orders::{
    build_take_orders_config_from_simulation, find_failing_order_index, simulate_take_orders,
    ParsedTakeOrdersMode, TakeOrderCandidate,
};
use alloy::primitives::Address;
use rain_math_float::Float;
use raindex_bindings::provider::mk_read_provider;
use raindex_bindings::IRaindexV7::OrderV4;
use std::ops::{Div, Mul};
#[cfg(target_family = "wasm")]
use std::str::FromStr;
use url::Url;

use super::approval::{check_approval_needed, ApprovalCheckParams};
use super::result::{build_calldata_result, TakeOrderEstimate, TakeOrdersCalldataResult};

/// Parameters for executing a take order.
#[derive(Debug, Clone)]
pub struct TakeOrderExecutionParams {
    pub mode: ParsedTakeOrdersMode,
    pub price_cap: Float,
    pub taker: Address,
    pub sell_token: Address,
}

/// RPC context for blockchain interactions.
#[derive(Debug, Clone)]
pub struct RpcContext<'a> {
    pub rpc_urls: &'a [Url],
    pub block_number: Option<u64>,
}

pub fn build_candidate_from_quote(
    order: &RaindexOrder,
    quote: &RaindexOrderQuote,
) -> Result<Option<TakeOrderCandidate>, RaindexError> {
    if !quote.success {
        return Ok(None);
    }

    let data = match &quote.data {
        Some(d) => d,
        None => return Ok(None),
    };

    if !has_capacity(data)? {
        return Ok(None);
    }

    #[cfg(target_family = "wasm")]
    let raindex_addr = Address::from_str(&order.raindex())?;
    #[cfg(not(target_family = "wasm"))]
    let raindex_addr = order.raindex();
    let order_v4: OrderV4 = order.try_into()?;
    let input_io_index = quote.pair.input_index;
    let output_io_index = quote.pair.output_index;

    if !indices_in_bounds(&order_v4, input_io_index, output_io_index) {
        return Ok(None);
    }

    Ok(Some(TakeOrderCandidate {
        raindex: raindex_addr,
        order: order_v4,
        input_io_index,
        output_io_index,
        max_output: data.max_output,
        ratio: data.ratio,
        // Inherit the composed signed context that the quote pipeline
        // attached (oracle entries first, then any injector-contributed
        // entries). Dropping it here was a regression: gated orders whose
        // `calculate-io` asserted on signed context during quoting would
        // then be executed without the same context at `takeOrders5` time.
        signed_context: quote.signed_context.clone(),
    }))
}

fn indices_in_bounds(order: &OrderV4, input_index: u32, output_index: u32) -> bool {
    (input_index as usize) < order.validInputs.len()
        && (output_index as usize) < order.validOutputs.len()
}

fn has_capacity(data: &RaindexOrderQuoteValue) -> Result<bool, RaindexError> {
    Ok(data.max_output.gt(Float::zero()?)?)
}

pub fn estimate_take_order(
    max_output: Float,
    ratio: Float,
    is_buy: bool,
    amount: Float,
) -> Result<TakeOrderEstimate, RaindexError> {
    let zero = Float::zero()?;

    if amount.lte(zero)? {
        return Ok(TakeOrderEstimate::new(zero, zero, false));
    }

    let (expected_receive, expected_spend, is_partial) = if is_buy {
        let is_partial = max_output.lt(amount)?;
        let output = if is_partial { max_output } else { amount };
        let input = output.mul(ratio)?;
        (output, input, is_partial)
    } else {
        let max_input = max_output.mul(ratio)?;
        let is_partial = max_input.lt(amount)?;
        let input = if is_partial { max_input } else { amount };
        let output = if ratio.eq(zero)? {
            max_output
        } else {
            input.div(ratio)?
        };
        (output, input, is_partial)
    };

    Ok(TakeOrderEstimate::new(
        expected_spend,
        expected_receive,
        is_partial,
    ))
}

pub async fn execute_single_take(
    candidate: TakeOrderCandidate,
    execution_params: TakeOrderExecutionParams,
    rpc_context: RpcContext<'_>,
) -> Result<TakeOrdersCalldataResult, RaindexError> {
    let zero = Float::zero()?;

    if candidate.ratio.gt(execution_params.price_cap)? {
        return Err(RaindexError::NoLiquidity);
    }

    let raindex_addr = candidate.raindex;

    let approval_params = ApprovalCheckParams {
        rpc_urls: rpc_context.rpc_urls.to_vec(),
        sell_token: execution_params.sell_token,
        taker: execution_params.taker,
        raindex: raindex_addr,
        mode: execution_params.mode,
        price_cap: execution_params.price_cap,
    };

    if let Some(approval_result) = check_approval_needed(&approval_params).await? {
        return Ok(approval_result);
    }

    // Signed context (oracle + injector) is already attached to the
    // candidate by `build_candidate_from_quote`, which inherits it from
    // the quote. No extra fetch is needed at execute time.
    let target = execution_params.mode.target_amount();
    let is_buy_mode = execution_params.mode.is_buy_mode();

    let (output, input) = if is_buy_mode {
        let output = if candidate.max_output.lte(target)? {
            candidate.max_output
        } else {
            target
        };
        let input = output.mul(candidate.ratio)?;
        (output, input)
    } else {
        let max_input = candidate.max_output.mul(candidate.ratio)?;
        let input = if max_input.lte(target)? {
            max_input
        } else {
            target
        };
        let output = if candidate.ratio.eq(zero)? {
            candidate.max_output
        } else {
            input.div(candidate.ratio)?
        };
        (output, input)
    };

    if output.lte(zero)? {
        return Err(RaindexError::NoLiquidity);
    }

    let sim = SimulationResult {
        legs: vec![SelectedTakeOrderLeg {
            candidate: candidate.clone(),
            input,
            output,
        }],
        total_input: input,
        total_output: output,
    };

    let built = build_take_orders_config_from_simulation(
        sim,
        execution_params.mode,
        execution_params.price_cap,
    )?
    .ok_or(RaindexError::NoLiquidity)?;

    let provider = mk_read_provider(rpc_context.rpc_urls)
        .map_err(|e| RaindexError::PreflightError(e.to_string()))?;

    let sim_result = simulate_take_orders(
        &provider,
        raindex_addr,
        execution_params.taker,
        &built.config,
        rpc_context.block_number,
    )
    .await;

    match sim_result {
        Ok(()) => build_calldata_result(
            raindex_addr,
            built,
            execution_params.mode,
            execution_params.price_cap,
        ),
        Err(sim_error) => {
            if built.config.orders.len() == 1 {
                Err(RaindexError::PreflightError(format!(
                    "Order failed simulation: {}",
                    sim_error
                )))
            } else if let Some(_failing_idx) = find_failing_order_index(
                &provider,
                raindex_addr,
                execution_params.taker,
                &built.config,
                rpc_context.block_number,
            )
            .await
            {
                Err(RaindexError::PreflightError(format!(
                    "Order failed simulation: {}",
                    sim_error
                )))
            } else {
                Err(RaindexError::PreflightError(format!(
                    "Simulation failed: {}",
                    sim_error
                )))
            }
        }
    }
}

#[cfg(test)]
#[cfg(not(target_family = "wasm"))]
mod tests {
    use super::*;
    use crate::raindex_client::order_quotes::{RaindexOrderQuote, RaindexOrderQuoteValue};
    use rain_math_float::Float;
    use raindex_quote::Pair;
    use std::ops::Mul;

    fn make_quote_value(max_output: Float, ratio: Float) -> RaindexOrderQuoteValue {
        let max_input = max_output.mul(ratio).unwrap();
        RaindexOrderQuoteValue {
            max_output,
            formatted_max_output: max_output.format().unwrap(),
            max_input,
            formatted_max_input: max_input.format().unwrap(),
            ratio,
            formatted_ratio: ratio.format().unwrap(),
            inverse_ratio: ratio,
            formatted_inverse_ratio: ratio.format().unwrap(),
            formatted_max_output_as_percent_of_vault: None,
        }
    }

    fn make_quote(
        input_index: u32,
        output_index: u32,
        data: Option<RaindexOrderQuoteValue>,
        success: bool,
    ) -> RaindexOrderQuote {
        RaindexOrderQuote {
            pair: Pair {
                pair_name: "A/B".to_string(),
                input_index,
                output_index,
            },
            block_number: 1,
            data,
            success,
            signed_context: vec![],
            error: if success {
                None
            } else {
                Some("Quote failed".to_string())
            },
        }
    }

    #[test]
    fn test_indices_in_bounds() {
        use crate::test_helpers::orders::make_basic_order;
        use alloy::primitives::Address;

        let order = make_basic_order(Address::from([4u8; 20]), Address::from([5u8; 20]));

        assert!(indices_in_bounds(&order, 0, 0));
        assert!(!indices_in_bounds(&order, 1, 0));
        assert!(!indices_in_bounds(&order, 0, 1));
        assert!(!indices_in_bounds(&order, 99, 99));
    }

    #[test]
    fn test_has_capacity_positive() {
        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("1".to_string()).unwrap();
        let data = make_quote_value(max_output, ratio);
        assert!(has_capacity(&data).unwrap());
    }

    #[test]
    fn test_has_capacity_zero() {
        let max_output = Float::zero().unwrap();
        let ratio = Float::parse("1".to_string()).unwrap();
        let data = make_quote_value(max_output, ratio);
        assert!(!has_capacity(&data).unwrap());
    }

    #[test]
    fn test_has_capacity_negative() {
        let max_output = Float::parse("-1".to_string()).unwrap();
        let ratio = Float::parse("1".to_string()).unwrap();
        let data = make_quote_value(max_output, ratio);
        assert!(!has_capacity(&data).unwrap());
    }

    #[test]
    fn test_quote_failed_returns_none() {
        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("1".to_string()).unwrap();
        let quote = make_quote(0, 0, Some(make_quote_value(max_output, ratio)), false);

        use crate::local_db::RaindexIdentifier;
        use crate::raindex_client::tests::{get_test_yaml, CHAIN_ID_1_RAINDEX_ADDRESS};
        use alloy::primitives::Address;
        use std::str::FromStr;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use alloy::primitives::b256;
            use httpmock::MockServer;
            use raindex_subgraph_client::utils::float::F1;
            use serde_json::json;

            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.path("/sg");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [{
                            "id": "0x46891c626a8a188610b902ee4a0ce8a7e81915e1b922584f8168d14525899dfb",
                            "orderBytes": "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000005f6c104ca9812ef91fe2e26a2e7187b92d3b0e800000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000022009cd210f509c66e18fab61fd30f76fb17c6c6cd09f0972ce0815b5b7630a1b050000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000075000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015020000000c02020002011000000110000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372",
                            "orderHash": "0x283508c8f56f4de2f21ee91749d64ec3948c16bc6b4bfe4f8d11e4e67d76f4e0",
                            "owner": "0x0000000000000000000000000000000000000000",
                            "outputs": [{
                                "id": "0x0000000000000000000000000000000000000000",
                                "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                                "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                                "balance": F1,
                                "token": {
                                    "id": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                                    "address": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                                    "name": "sFLR",
                                    "symbol": "sFLR",
                                    "decimals": "18"
                                },
                                "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                                "ordersAsOutput": [],
                                "ordersAsInput": [],
                                "balanceChanges": []
                            }],
                            "inputs": [{
                                "id": "0x0000000000000000000000000000000000000000",
                                "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                                "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                                "balance": F1,
                                "token": {
                                    "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                    "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                    "name": "WFLR",
                                    "symbol": "WFLR",
                                    "decimals": "18"
                                },
                                "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                                "ordersAsOutput": [],
                                "ordersAsInput": [],
                                "balanceChanges": []
                            }],
                            "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                            "active": true,
                            "timestampAdded": "1739448802",
                            "meta": null,
                            "addEvents": [],
                            "trades": [],
                            "removeEvents": []
                        }]
                    }
                }));
            });

            let raindex_client = crate::raindex_client::RaindexClient::new(
                vec![get_test_yaml(
                    &server.url("/sg"),
                    "http://localhost:3000",
                    "http://localhost:3000",
                    "http://localhost:3000",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let order = raindex_client
                .get_order_by_hash(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000123"),
                )
                .await
                .unwrap();

            let result = build_candidate_from_quote(&order, &quote).unwrap();
            assert!(result.is_none(), "Failed quote should return None");
        });
    }

    #[test]
    fn test_quote_no_data_returns_none() {
        let quote = make_quote(0, 0, None, true);

        use crate::local_db::RaindexIdentifier;
        use crate::raindex_client::tests::{get_test_yaml, CHAIN_ID_1_RAINDEX_ADDRESS};
        use alloy::primitives::Address;
        use std::str::FromStr;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use alloy::primitives::b256;
            use httpmock::MockServer;
            use raindex_subgraph_client::utils::float::F1;
            use serde_json::json;

            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.path("/sg");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [{
                            "id": "0x46891c626a8a188610b902ee4a0ce8a7e81915e1b922584f8168d14525899dfb",
                            "orderBytes": "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000005f6c104ca9812ef91fe2e26a2e7187b92d3b0e800000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000022009cd210f509c66e18fab61fd30f76fb17c6c6cd09f0972ce0815b5b7630a1b050000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000075000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015020000000c02020002011000000110000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372",
                            "orderHash": "0x283508c8f56f4de2f21ee91749d64ec3948c16bc6b4bfe4f8d11e4e67d76f4e0",
                            "owner": "0x0000000000000000000000000000000000000000",
                            "outputs": [{
                                "id": "0x0000000000000000000000000000000000000000",
                                "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                                "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                                "balance": F1,
                                "token": {
                                    "id": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                                    "address": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                                    "name": "sFLR",
                                    "symbol": "sFLR",
                                    "decimals": "18"
                                },
                                "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                                "ordersAsOutput": [],
                                "ordersAsInput": [],
                                "balanceChanges": []
                            }],
                            "inputs": [{
                                "id": "0x0000000000000000000000000000000000000000",
                                "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                                "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                                "balance": F1,
                                "token": {
                                    "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                    "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                    "name": "WFLR",
                                    "symbol": "WFLR",
                                    "decimals": "18"
                                },
                                "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                                "ordersAsOutput": [],
                                "ordersAsInput": [],
                                "balanceChanges": []
                            }],
                            "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                            "active": true,
                            "timestampAdded": "1739448802",
                            "meta": null,
                            "addEvents": [],
                            "trades": [],
                            "removeEvents": []
                        }]
                    }
                }));
            });

            let raindex_client = crate::raindex_client::RaindexClient::new(
                vec![get_test_yaml(
                    &server.url("/sg"),
                    "http://localhost:3000",
                    "http://localhost:3000",
                    "http://localhost:3000",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let order = raindex_client
                .get_order_by_hash(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000123"),
                )
                .await
                .unwrap();

            let result = build_candidate_from_quote(&order, &quote).unwrap();
            assert!(result.is_none(), "Quote with no data should return None");
        });
    }

    #[test]
    fn test_quote_zero_capacity_returns_none() {
        let max_output = Float::zero().unwrap();
        let ratio = Float::parse("1".to_string()).unwrap();
        let quote = make_quote(0, 0, Some(make_quote_value(max_output, ratio)), true);

        use crate::local_db::RaindexIdentifier;
        use crate::raindex_client::tests::{get_test_yaml, CHAIN_ID_1_RAINDEX_ADDRESS};
        use alloy::primitives::Address;
        use std::str::FromStr;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use alloy::primitives::b256;
            use httpmock::MockServer;
            use raindex_subgraph_client::utils::float::F1;
            use serde_json::json;

            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.path("/sg");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [{
                            "id": "0x46891c626a8a188610b902ee4a0ce8a7e81915e1b922584f8168d14525899dfb",
                            "orderBytes": "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000005f6c104ca9812ef91fe2e26a2e7187b92d3b0e800000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000022009cd210f509c66e18fab61fd30f76fb17c6c6cd09f0972ce0815b5b7630a1b050000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000075000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015020000000c02020002011000000110000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372",
                            "orderHash": "0x283508c8f56f4de2f21ee91749d64ec3948c16bc6b4bfe4f8d11e4e67d76f4e0",
                            "owner": "0x0000000000000000000000000000000000000000",
                            "outputs": [{
                                "id": "0x0000000000000000000000000000000000000000",
                                "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                                "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                                "balance": F1,
                                "token": {
                                    "id": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                                    "address": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                                    "name": "sFLR",
                                    "symbol": "sFLR",
                                    "decimals": "18"
                                },
                                "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                                "ordersAsOutput": [],
                                "ordersAsInput": [],
                                "balanceChanges": []
                            }],
                            "inputs": [{
                                "id": "0x0000000000000000000000000000000000000000",
                                "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                                "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                                "balance": F1,
                                "token": {
                                    "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                    "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                    "name": "WFLR",
                                    "symbol": "WFLR",
                                    "decimals": "18"
                                },
                                "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                                "ordersAsOutput": [],
                                "ordersAsInput": [],
                                "balanceChanges": []
                            }],
                            "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                            "active": true,
                            "timestampAdded": "1739448802",
                            "meta": null,
                            "addEvents": [],
                            "trades": [],
                            "removeEvents": []
                        }]
                    }
                }));
            });

            let raindex_client = crate::raindex_client::RaindexClient::new(
                vec![get_test_yaml(
                    &server.url("/sg"),
                    "http://localhost:3000",
                    "http://localhost:3000",
                    "http://localhost:3000",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let order = raindex_client
                .get_order_by_hash(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000123"),
                )
                .await
                .unwrap();

            let result = build_candidate_from_quote(&order, &quote).unwrap();
            assert!(
                result.is_none(),
                "Quote with zero capacity should return None"
            );
        });
    }

    /// Regression: a quote whose `signed_context` is populated (as produced
    /// by the injector-aware quote pipeline) must propagate that context into
    /// the candidate verbatim. Dropping it here previously caused gated
    /// single-takes to execute `takeOrders5` without the context that
    /// `calculate-io` saw at quote time.
    #[test]
    fn test_build_candidate_inherits_signed_context_from_quote() {
        use crate::local_db::RaindexIdentifier;
        use crate::raindex_client::tests::{get_test_yaml, CHAIN_ID_1_RAINDEX_ADDRESS};
        use alloy::primitives::{b256, Address, Bytes, FixedBytes, U256};
        use httpmock::MockServer;
        use raindex_bindings::IRaindexV7::SignedContextV2;
        use raindex_subgraph_client::utils::float::F1;
        use serde_json::json;
        use std::str::FromStr;

        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("1".to_string()).unwrap();

        let oracle_entry = SignedContextV2 {
            signer: Address::from([0xAAu8; 20]),
            domainSeparator: alloy::primitives::B256::repeat_byte(0xd5),
            context: vec![FixedBytes::<32>::from(U256::from(1u64).to_be_bytes::<32>())],
            signature: Bytes::from(vec![0x01]),
        };
        let injected_entry = SignedContextV2 {
            signer: Address::from([0xBBu8; 20]),
            domainSeparator: alloy::primitives::B256::repeat_byte(0xd5),
            context: vec![FixedBytes::<32>::from(U256::from(2u64).to_be_bytes::<32>())],
            signature: Bytes::from(vec![0x02]),
        };

        let mut quote = make_quote(0, 0, Some(make_quote_value(max_output, ratio)), true);
        quote.signed_context = vec![oracle_entry.clone(), injected_entry.clone()];

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.path("/sg");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [{
                            "id": "0x46891c626a8a188610b902ee4a0ce8a7e81915e1b922584f8168d14525899dfb",
                            "orderBytes": "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000005f6c104ca9812ef91fe2e26a2e7187b92d3b0e800000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000022009cd210f509c66e18fab61fd30f76fb17c6c6cd09f0972ce0815b5b7630a1b050000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000075000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015020000000c02020002011000000110000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372",
                            "orderHash": "0x283508c8f56f4de2f21ee91749d64ec3948c16bc6b4bfe4f8d11e4e67d76f4e0",
                            "owner": "0x0000000000000000000000000000000000000000",
                            "outputs": [{
                                "id": "0x0000000000000000000000000000000000000000",
                                "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                                "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                                "balance": F1,
                                "token": {
                                    "id": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                                    "address": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                                    "name": "sFLR",
                                    "symbol": "sFLR",
                                    "decimals": "18"
                                },
                                "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                                "ordersAsOutput": [],
                                "ordersAsInput": [],
                                "balanceChanges": []
                            }],
                            "inputs": [{
                                "id": "0x0000000000000000000000000000000000000000",
                                "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                                "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                                "balance": F1,
                                "token": {
                                    "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                    "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                    "name": "WFLR",
                                    "symbol": "WFLR",
                                    "decimals": "18"
                                },
                                "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                                "ordersAsOutput": [],
                                "ordersAsInput": [],
                                "balanceChanges": []
                            }],
                            "raindex": { "id": CHAIN_ID_1_RAINDEX_ADDRESS },
                            "active": true,
                            "timestampAdded": "1739448802",
                            "meta": null,
                            "addEvents": [],
                            "trades": [],
                            "removeEvents": []
                        }]
                    }
                }));
            });

            let raindex_client = crate::raindex_client::RaindexClient::new(
                vec![get_test_yaml(
                    &server.url("/sg"),
                    "http://localhost:3000",
                    "http://localhost:3000",
                    "http://localhost:3000",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let order = raindex_client
                .get_order_by_hash(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000123"),
                )
                .await
                .unwrap();

            let candidate = build_candidate_from_quote(&order, &quote)
                .unwrap()
                .expect("candidate should be built");

            assert_eq!(
                candidate.signed_context.len(),
                2,
                "candidate must inherit both oracle and injected context entries"
            );
            assert_eq!(candidate.signed_context[0].signer, oracle_entry.signer);
            assert_eq!(candidate.signed_context[1].signer, injected_entry.signer);
            assert_eq!(
                candidate.signed_context[0].signature,
                oracle_entry.signature
            );
            assert_eq!(
                candidate.signed_context[1].signature,
                injected_entry.signature
            );
        });
    }
}
