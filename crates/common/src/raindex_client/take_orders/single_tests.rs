use crate::local_db::RaindexIdentifier;
use crate::raindex_client::order_quotes::RaindexOrderQuote;
use crate::raindex_client::take_orders::single::{
    build_candidate_from_quote, estimate_take_order, execute_single_take, RpcContext,
    TakeOrderExecutionParams,
};
use crate::raindex_client::RaindexClient;
use crate::raindex_client::RaindexError;
use crate::take_orders::{ParsedTakeOrdersMode, TakeOrdersMode};
use crate::test_helpers::dotrain::create_dotrain_config_with_params;
use crate::test_helpers::local_evm::{
    create_vault, fund_and_approve_taker, fund_standard_two_token_vault,
    setup_test as base_setup_test,
};
use crate::test_helpers::orders::deploy::deploy_order;
use crate::test_helpers::quotes::{make_quote, make_quote_value};
use crate::test_helpers::subgraph::{create_sg_order_json, get_minimal_yaml_for_chain};
use alloy::network::{ReceiptResponse, TransactionBuilder};
use alloy::primitives::{Address, Bytes, B256, U256};
use alloy::rpc::types::TransactionRequest;
use alloy::serde::WithOtherFields;
use alloy::sol_types::SolCall;
use httpmock::MockServer;
use rain_math_float::Float;
use raindex_bindings::IRaindexV7::takeOrders5Call;
use raindex_quote::Pair;
use serde_json::json;
use std::ops::{Div, Mul};

fn high_price_cap() -> String {
    "1000000".to_string()
}

fn parse_buy_up_to(amount: &str) -> ParsedTakeOrdersMode {
    ParsedTakeOrdersMode {
        mode: TakeOrdersMode::BuyUpTo,
        amount: Float::parse(amount.to_string()).unwrap(),
    }
}

fn parse_buy_exact(amount: &str) -> ParsedTakeOrdersMode {
    ParsedTakeOrdersMode {
        mode: TakeOrdersMode::BuyExact,
        amount: Float::parse(amount.to_string()).unwrap(),
    }
}

fn parse_spend_up_to(amount: &str) -> ParsedTakeOrdersMode {
    ParsedTakeOrdersMode {
        mode: TakeOrdersMode::SpendUpTo,
        amount: Float::parse(amount.to_string()).unwrap(),
    }
}

fn parse_spend_exact(amount: &str) -> ParsedTakeOrdersMode {
    ParsedTakeOrdersMode {
        mode: TakeOrdersMode::SpendExact,
        amount: Float::parse(amount.to_string()).unwrap(),
    }
}

#[tokio::test]
async fn test_single_order_take_happy_path_buy_up_to() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let mode = parse_buy_up_to("50");
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context)
        .await
        .expect("Should succeed with BuyUpTo mode");

    assert!(result.is_ready());
    let result = result.take_orders_info().unwrap();
    assert_eq!(result.raindex(), setup.raindex);
    assert!(!result.calldata().is_empty());
    assert_eq!(
        result.prices().len(),
        1,
        "Should have exactly 1 price entry"
    );

    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert_eq!(config.orders.len(), 1, "Should have exactly 1 order");
    assert!(config.IOIsInput, "IOIsInput should be true for buy mode");

    let expected_ratio = Float::parse("2".to_string()).unwrap();
    assert!(
        result.prices()[0].eq(expected_ratio).unwrap(),
        "Price should match order ratio of 2"
    );
}

#[tokio::test]
async fn test_single_order_take_happy_path_buy_exact() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let buy_target = "50";
    let mode = parse_buy_exact(buy_target);
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context)
        .await
        .expect("Should succeed with BuyExact mode");

    assert!(result.is_ready());
    let result = result.take_orders_info().unwrap();
    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    let expected_buy = Float::parse(buy_target.to_string()).unwrap().get_inner();

    assert_eq!(
        config.minimumIO, expected_buy,
        "minimumIO should equal buy_target for BuyExact mode"
    );
    assert_eq!(
        config.maximumIO, expected_buy,
        "maximumIO should equal buy_target for BuyExact mode"
    );
}

#[tokio::test]
async fn test_single_order_take_happy_path_spend_up_to() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let mode = parse_spend_up_to("100");
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context)
        .await
        .expect("Should succeed with SpendUpTo mode");

    assert!(result.is_ready());
    let result = result.take_orders_info().unwrap();
    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert!(
        !config.IOIsInput,
        "IOIsInput should be false for spend mode"
    );
    assert_eq!(
        config.minimumIO,
        Float::zero().unwrap().get_inner(),
        "minimumIO should be zero for SpendUpTo mode"
    );
}

#[tokio::test]
async fn test_single_order_take_no_capacity_returns_error() {
    let max_output = Float::zero().unwrap();
    let ratio = Float::parse("1".to_string()).unwrap();
    let max_input = Float::zero().unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "1");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let result = build_candidate_from_quote(&order, &quote).unwrap();
    assert!(
        result.is_none(),
        "Quote with zero capacity should return None"
    );
}

#[tokio::test]
async fn test_single_order_take_invalid_io_index_returns_none() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = RaindexOrderQuote {
        pair: Pair {
            pair_name: "A/B".to_string(),
            input_index: 99,
            output_index: 99,
        },
        block_number: 1,
        data: Some(make_quote_value(max_output, max_input, ratio)),
        success: true,
        error: None,
        signed_context: vec![],
    };

    let result = build_candidate_from_quote(&order, &quote).unwrap();
    assert!(
        result.is_none(),
        "Quote with out-of-bounds indices should return None"
    );
}

#[tokio::test]
async fn test_single_order_take_buy_exact_insufficient_liquidity() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "50", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let max_output = Float::parse("50".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let mode = parse_buy_exact("100");
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context).await;

    assert!(
        matches!(result, Err(RaindexError::InsufficientLiquidity { .. })),
        "BuyExact mode should return InsufficientLiquidity when amount > available, got: {:?}",
        result
    );
}

#[tokio::test]
async fn test_single_order_take_price_exceeds_cap() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "5");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("5".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let mode = parse_buy_up_to("50");
    let price_cap = Float::parse("2".to_string()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context).await;

    assert!(
        matches!(result, Err(RaindexError::NoLiquidity)),
        "Should return NoLiquidity when order price (5) exceeds price_cap (2), got: {:?}",
        result
    );
}

#[tokio::test]
async fn test_single_order_take_failed_quote_returns_none() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let quote = make_quote(0, 1, None, false);

    let result = build_candidate_from_quote(&order, &quote).unwrap();
    assert!(result.is_none(), "Failed quote should return None");
}

#[tokio::test]
async fn test_single_order_take_preflight_insufficient_balance() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let mode = parse_buy_up_to("50");
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context)
        .await
        .expect("Should succeed with approval result");

    assert!(
        result.is_needs_approval(),
        "Should return NeedsApproval when taker has no balance (allowance check happens first), got: {:?}",
        result
    );
    let approval_info = result.approval_info().expect("Should have approval info");
    assert_eq!(approval_info.token(), setup.token1);
    assert_eq!(approval_info.spender(), setup.raindex);
}

#[tokio::test]
async fn test_single_order_take_preflight_insufficient_allowance() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();

    let token_contract = setup
        .local_evm
        .tokens
        .iter()
        .find(|t| *t.address() == setup.token1)
        .expect("Token should exist");

    let amount = U256::from(10).pow(U256::from(22));
    token_contract
        .transfer(taker, amount)
        .from(setup.owner)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let mode = parse_buy_up_to("50");
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context)
        .await
        .expect("Should succeed with approval result");

    assert!(
        result.is_needs_approval(),
        "Should return NeedsApproval when taker has no allowance, got: {:?}",
        result
    );
    let approval_info = result.approval_info().expect("Should have approval info");
    assert_eq!(approval_info.token(), setup.token1);
    assert_eq!(approval_info.spender(), setup.raindex);
}

#[tokio::test]
async fn test_single_order_take_approval_then_ready_flow() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();

    let token_contract = setup
        .local_evm
        .tokens
        .iter()
        .find(|t| *t.address() == setup.token1)
        .expect("Token should exist");

    let amount = U256::from(10).pow(U256::from(22));
    token_contract
        .transfer(taker, amount)
        .from(setup.owner)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let mode = parse_buy_up_to("50");
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    // Step 1: First call should return NeedsApproval
    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate.clone(), execution_params, rpc_context)
        .await
        .expect("Should succeed with approval result");

    assert!(
        result.is_needs_approval(),
        "First call should return NeedsApproval, got: {:?}",
        result
    );

    let approval_info = result.approval_info().expect("Should have approval info");
    assert_eq!(approval_info.token(), setup.token1);
    assert_eq!(approval_info.spender(), setup.raindex);
    assert!(
        !approval_info.calldata().is_empty(),
        "Approval calldata should not be empty"
    );

    // Step 2: Execute the approval transaction
    let approval_tx = WithOtherFields::new(
        TransactionRequest::default()
            .with_input(Bytes::from(approval_info.calldata().to_vec()))
            .with_to(approval_info.token())
            .with_from(taker),
    );

    let approval_result = setup
        .local_evm
        .send_transaction(approval_tx)
        .await
        .expect("Failed to send approval transaction");

    assert!(
        approval_result.status(),
        "Approval transaction should succeed"
    );

    // Step 3: Second call should return Ready with take order calldata
    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context)
        .await
        .expect("Should succeed with ready result after approval");

    assert!(
        result.is_ready(),
        "Second call after approval should return Ready, got: {:?}",
        result
    );

    let take_info = result
        .take_orders_info()
        .expect("Should have take orders info");
    assert_eq!(take_info.raindex(), setup.raindex);
    assert!(
        !take_info.calldata().is_empty(),
        "Take orders calldata should not be empty"
    );

    // Verify the calldata is valid by decoding it
    let decoded = takeOrders5Call::abi_decode(take_info.calldata())
        .expect("Should decode take orders calldata");
    assert_eq!(
        decoded.config.orders.len(),
        1,
        "Should have exactly 1 order"
    );
}

#[tokio::test]
async fn test_single_order_take_calldata_encoding_buy_mode() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let buy_target = "50";
    let price_cap_str = "10";
    let mode = parse_buy_up_to(buy_target);
    let price_cap = Float::parse(price_cap_str.to_string()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context)
        .await
        .expect("Should succeed");

    assert!(result.is_ready());
    let result = result.take_orders_info().unwrap();
    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert!(config.IOIsInput, "IOIsInput should be true for buy mode");

    let expected_price_cap = Float::parse(price_cap_str.to_string()).unwrap().get_inner();
    assert_eq!(
        config.maximumIORatio, expected_price_cap,
        "maximumIORatio should match price_cap"
    );

    let expected_max_io = Float::parse(buy_target.to_string()).unwrap().get_inner();
    assert_eq!(
        config.maximumIO, expected_max_io,
        "maximumIO should match buy target"
    );

    assert_eq!(config.orders.len(), 1, "Should have exactly 1 order");
}

#[tokio::test]
async fn test_single_order_take_expected_spend_calculation() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let mode = parse_buy_up_to("50");
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context)
        .await
        .expect("Should succeed");

    assert!(result.is_ready());
    let result = result.take_orders_info().unwrap();
    let expected_sell = Float::parse("100".to_string()).unwrap();
    assert!(
        result.expected_sell().eq(expected_sell).unwrap(),
        "expected_sell should be buy_amount * ratio = 50 * 2 = 100, got: {:?}",
        result.expected_sell().format()
    );

    let expected_price = Float::parse("2".to_string()).unwrap();
    let tolerance = Float::parse("0.01".to_string()).unwrap();
    let diff = if result.effective_price().gt(expected_price).unwrap() {
        result.effective_price().sub(expected_price).unwrap()
    } else {
        expected_price.sub(result.effective_price()).unwrap()
    };
    assert!(
        diff.lte(tolerance).unwrap(),
        "effective_price should be ~2 (sell/buy = 100/50), got: {:?}",
        result.effective_price().format()
    );
}

use std::ops::Sub;

#[tokio::test]
async fn test_single_order_take_spend_exact_mode() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let spend_budget = "100";
    let mode = parse_spend_exact(spend_budget);
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context)
        .await
        .expect("Should succeed with SpendExact mode");

    assert!(result.is_ready());
    let result = result.take_orders_info().unwrap();
    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert!(
        !config.IOIsInput,
        "IOIsInput should be false for spend mode"
    );

    let expected_spend = Float::parse(spend_budget.to_string()).unwrap().get_inner();
    assert_eq!(
        config.minimumIO, expected_spend,
        "minimumIO should equal spend_budget for SpendExact mode"
    );
    assert_eq!(
        config.maximumIO, expected_spend,
        "maximumIO should equal spend_budget for SpendExact mode"
    );
}

#[test]
fn test_estimate_buy_sufficient_capacity() {
    let max_output = Float::parse("1000".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let amount = Float::parse("100".to_string()).unwrap();

    let result = estimate_take_order(max_output, ratio, true, amount).unwrap();

    let expected_receive = Float::parse("100".to_string()).unwrap();
    let expected_spend = Float::parse("200".to_string()).unwrap();

    assert!(result.expected_receive().eq(expected_receive).unwrap());
    assert!(result.expected_spend().eq(expected_spend).unwrap());
    assert!(!result.is_partial());
}

#[test]
fn test_estimate_buy_insufficient_capacity() {
    let max_output = Float::parse("50".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let amount = Float::parse("100".to_string()).unwrap();

    let result = estimate_take_order(max_output, ratio, true, amount).unwrap();

    let expected_receive = Float::parse("50".to_string()).unwrap();
    let expected_spend = Float::parse("100".to_string()).unwrap();

    assert!(result.expected_receive().eq(expected_receive).unwrap());
    assert!(result.expected_spend().eq(expected_spend).unwrap());
    assert!(result.is_partial());
}

#[test]
fn test_estimate_sell_sufficient_capacity() {
    let max_output = Float::parse("1000".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let amount = Float::parse("100".to_string()).unwrap();

    let result = estimate_take_order(max_output, ratio, false, amount).unwrap();

    let expected_spend = Float::parse("100".to_string()).unwrap();
    let expected_receive = Float::parse("50".to_string()).unwrap();

    assert!(result.expected_spend().eq(expected_spend).unwrap());
    assert!(result.expected_receive().eq(expected_receive).unwrap());
    assert!(!result.is_partial());
}

#[test]
fn test_estimate_sell_insufficient_capacity() {
    let max_output = Float::parse("50".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let amount = Float::parse("200".to_string()).unwrap();

    let result = estimate_take_order(max_output, ratio, false, amount).unwrap();

    let expected_spend = Float::parse("100".to_string()).unwrap();
    let expected_receive = Float::parse("50".to_string()).unwrap();

    assert!(result.expected_spend().eq(expected_spend).unwrap());
    assert!(result.expected_receive().eq(expected_receive).unwrap());
    assert!(result.is_partial());
}

#[test]
fn test_estimate_zero_amount() {
    let max_output = Float::parse("1000".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let amount = Float::parse("0".to_string()).unwrap();

    let result = estimate_take_order(max_output, ratio, true, amount).unwrap();

    let zero = Float::zero().unwrap();
    assert!(result.expected_spend().eq(zero).unwrap());
    assert!(result.expected_receive().eq(zero).unwrap());
    assert!(!result.is_partial());
}

#[test]
fn test_estimate_negative_amount() {
    let max_output = Float::parse("1000".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let amount = Float::parse("-10".to_string()).unwrap();

    let result = estimate_take_order(max_output, ratio, true, amount).unwrap();

    let zero = Float::zero().unwrap();
    assert!(result.expected_spend().eq(zero).unwrap());
    assert!(result.expected_receive().eq(zero).unwrap());
    assert!(!result.is_partial());
}

#[test]
fn test_estimate_exact_capacity_buy() {
    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let amount = Float::parse("100".to_string()).unwrap();

    let result = estimate_take_order(max_output, ratio, true, amount).unwrap();

    let expected_receive = Float::parse("100".to_string()).unwrap();
    let expected_spend = Float::parse("200".to_string()).unwrap();

    assert!(result.expected_receive().eq(expected_receive).unwrap());
    assert!(result.expected_spend().eq(expected_spend).unwrap());
    assert!(!result.is_partial());
}

#[test]
fn test_estimate_exact_capacity_sell() {
    let max_output = Float::parse("50".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let amount = Float::parse("100".to_string()).unwrap();

    let result = estimate_take_order(max_output, ratio, false, amount).unwrap();

    let expected_spend = Float::parse("100".to_string()).unwrap();
    let expected_receive = Float::parse("50".to_string()).unwrap();

    assert!(result.expected_spend().eq(expected_spend).unwrap());
    assert!(result.expected_receive().eq(expected_receive).unwrap());
    assert!(!result.is_partial());
}

#[tokio::test]
async fn test_single_order_take_spend_exact_insufficient_liquidity() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "50", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let max_output = Float::parse("50".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let mode = parse_spend_exact("200");
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context).await;

    assert!(
        matches!(result, Err(RaindexError::InsufficientLiquidity { .. })),
        "SpendExact mode should return InsufficientLiquidity when amount > available max_input (100), got: {:?}",
        result
    );
}

#[tokio::test]
async fn test_single_order_take_calldata_encoding_spend_mode() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let spend_target = "100";
    let price_cap_str = "10";
    let mode = parse_spend_up_to(spend_target);
    let price_cap = Float::parse(price_cap_str.to_string()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context)
        .await
        .expect("Should succeed");

    assert!(result.is_ready());
    let result = result.take_orders_info().unwrap();
    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert!(
        !config.IOIsInput,
        "IOIsInput should be false for spend mode"
    );

    let expected_price_cap = Float::parse(price_cap_str.to_string()).unwrap().get_inner();
    assert_eq!(
        config.maximumIORatio, expected_price_cap,
        "maximumIORatio should match price_cap"
    );

    let expected_max_io = Float::parse(spend_target.to_string()).unwrap().get_inner();
    assert_eq!(
        config.maximumIO, expected_max_io,
        "maximumIO should match spend target"
    );

    assert_eq!(
        config.minimumIO,
        Float::zero().unwrap().get_inner(),
        "minimumIO should be zero for SpendUpTo mode"
    );

    assert_eq!(config.orders.len(), 1, "Should have exactly 1 order");
}

/// Helper function to create execution parameters for tests.
fn create_execution_params(
    mode: ParsedTakeOrdersMode,
    price_cap: Float,
    taker: Address,
    sell_token: Address,
) -> TakeOrderExecutionParams {
    TakeOrderExecutionParams {
        mode,
        price_cap,
        taker,
        sell_token,
    }
}

#[tokio::test]
async fn test_single_order_take_expected_receive_calculation() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id = B256::from(U256::from(1u64));
    fund_standard_two_token_vault(&setup, vault_id).await;

    let vault1 = create_vault(vault_id, &setup, &setup.token1_sg);
    let vault2 = create_vault(vault_id, &setup, &setup.token2_sg);

    let dotrain = create_dotrain_config_with_params(&setup, "100", "2");
    let (order_bytes, order_hash) = deploy_order(&setup, dotrain).await;

    let order_json = create_sg_order_json(
        &setup,
        &order_bytes,
        order_hash,
        vec![vault1.clone(), vault2.clone()],
        vec![vault1.clone(), vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [order_json]
            }
        }));
    });

    let yaml = get_minimal_yaml_for_chain(
        123,
        &setup.local_evm.url().to_string(),
        &sg_server.url("/sg"),
        &setup.raindex.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let order = client
        .get_order_by_hash(&RaindexIdentifier::new(123, setup.raindex), order_hash)
        .await
        .unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let max_output = Float::parse("100".to_string()).unwrap();
    let ratio = Float::parse("2".to_string()).unwrap();
    let max_input = max_output.mul(ratio).unwrap();
    let quote = make_quote(
        0,
        1,
        Some(make_quote_value(max_output, max_input, ratio)),
        true,
    );

    let candidate = build_candidate_from_quote(&order, &quote)
        .unwrap()
        .expect("Should build candidate from quote");

    let mode = parse_spend_up_to("100");
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let rpc_urls = vec![url::Url::parse(&setup.local_evm.url()).unwrap()];

    let execution_params = create_execution_params(mode, price_cap, taker, setup.token1);
    let rpc_context = RpcContext {
        rpc_urls: &rpc_urls,
        block_number: None,
    };

    let result = execute_single_take(candidate, execution_params, rpc_context)
        .await
        .expect("Should succeed");

    assert!(result.is_ready());
    let result = result.take_orders_info().unwrap();
    let expected_sell = Float::parse("100".to_string()).unwrap();
    assert!(
        result.expected_sell().eq(expected_sell).unwrap(),
        "expected_sell should equal spend_amount = 100, got: {:?}",
        result.expected_sell().format()
    );

    let expected_receive = Float::parse("50".to_string()).unwrap();
    let computed_receive = result
        .expected_sell()
        .div(result.effective_price())
        .unwrap();
    let tolerance = Float::parse("0.01".to_string()).unwrap();
    let receive_diff = if computed_receive.gt(expected_receive).unwrap() {
        computed_receive.sub(expected_receive).unwrap()
    } else {
        expected_receive.sub(computed_receive).unwrap()
    };
    assert!(
        receive_diff.lte(tolerance).unwrap(),
        "expected receive (computed from expected_sell / effective_price) should be ~50, got: {:?}",
        computed_receive.format()
    );

    let expected_price = Float::parse("2".to_string()).unwrap();
    let tolerance = Float::parse("0.01".to_string()).unwrap();
    let diff = if result.effective_price().gt(expected_price).unwrap() {
        result.effective_price().sub(expected_price).unwrap()
    } else {
        expected_price.sub(result.effective_price()).unwrap()
    };
    assert!(
        diff.lte(tolerance).unwrap(),
        "effective_price should be ~2 (sell/buy = 100/50), got: {:?}",
        result.effective_price().format()
    );
}
