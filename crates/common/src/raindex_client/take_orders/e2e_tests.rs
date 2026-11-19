use crate::raindex_client::take_orders::TakeOrdersRequest;
use crate::raindex_client::tests::get_test_yaml;
use crate::raindex_client::RaindexClient;
use crate::raindex_client::RaindexError;
use crate::take_orders::TakeOrdersMode;
use crate::test_helpers::dotrain::{
    create_dotrain_config_for_raindex, create_dotrain_config_with_params,
    create_dotrain_config_with_vault_and_ratio,
};
use crate::test_helpers::local_evm::{
    approve_taker, create_vault, create_vault_for_raindex, deposit_to_raindex,
    fund_and_approve_taker, fund_and_approve_taker_multi_raindex, fund_standard_two_token_vault,
    fund_taker, setup_multi_raindex_test, setup_test as base_setup_test, standard_deposit_amount,
};
use crate::test_helpers::orders::deploy::{deploy_order, deploy_order_to_raindex};
use crate::test_helpers::subgraph::{
    create_sg_order_json, create_sg_order_json_with_raindex, get_minimal_yaml_for_chain,
    get_multi_raindex_yaml,
};
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, B256, U256};
use alloy::rpc::types::TransactionRequest;
use alloy::serde::WithOtherFields;
use alloy::sol_types::SolCall;
use httpmock::MockServer;
use rain_math_float::Float;
use raindex_bindings::IRaindexV7::{takeOrders5Call, TakeOrdersConfigV6};
use serde_json::json;
use std::ops::{Mul, Sub};

fn high_price_cap() -> String {
    "1000000".to_string()
}

fn test_taker() -> String {
    Address::ZERO.to_string()
}

#[tokio::test]
async fn test_get_take_orders_calldata_no_orders_returns_no_liquidity() {
    let sg_server = MockServer::start_async().await;

    sg_server.mock(|when, then| {
        when.path("/sg1");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": []
            }
        }));
    });
    sg_server.mock(|when, then| {
        when.path("/sg2");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": []
            }
        }));
    });

    let client = RaindexClient::new(
        vec![get_test_yaml(
            &sg_server.url("/sg1"),
            &sg_server.url("/sg2"),
            "http://localhost:0/unused_rpc1",
            "http://localhost:0/unused_rpc2",
        )],
        None,
        None,
    )
    .await
    .unwrap();

    let res = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 1,
            taker: test_taker(),
            sell_token: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            buy_token: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: "1".to_string(),
            price_cap: high_price_cap(),
        })
        .await;

    assert!(
        matches!(res, Err(RaindexError::NoLiquidity)),
        "Expected NoLiquidity error when subgraph returns empty orders, got: {:?}",
        res
    );
}

#[tokio::test]
async fn test_get_take_orders_calldata_no_candidates_returns_no_liquidity() {
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

    let res = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: test_taker(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: "10".to_string(),
            price_cap: high_price_cap(),
        })
        .await;

    assert!(
        matches!(res, Err(RaindexError::NoLiquidity)),
        "Expected NoLiquidity error when no candidates (no vault balance), got: {:?}",
        res
    );
}

#[tokio::test]
async fn test_get_take_orders_calldata_happy_path_returns_valid_config() {
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: "100".to_string(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("Should succeed with funded vault and valid order");
    assert!(result.is_ready(), "Expected Ready variant");
    let result = result.take_orders_info().unwrap();

    assert_eq!(
        result.raindex(),
        setup.raindex,
        "Raindex address should match"
    );

    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert!(
        !config.orders.is_empty(),
        "Should have at least one order in config"
    );

    assert_eq!(
        config.minimumIO,
        Float::zero().unwrap().get_inner(),
        "minimumIO should be zero for Partial mode"
    );

    assert!(
        !result.prices().is_empty(),
        "Should have at least one price in result"
    );

    let expected_ratio = Float::parse("2".to_string()).unwrap();
    assert!(
        result.prices()[0].eq(expected_ratio).unwrap(),
        "Price should match expected ratio of 2, got: {:?}",
        result.prices()[0].format()
    );

    let zero = Float::zero().unwrap();
    assert!(
        result.effective_price().gt(zero).unwrap(),
        "Effective price should be > 0"
    );
}

#[tokio::test]
async fn test_get_take_orders_calldata_min_receive_mode_exact_vs_partial() {
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let buy_target = "50".to_string();
    let price_cap = "5".to_string();
    let result_partial = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: buy_target.clone(),
            price_cap: price_cap.clone(),
        })
        .await
        .expect("BuyUpTo mode should succeed");
    assert!(
        result_partial.is_ready(),
        "Expected Ready variant for BuyUpTo"
    );
    let result_partial = result_partial.take_orders_info().unwrap();

    let result_exact = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyExact,
            amount: buy_target.clone(),
            price_cap: price_cap.clone(),
        })
        .await
        .expect("BuyExact mode should succeed");
    assert!(
        result_exact.is_ready(),
        "Expected Ready variant for BuyExact"
    );
    let result_exact = result_exact.take_orders_info().unwrap();

    let decoded_partial = takeOrders5Call::abi_decode(result_partial.calldata())
        .expect("Should decode partial calldata");
    let config_partial = decoded_partial.config;

    let decoded_exact =
        takeOrders5Call::abi_decode(result_exact.calldata()).expect("Should decode exact calldata");
    let config_exact = decoded_exact.config;

    let expected_buy_target = Float::parse(buy_target).unwrap().get_inner();
    let expected_price_cap = Float::parse(price_cap).unwrap().get_inner();

    assert_eq!(
        config_partial.maximumIO, expected_buy_target,
        "maximumIO should equal buy_target"
    );
    assert_eq!(
        config_exact.maximumIO, expected_buy_target,
        "maximumIO should equal buy_target"
    );

    assert_eq!(
        config_partial.minimumIO,
        Float::zero().unwrap().get_inner(),
        "minimumIO should be zero for BuyUpTo mode"
    );

    assert_eq!(
        config_exact.minimumIO, expected_buy_target,
        "minimumIO should equal buy_target for BuyExact mode"
    );

    assert_eq!(
        config_partial.maximumIORatio, expected_price_cap,
        "maximumIORatio should equal price_cap"
    );
    assert_eq!(
        config_exact.maximumIORatio, expected_price_cap,
        "maximumIORatio should equal price_cap"
    );
}

#[tokio::test]
async fn test_get_take_orders_calldata_wrong_direction_returns_no_liquidity() {
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

    let fake_token = "0xcccccccccccccccccccccccccccccccccccccccc";
    let res = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: test_taker(),
            sell_token: fake_token.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: "10".to_string(),
            price_cap: high_price_cap(),
        })
        .await;

    assert!(
        matches!(res, Err(RaindexError::NoLiquidity)),
        "Expected NoLiquidity error when using wrong direction/fake token, got: {:?}",
        res
    );
}

#[tokio::test]
async fn test_min_receive_mode_exact_returns_error_when_insufficient_liquidity() {
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let result_partial = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: "100".to_string(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("BuyUpTo mode calldata build should succeed");
    assert!(result_partial.is_ready(), "Expected Ready variant");
    let result_partial = result_partial.take_orders_info().unwrap();

    let decoded_partial = takeOrders5Call::abi_decode(result_partial.calldata())
        .expect("Should decode partial calldata");
    let config_partial = decoded_partial.config;

    assert_eq!(
        config_partial.minimumIO,
        Float::zero().unwrap().get_inner(),
        "BuyUpTo mode minimumIO should be zero"
    );

    let result_exact = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyExact,
            amount: "100".to_string(),
            price_cap: high_price_cap(),
        })
        .await;

    assert!(
        matches!(
            result_exact,
            Err(RaindexError::InsufficientLiquidity { .. })
        ),
        "BuyExact mode should return InsufficientLiquidity when buy_target > available, got: {:?}",
        result_exact
    );
}

#[tokio::test]
async fn test_maximum_io_ratio_enforcement_skips_overpriced_leg() {
    let setup = base_setup_test().await;

    let vault_id_1 = B256::from(U256::from(1u64));
    let vault_id_2 = B256::from(U256::from(2u64));

    let amount = standard_deposit_amount();
    setup
        .local_evm
        .deposit(setup.owner, setup.token2, amount, 18, vault_id_1)
        .await;
    setup
        .local_evm
        .deposit(setup.owner, setup.token2, amount, 18, vault_id_2)
        .await;

    let dotrain_cheap = create_dotrain_config_with_vault_and_ratio(&setup, "0x01", "50", "1");
    let dotrain_expensive = create_dotrain_config_with_vault_and_ratio(&setup, "0x02", "50", "2");

    let (order_bytes_cheap, order_hash_cheap) = deploy_order(&setup, dotrain_cheap).await;
    let (order_bytes_expensive, order_hash_expensive) =
        deploy_order(&setup, dotrain_expensive).await;

    let vault1 = create_vault(vault_id_1, &setup, &setup.token2_sg);
    let vault2 = create_vault(vault_id_2, &setup, &setup.token2_sg);
    let input_vault = create_vault(vault_id_1, &setup, &setup.token1_sg);

    let sg_order_cheap = create_sg_order_json(
        &setup,
        &order_bytes_cheap,
        order_hash_cheap,
        vec![input_vault.clone()],
        vec![vault1.clone()],
    );
    let sg_order_expensive = create_sg_order_json(
        &setup,
        &order_bytes_expensive,
        order_hash_expensive,
        vec![input_vault.clone()],
        vec![vault2.clone()],
    );

    let sg_server = MockServer::start_async().await;
    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [sg_order_cheap, sg_order_expensive]
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let buy_target = "100".to_string();
    let price_cap = "2".to_string();
    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: buy_target.clone(),
            price_cap: price_cap.clone(),
        })
        .await
        .expect("Should build calldata with both orders");
    assert!(result.is_ready(), "Expected Ready variant");
    let result = result.take_orders_info().unwrap();

    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let original_config = decoded.config;

    assert_eq!(
        original_config.orders.len(),
        2,
        "Should have 2 orders in config"
    );

    let expected_price_cap = Float::parse(price_cap.clone()).unwrap();
    assert_eq!(
        original_config.maximumIORatio,
        expected_price_cap.get_inner(),
        "maximumIORatio should equal price_cap (2)"
    );

    assert_eq!(result.prices().len(), 2, "Should have 2 prices");
    let cheap_price = Float::parse("1".to_string()).unwrap();
    let expensive_price = Float::parse("2".to_string()).unwrap();
    assert!(
        result.prices().iter().any(|p| p.eq(cheap_price).unwrap()),
        "Should have price 1 in the list"
    );
    assert!(
        result
            .prices()
            .iter()
            .any(|p| p.eq(expensive_price).unwrap()),
        "Should have price 2 in the list"
    );

    let lowered_max_io_ratio = Float::parse("1.5".to_string()).unwrap();
    let modified_config = TakeOrdersConfigV6 {
        minimumIO: original_config.minimumIO,
        maximumIO: original_config.maximumIO,
        maximumIORatio: lowered_max_io_ratio.get_inner(),
        IOIsInput: false,
        orders: original_config.orders.clone(),
        data: original_config.data.clone(),
    };

    let modified_calldata_bytes = takeOrders5Call {
        config: modified_config,
    }
    .abi_encode();

    let tx = WithOtherFields::new(
        TransactionRequest::default()
            .with_input(modified_calldata_bytes.clone())
            .with_to(setup.raindex)
            .with_from(taker),
    );

    let call_result = setup.local_evm.call(tx.clone()).await;
    assert!(
        call_result.is_ok(),
        "Partial mode with lowered maximumIORatio should not revert, got: {:?}",
        call_result
    );

    let token2_contract = setup
        .local_evm
        .tokens
        .iter()
        .find(|t| *t.address() == setup.token2)
        .unwrap();

    let taker_token2_before: U256 = token2_contract.balanceOf(taker).call().await.unwrap();

    let tx_result = setup.local_evm.send_transaction(tx).await;
    assert!(
        tx_result.is_ok(),
        "Transaction should succeed, got: {:?}",
        tx_result
    );

    let taker_token2_after: U256 = token2_contract.balanceOf(taker).call().await.unwrap();

    let received = taker_token2_after - taker_token2_before;
    let expected_from_cheap_only =
        Float::from_fixed_decimal(U256::from(50) * U256::from(10).pow(U256::from(18)), 18).unwrap();

    let received_float = Float::from_fixed_decimal(received, 18).unwrap();
    assert!(
        received_float.lte(expected_from_cheap_only).unwrap(),
        "Should only receive from cheap order (max 50), got: {:?}",
        received_float.format()
    );

    assert!(
        received > U256::ZERO,
        "Should have received some tokens from cheap order"
    );

    let exact_config = TakeOrdersConfigV6 {
        minimumIO: original_config.maximumIO,
        maximumIO: original_config.maximumIO,
        maximumIORatio: lowered_max_io_ratio.get_inner(),
        IOIsInput: false,
        orders: original_config.orders.clone(),
        data: original_config.data.clone(),
    };

    let exact_calldata_bytes = takeOrders5Call {
        config: exact_config,
    }
    .abi_encode();

    let exact_tx = WithOtherFields::new(
        TransactionRequest::default()
            .with_input(exact_calldata_bytes)
            .with_to(setup.raindex)
            .with_from(taker),
    );

    let exact_call_result = setup.local_evm.call(exact_tx).await;
    assert!(
        exact_call_result.is_err(),
        "Exact mode should revert when expensive leg is skipped due to maximumIORatio, got: {:?}",
        exact_call_result
    );

    let error_str = format!("{:?}", exact_call_result.unwrap_err());
    assert!(
        error_str.contains("MinimumInput") || error_str.contains("execution reverted"),
        "Error should indicate MinimumInput revert because expected buy cannot be met, got: {}",
        error_str
    );
}

#[tokio::test]
async fn test_maximum_io_ratio_enforcement_with_worsened_on_chain_price() {
    let setup = base_setup_test().await;

    let vault_id_1 = B256::from(U256::from(1u64));
    let vault_id_2 = B256::from(U256::from(2u64));

    let amount = standard_deposit_amount();
    setup
        .local_evm
        .deposit(setup.owner, setup.token2, amount, 18, vault_id_1)
        .await;
    setup
        .local_evm
        .deposit(setup.owner, setup.token2, amount, 18, vault_id_2)
        .await;

    let dotrain_cheap = create_dotrain_config_with_vault_and_ratio(&setup, "0x01", "50", "1");
    let dotrain_expensive = create_dotrain_config_with_vault_and_ratio(&setup, "0x02", "50", "2");

    let (order_bytes_cheap, order_hash_cheap) = deploy_order(&setup, dotrain_cheap).await;
    let (order_bytes_expensive, order_hash_expensive) =
        deploy_order(&setup, dotrain_expensive.clone()).await;

    let vault1 = create_vault(vault_id_1, &setup, &setup.token2_sg);
    let vault2 = create_vault(vault_id_2, &setup, &setup.token2_sg);
    let input_vault = create_vault(vault_id_1, &setup, &setup.token1_sg);

    let sg_order_cheap = create_sg_order_json(
        &setup,
        &order_bytes_cheap,
        order_hash_cheap,
        vec![input_vault.clone()],
        vec![vault1.clone()],
    );
    let sg_order_expensive = create_sg_order_json(
        &setup,
        &order_bytes_expensive,
        order_hash_expensive,
        vec![input_vault.clone()],
        vec![vault2.clone()],
    );

    let sg_server = MockServer::start_async().await;
    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [sg_order_cheap, sg_order_expensive]
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let buy_target_2 = "100".to_string();
    let price_cap_2 = "2".to_string();
    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: buy_target_2.clone(),
            price_cap: price_cap_2.clone(),
        })
        .await
        .expect("Should build calldata with both orders");
    assert!(result.is_ready(), "Expected Ready variant");
    let result = result.take_orders_info().unwrap();

    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let original_config = decoded.config;

    let expected_price_cap_2 = Float::parse(price_cap_2.clone()).unwrap();
    assert_eq!(
        original_config.maximumIORatio,
        expected_price_cap_2.get_inner(),
        "maximumIORatio should equal price_cap (2)"
    );

    let withdraw_amount = Float::from_fixed_decimal(amount, 18).unwrap().get_inner();

    let withdraw_tx = setup
        .local_evm
        .raindex
        .withdraw5(setup.token2, vault_id_2, withdraw_amount, vec![])
        .from(setup.owner)
        .into_transaction_request();
    setup
        .local_evm
        .send_transaction(withdraw_tx)
        .await
        .expect("Withdraw should succeed");

    let vault_id_3 = B256::from(U256::from(3u64));
    setup
        .local_evm
        .deposit(setup.owner, setup.token2, amount, 18, vault_id_3)
        .await;

    let dotrain_worsened = create_dotrain_config_with_vault_and_ratio(&setup, "0x03", "50", "3");
    let (_, _) = deploy_order(&setup, dotrain_worsened).await;

    let tx = WithOtherFields::new(
        TransactionRequest::default()
            .with_input(result.calldata().to_vec())
            .with_to(setup.raindex)
            .with_from(taker),
    );

    let token2_contract = setup
        .local_evm
        .tokens
        .iter()
        .find(|t| *t.address() == setup.token2)
        .unwrap();

    let taker_token2_before: U256 = token2_contract.balanceOf(taker).call().await.unwrap();

    let tx_result = setup.local_evm.send_transaction(tx).await;
    assert!(
        tx_result.is_ok(),
        "Transaction with original calldata should succeed, got: {:?}",
        tx_result
    );

    let taker_token2_after: U256 = token2_contract.balanceOf(taker).call().await.unwrap();

    let received = taker_token2_after - taker_token2_before;
    let expected_from_cheap_only =
        Float::from_fixed_decimal(U256::from(50) * U256::from(10).pow(U256::from(18)), 18).unwrap();

    let received_float = Float::from_fixed_decimal(received, 18).unwrap();
    assert!(
        received_float.lte(expected_from_cheap_only).unwrap(),
        "Should only receive from cheap order since expensive order's vault was emptied, got: {:?}",
        received_float.format()
    );

    assert!(
        received > U256::ZERO,
        "Should have received tokens from cheap order"
    );

    let exact_config = TakeOrdersConfigV6 {
        minimumIO: original_config.maximumIO,
        maximumIO: original_config.maximumIO,
        maximumIORatio: original_config.maximumIORatio,
        IOIsInput: false,
        orders: original_config.orders.clone(),
        data: original_config.data.clone(),
    };

    let exact_calldata_bytes = takeOrders5Call {
        config: exact_config,
    }
    .abi_encode();

    let exact_tx = WithOtherFields::new(
        TransactionRequest::default()
            .with_input(exact_calldata_bytes)
            .with_to(setup.raindex)
            .with_from(taker),
    );

    let exact_call_result = setup.local_evm.call(exact_tx).await;
    assert!(
        exact_call_result.is_err(),
        "Exact mode should revert when simulated buy cannot be achieved after vault emptied, got: {:?}",
        exact_call_result
    );
}

#[tokio::test]
async fn test_cross_raindex_selection_picks_best_book() {
    let setup = setup_multi_raindex_test().await;
    let sg_server = MockServer::start_async().await;

    assert_ne!(
        setup.raindex_a, setup.raindex_b,
        "Raindex addresses should be different"
    );

    let vault_id_a = B256::from(U256::from(1u64));
    let vault_id_b = B256::from(U256::from(2u64));

    let deposit_amount = U256::from(10).pow(U256::from(22));
    deposit_to_raindex(
        &setup,
        setup.raindex_a,
        setup.token2,
        deposit_amount,
        vault_id_a,
    )
    .await;
    deposit_to_raindex(
        &setup,
        setup.raindex_b,
        setup.token2,
        deposit_amount,
        vault_id_b,
    )
    .await;

    let dotrain_a = create_dotrain_config_for_raindex(&setup, setup.raindex_a, "0x01", "5", "2");
    let (order_bytes_a, order_hash_a, _order_v4_a) =
        deploy_order_to_raindex(&setup, setup.raindex_a, dotrain_a).await;

    let dotrain_b = create_dotrain_config_for_raindex(&setup, setup.raindex_b, "0x02", "8", "2");
    let (order_bytes_b, order_hash_b, order_v4_b) =
        deploy_order_to_raindex(&setup, setup.raindex_b, dotrain_b).await;

    let vault_a_input =
        create_vault_for_raindex(vault_id_a, &setup, setup.raindex_a, &setup.token1_sg);
    let vault_a_output =
        create_vault_for_raindex(vault_id_a, &setup, setup.raindex_a, &setup.token2_sg);
    let vault_b_input =
        create_vault_for_raindex(vault_id_b, &setup, setup.raindex_b, &setup.token1_sg);
    let vault_b_output =
        create_vault_for_raindex(vault_id_b, &setup, setup.raindex_b, &setup.token2_sg);

    let sg_order_a = create_sg_order_json_with_raindex(
        &setup,
        setup.raindex_a,
        &order_bytes_a,
        order_hash_a,
        vec![vault_a_input],
        vec![vault_a_output],
    );
    let sg_order_b = create_sg_order_json_with_raindex(
        &setup,
        setup.raindex_b,
        &order_bytes_b,
        order_hash_b,
        vec![vault_b_input],
        vec![vault_b_output],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [sg_order_a, sg_order_b]
            }
        }));
    });

    let yaml = get_multi_raindex_yaml(
        123,
        &setup.local_evm.url(),
        &sg_server.url("/sg"),
        &setup.raindex_a.to_string(),
        &setup.raindex_b.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker_multi_raindex(
        &setup,
        setup.token1,
        taker,
        setup.raindex_b,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let buy_target_cross = "8".to_string();
    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: buy_target_cross.clone(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("Should succeed with orders from multiple raindexes");
    assert!(result.is_ready(), "Expected Ready variant");
    let result = result.take_orders_info().unwrap();

    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert_eq!(
        result.raindex(),
        setup.raindex_b,
        "Should select raindex B (max_output=8 > max_output=5)"
    );

    assert!(
        !config.orders.is_empty(),
        "Should have at least one order from the winning raindex"
    );

    for config_item in &config.orders {
        let config_order = &config_item.order;
        assert_eq!(
            config_order.owner, order_v4_b.owner,
            "All orders should be from raindex B"
        );
        assert_eq!(
            config_order.evaluable.bytecode, order_v4_b.evaluable.bytecode,
            "All order bytecodes should match raindex B's order"
        );
    }

    let expected_ratio = Float::parse("2".to_string()).unwrap();
    assert!(
        result.prices()[0].eq(expected_ratio).unwrap(),
        "Price should be 2 (raindex B's ratio)"
    );

    let tolerance = Float::parse("0.0001".to_string()).unwrap();
    let diff = if result.effective_price().gt(expected_ratio).unwrap() {
        result.effective_price().sub(expected_ratio).unwrap()
    } else {
        expected_ratio.sub(result.effective_price()).unwrap()
    };
    assert!(
        diff.lte(tolerance).unwrap(),
        "Effective price should be ~2 (sell/buy ratio), got: {:?}",
        result.effective_price().format()
    );
}

#[tokio::test]
async fn test_cross_raindex_selection_flips_when_economics_flip() {
    let setup = setup_multi_raindex_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id_a = B256::from(U256::from(1u64));
    let vault_id_b = B256::from(U256::from(2u64));

    let deposit_amount = U256::from(10).pow(U256::from(22));
    deposit_to_raindex(
        &setup,
        setup.raindex_a,
        setup.token2,
        deposit_amount,
        vault_id_a,
    )
    .await;
    deposit_to_raindex(
        &setup,
        setup.raindex_b,
        setup.token2,
        deposit_amount,
        vault_id_b,
    )
    .await;

    let dotrain_a = create_dotrain_config_for_raindex(&setup, setup.raindex_a, "0x01", "10", "2");
    let (order_bytes_a, order_hash_a, order_v4_a) =
        deploy_order_to_raindex(&setup, setup.raindex_a, dotrain_a).await;

    let dotrain_b = create_dotrain_config_for_raindex(&setup, setup.raindex_b, "0x02", "3", "2");
    let (order_bytes_b, order_hash_b, _order_v4_b) =
        deploy_order_to_raindex(&setup, setup.raindex_b, dotrain_b).await;

    let vault_a_input =
        create_vault_for_raindex(vault_id_a, &setup, setup.raindex_a, &setup.token1_sg);
    let vault_a_output =
        create_vault_for_raindex(vault_id_a, &setup, setup.raindex_a, &setup.token2_sg);
    let vault_b_input =
        create_vault_for_raindex(vault_id_b, &setup, setup.raindex_b, &setup.token1_sg);
    let vault_b_output =
        create_vault_for_raindex(vault_id_b, &setup, setup.raindex_b, &setup.token2_sg);

    let sg_order_a = create_sg_order_json_with_raindex(
        &setup,
        setup.raindex_a,
        &order_bytes_a,
        order_hash_a,
        vec![vault_a_input],
        vec![vault_a_output],
    );
    let sg_order_b = create_sg_order_json_with_raindex(
        &setup,
        setup.raindex_b,
        &order_bytes_b,
        order_hash_b,
        vec![vault_b_input],
        vec![vault_b_output],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [sg_order_a, sg_order_b]
            }
        }));
    });

    let yaml = get_multi_raindex_yaml(
        123,
        &setup.local_evm.url(),
        &sg_server.url("/sg"),
        &setup.raindex_a.to_string(),
        &setup.raindex_b.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker_multi_raindex(
        &setup,
        setup.token1,
        taker,
        setup.raindex_a,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let buy_target_flip = "10".to_string();
    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: buy_target_flip.clone(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("Should succeed with flipped economics");
    assert!(result.is_ready(), "Expected Ready variant");
    let result = result.take_orders_info().unwrap();

    assert_eq!(
        result.raindex(),
        setup.raindex_a,
        "Should select raindex A (max_output=10 > max_output=3)"
    );

    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert!(
        !config.orders.is_empty(),
        "Should have at least one order from the winning raindex"
    );

    for config_item in &config.orders {
        let config_order = &config_item.order;
        assert_eq!(
            config_order.owner, order_v4_a.owner,
            "All orders should be from raindex A"
        );
        assert_eq!(
            config_order.evaluable.bytecode, order_v4_a.evaluable.bytecode,
            "All order bytecodes should match raindex A's order"
        );
    }

    let actual_max_input = Float::from_raw(config.maximumIO);
    let min_expected = Float::parse("10".to_string()).unwrap();
    assert!(
        actual_max_input.gte(min_expected).unwrap(),
        "maximumIO should be at least 10 (raindex A's max_output), got: {:?}",
        actual_max_input.format()
    );
}

#[tokio::test]
async fn test_cross_raindex_economic_selection_prefers_best_yield() {
    let setup = setup_multi_raindex_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id_a = B256::from(U256::from(1u64));
    let vault_id_b = B256::from(U256::from(2u64));

    let deposit_amount = U256::from(10).pow(U256::from(22));
    deposit_to_raindex(
        &setup,
        setup.raindex_a,
        setup.token2,
        deposit_amount,
        vault_id_a,
    )
    .await;
    deposit_to_raindex(
        &setup,
        setup.raindex_b,
        setup.token2,
        deposit_amount,
        vault_id_b,
    )
    .await;

    let dotrain_a = create_dotrain_config_for_raindex(&setup, setup.raindex_a, "0x01", "5", "1");
    let (order_bytes_a, order_hash_a, order_v4_a) =
        deploy_order_to_raindex(&setup, setup.raindex_a, dotrain_a).await;

    let dotrain_b = create_dotrain_config_for_raindex(&setup, setup.raindex_b, "0x02", "8", "1.5");
    let (order_bytes_b, order_hash_b, _order_v4_b) =
        deploy_order_to_raindex(&setup, setup.raindex_b, dotrain_b).await;

    let vault_a_input =
        create_vault_for_raindex(vault_id_a, &setup, setup.raindex_a, &setup.token1_sg);
    let vault_a_output =
        create_vault_for_raindex(vault_id_a, &setup, setup.raindex_a, &setup.token2_sg);
    let vault_b_input =
        create_vault_for_raindex(vault_id_b, &setup, setup.raindex_b, &setup.token1_sg);
    let vault_b_output =
        create_vault_for_raindex(vault_id_b, &setup, setup.raindex_b, &setup.token2_sg);

    let sg_order_a = create_sg_order_json_with_raindex(
        &setup,
        setup.raindex_a,
        &order_bytes_a,
        order_hash_a,
        vec![vault_a_input],
        vec![vault_a_output],
    );
    let sg_order_b = create_sg_order_json_with_raindex(
        &setup,
        setup.raindex_b,
        &order_bytes_b,
        order_hash_b,
        vec![vault_b_input],
        vec![vault_b_output],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [sg_order_a, sg_order_b]
            }
        }));
    });

    let yaml = get_multi_raindex_yaml(
        123,
        &setup.local_evm.url(),
        &sg_server.url("/sg"),
        &setup.raindex_a.to_string(),
        &setup.raindex_b.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker_multi_raindex(
        &setup,
        setup.token1,
        taker,
        setup.raindex_a,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let buy_target_yield = "5".to_string();
    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: buy_target_yield.clone(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("Should succeed with orders from multiple raindexes");
    assert!(result.is_ready(), "Expected Ready variant");
    let result = result.take_orders_info().unwrap();

    assert_eq!(
        result.raindex(), setup.raindex_a,
        "Should select raindex A (can fill 5 buy at ratio 1.0) over B (can fill 5 buy but at worse price 1.5)"
    );

    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert!(
        !config.orders.is_empty(),
        "Should have at least one order from the winning raindex"
    );

    for config_item in &config.orders {
        let config_order = &config_item.order;
        assert_eq!(
            config_order.owner, order_v4_a.owner,
            "All orders should be from raindex A"
        );
        assert_eq!(
            config_order.evaluable.bytecode, order_v4_a.evaluable.bytecode,
            "All order bytecodes should match raindex A's order"
        );
    }

    assert_eq!(
        result.prices().len(),
        1,
        "Should have exactly one price (from raindex A only)"
    );
    let expected_ratio = Float::parse("1".to_string()).unwrap();
    assert!(
        result.prices()[0].eq(expected_ratio).unwrap(),
        "Price should be 1.0 (raindex A's ratio), got: {:?}",
        result.prices()[0].format()
    );

    let tolerance = Float::parse("0.0001".to_string()).unwrap();
    let diff = if result.effective_price().gt(expected_ratio).unwrap() {
        result.effective_price().sub(expected_ratio).unwrap()
    } else {
        expected_ratio.sub(result.effective_price()).unwrap()
    };
    assert!(
        diff.lte(tolerance).unwrap(),
        "Effective price should be ~1.0 (total_sell/total_buy), got: {:?}",
        result.effective_price().format()
    );
}

#[tokio::test]
async fn test_get_take_orders_calldata_invalid_address_returns_from_hex_error() {
    let client = RaindexClient::new(
        vec![get_test_yaml(
            "http://localhost:0/unused_sg1",
            "http://localhost:0/unused_sg2",
            "http://localhost:0/unused_rpc1",
            "http://localhost:0/unused_rpc2",
        )],
        None,
        None,
    )
    .await
    .unwrap();

    let res = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 1,
            taker: test_taker(),
            sell_token: "not-an-address".to_string(),
            buy_token: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: "1".to_string(),
            price_cap: high_price_cap(),
        })
        .await;

    assert!(
        matches!(res, Err(RaindexError::FromHexError(_))),
        "Expected FromHexError for invalid sellToken address, got: {:?}",
        res
    );
}

#[tokio::test]
async fn test_get_take_orders_calldata_invalid_float_returns_float_error() {
    let client = RaindexClient::new(
        vec![get_test_yaml(
            "http://localhost:0/unused_sg1",
            "http://localhost:0/unused_sg2",
            "http://localhost:0/unused_rpc1",
            "http://localhost:0/unused_rpc2",
        )],
        None,
        None,
    )
    .await
    .unwrap();

    let res = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 1,
            taker: test_taker(),
            sell_token: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            buy_token: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: "not-a-float".to_string(),
            price_cap: high_price_cap(),
        })
        .await;

    assert!(
        matches!(res, Err(RaindexError::Float(_))),
        "Expected Float error for invalid amount, got: {:?}",
        res
    );
}

#[tokio::test]
async fn test_prices_sorted_best_to_worst_matching_config_orders() {
    let setup = base_setup_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id_1 = B256::from(U256::from(1u64));
    let vault_id_2 = B256::from(U256::from(2u64));

    let amount = standard_deposit_amount();
    setup
        .local_evm
        .deposit(setup.owner, setup.token2, amount, 18, vault_id_1)
        .await;
    setup
        .local_evm
        .deposit(setup.owner, setup.token2, amount, 18, vault_id_2)
        .await;

    let dotrain_cheap = create_dotrain_config_with_vault_and_ratio(&setup, "0x01", "100", "1");
    let dotrain_expensive = create_dotrain_config_with_vault_and_ratio(&setup, "0x02", "100", "2");

    let (order_bytes_cheap, order_hash_cheap) = deploy_order(&setup, dotrain_cheap).await;
    let (order_bytes_expensive, order_hash_expensive) =
        deploy_order(&setup, dotrain_expensive).await;

    let vault1 = create_vault(vault_id_1, &setup, &setup.token2_sg);
    let vault2 = create_vault(vault_id_2, &setup, &setup.token2_sg);
    let input_vault = create_vault(vault_id_1, &setup, &setup.token1_sg);

    let sg_order_cheap = create_sg_order_json(
        &setup,
        &order_bytes_cheap,
        order_hash_cheap,
        vec![input_vault.clone()],
        vec![vault1.clone()],
    );
    let sg_order_expensive = create_sg_order_json(
        &setup,
        &order_bytes_expensive,
        order_hash_expensive,
        vec![input_vault.clone()],
        vec![vault2.clone()],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [sg_order_expensive.clone(), sg_order_cheap.clone()]
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: "200".to_string(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("Should build calldata with both orders");
    assert!(result.is_ready(), "Expected Ready variant");
    let result = result.take_orders_info().unwrap();

    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert_eq!(
        result.prices().len(),
        2,
        "Should have 2 prices for 2 orders"
    );
    assert_eq!(config.orders.len(), 2, "Should have 2 orders in config");

    let cheap_price = Float::parse("1".to_string()).unwrap();
    let expensive_price = Float::parse("2".to_string()).unwrap();

    assert!(
        result.prices()[0].eq(cheap_price).unwrap(),
        "First price should be cheap (1), got: {:?}",
        result.prices()[0].format()
    );
    assert!(
        result.prices()[1].eq(expensive_price).unwrap(),
        "Second price should be expensive (2), got: {:?}",
        result.prices()[1].format()
    );

    assert!(
        result.prices()[0].lt(result.prices()[1]).unwrap(),
        "Prices should be sorted best (lowest) to worst: {:?} < {:?}",
        result.prices()[0].format(),
        result.prices()[1].format()
    );

    use alloy::primitives::keccak256;
    use alloy::sol_types::SolValue;
    let first_order_hash = B256::from(keccak256(config.orders[0].order.abi_encode()));
    let second_order_hash = B256::from(keccak256(config.orders[1].order.abi_encode()));

    assert_eq!(
        first_order_hash, order_hash_cheap,
        "First order in config should be the cheap order (ratio=1)"
    );
    assert_eq!(
        second_order_hash, order_hash_expensive,
        "Second order in config should be the expensive order (ratio=2)"
    );
}

#[tokio::test]
async fn test_spend_up_to_mode_happy_path() {
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::SpendUpTo,
            amount: "100".to_string(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("Should succeed with funded vault and valid order in spend mode");
    assert!(result.is_ready(), "Expected Ready variant");
    let result = result.take_orders_info().unwrap();

    assert_eq!(
        result.raindex(),
        setup.raindex,
        "Raindex address should match"
    );

    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert!(
        !config.orders.is_empty(),
        "Should have at least one order in config"
    );

    assert!(
        !config.IOIsInput,
        "IOIsInput should be false for spend mode"
    );

    assert_eq!(
        config.minimumIO,
        Float::zero().unwrap().get_inner(),
        "minimumIO should be zero for SpendUpTo mode"
    );

    let spend_budget = Float::parse("100".to_string()).unwrap();
    assert_eq!(
        config.maximumIO,
        spend_budget.get_inner(),
        "maximumIO should equal spend budget"
    );

    assert!(
        !result.prices().is_empty(),
        "Should have at least one price in result"
    );

    let zero = Float::zero().unwrap();
    assert!(
        result.effective_price().gt(zero).unwrap(),
        "Effective price should be > 0"
    );
}

#[tokio::test]
async fn test_spend_exact_vs_spend_up_to_modes() {
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let spend_budget = "50".to_string();
    let price_cap = "5".to_string();

    let result_up_to = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::SpendUpTo,
            amount: spend_budget.clone(),
            price_cap: price_cap.clone(),
        })
        .await
        .expect("SpendUpTo mode should succeed");
    assert!(
        result_up_to.is_ready(),
        "Expected Ready variant for SpendUpTo"
    );
    let result_up_to = result_up_to.take_orders_info().unwrap();

    let result_exact = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::SpendExact,
            amount: spend_budget.clone(),
            price_cap: price_cap.clone(),
        })
        .await
        .expect("SpendExact mode should succeed");
    assert!(
        result_exact.is_ready(),
        "Expected Ready variant for SpendExact"
    );
    let result_exact = result_exact.take_orders_info().unwrap();

    let decoded_up_to =
        takeOrders5Call::abi_decode(result_up_to.calldata()).expect("Should decode up_to calldata");
    let config_up_to = decoded_up_to.config;

    let decoded_exact =
        takeOrders5Call::abi_decode(result_exact.calldata()).expect("Should decode exact calldata");
    let config_exact = decoded_exact.config;

    let expected_spend_budget = Float::parse(spend_budget).unwrap().get_inner();
    let expected_price_cap = Float::parse(price_cap).unwrap().get_inner();

    assert!(
        !config_up_to.IOIsInput,
        "IOIsInput should be false for SpendUpTo mode"
    );
    assert!(
        !config_exact.IOIsInput,
        "IOIsInput should be false for SpendExact mode"
    );

    assert_eq!(
        config_up_to.maximumIO, expected_spend_budget,
        "maximumIO should equal spend_budget for SpendUpTo"
    );
    assert_eq!(
        config_exact.maximumIO, expected_spend_budget,
        "maximumIO should equal spend_budget for SpendExact"
    );

    assert_eq!(
        config_up_to.minimumIO,
        Float::zero().unwrap().get_inner(),
        "minimumIO should be zero for SpendUpTo mode"
    );
    assert_eq!(
        config_exact.minimumIO, expected_spend_budget,
        "minimumIO should equal spend_budget for SpendExact mode"
    );

    assert_eq!(
        config_up_to.maximumIORatio, expected_price_cap,
        "maximumIORatio should equal price_cap"
    );
    assert_eq!(
        config_exact.maximumIORatio, expected_price_cap,
        "maximumIORatio should equal price_cap"
    );
}

#[tokio::test]
async fn test_spend_exact_mode_insufficient_liquidity() {
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let result_up_to = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::SpendUpTo,
            amount: "200".to_string(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("SpendUpTo mode calldata build should succeed even with insufficient liquidity");
    assert!(result_up_to.is_ready(), "Expected Ready variant");
    let result_up_to = result_up_to.take_orders_info().unwrap();

    let decoded_up_to =
        takeOrders5Call::abi_decode(result_up_to.calldata()).expect("Should decode up_to calldata");
    let config_up_to = decoded_up_to.config;

    assert_eq!(
        config_up_to.minimumIO,
        Float::zero().unwrap().get_inner(),
        "SpendUpTo mode minimumIO should be zero"
    );

    let result_exact = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::SpendExact,
            amount: "200".to_string(),
            price_cap: high_price_cap(),
        })
        .await;

    assert!(
        matches!(
            result_exact,
            Err(RaindexError::InsufficientLiquidity { .. })
        ),
        "SpendExact mode should return InsufficientLiquidity when spend_budget > available capacity, got: {:?}",
        result_exact
    );
}

#[tokio::test]
async fn test_spend_mode_max_sell_cap_equals_spend_budget() {
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let spend_budget = "50".to_string();
    let price_cap = "10".to_string();

    let result_spend = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::SpendUpTo,
            amount: spend_budget.clone(),
            price_cap: price_cap.clone(),
        })
        .await
        .expect("Spend mode should succeed");
    assert!(
        result_spend.is_ready(),
        "Expected Ready variant for SpendUpTo"
    );
    let result_spend = result_spend.take_orders_info().unwrap();

    let result_buy = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: spend_budget.clone(),
            price_cap: price_cap.clone(),
        })
        .await
        .expect("Buy mode should succeed");
    assert!(result_buy.is_ready(), "Expected Ready variant for BuyUpTo");
    let result_buy = result_buy.take_orders_info().unwrap();

    let spend_budget_float = Float::parse(spend_budget.clone()).unwrap();
    let price_cap_float = Float::parse(price_cap).unwrap();

    assert!(
        result_spend.max_sell_cap().eq(spend_budget_float).unwrap(),
        "In spend mode, max_sell_cap should equal spend_budget ({}), got: {:?}",
        spend_budget,
        result_spend.max_sell_cap().format()
    );

    let expected_buy_max_sell_cap = spend_budget_float.mul(price_cap_float).unwrap();
    assert!(
        result_buy
            .max_sell_cap()
            .eq(expected_buy_max_sell_cap)
            .unwrap(),
        "In buy mode, max_sell_cap should equal buy_target * price_cap, got: {:?}",
        result_buy.max_sell_cap().format()
    );
}

#[tokio::test]
async fn test_spend_mode_cross_raindex_selection() {
    let setup = setup_multi_raindex_test().await;
    let sg_server = MockServer::start_async().await;

    let vault_id_a = B256::from(U256::from(1u64));
    let vault_id_b = B256::from(U256::from(2u64));

    let deposit_amount = U256::from(10).pow(U256::from(22));
    deposit_to_raindex(
        &setup,
        setup.raindex_a,
        setup.token2,
        deposit_amount,
        vault_id_a,
    )
    .await;
    deposit_to_raindex(
        &setup,
        setup.raindex_b,
        setup.token2,
        deposit_amount,
        vault_id_b,
    )
    .await;

    let dotrain_a = create_dotrain_config_for_raindex(&setup, setup.raindex_a, "0x01", "50", "2");
    let (order_bytes_a, order_hash_a, _order_v4_a) =
        deploy_order_to_raindex(&setup, setup.raindex_a, dotrain_a).await;

    let dotrain_b = create_dotrain_config_for_raindex(&setup, setup.raindex_b, "0x02", "80", "2");
    let (order_bytes_b, order_hash_b, order_v4_b) =
        deploy_order_to_raindex(&setup, setup.raindex_b, dotrain_b).await;

    let vault_a_input =
        create_vault_for_raindex(vault_id_a, &setup, setup.raindex_a, &setup.token1_sg);
    let vault_a_output =
        create_vault_for_raindex(vault_id_a, &setup, setup.raindex_a, &setup.token2_sg);
    let vault_b_input =
        create_vault_for_raindex(vault_id_b, &setup, setup.raindex_b, &setup.token1_sg);
    let vault_b_output =
        create_vault_for_raindex(vault_id_b, &setup, setup.raindex_b, &setup.token2_sg);

    let sg_order_a = create_sg_order_json_with_raindex(
        &setup,
        setup.raindex_a,
        &order_bytes_a,
        order_hash_a,
        vec![vault_a_input],
        vec![vault_a_output],
    );
    let sg_order_b = create_sg_order_json_with_raindex(
        &setup,
        setup.raindex_b,
        &order_bytes_b,
        order_hash_b,
        vec![vault_b_input],
        vec![vault_b_output],
    );

    sg_server.mock(|when, then| {
        when.path("/sg");
        then.status(200).json_body_obj(&json!({
            "data": {
                "orders": [sg_order_a, sg_order_b]
            }
        }));
    });

    let yaml = get_multi_raindex_yaml(
        123,
        &setup.local_evm.url(),
        &sg_server.url("/sg"),
        &setup.raindex_a.to_string(),
        &setup.raindex_b.to_string(),
    );

    let client = RaindexClient::new(vec![yaml], None, None).await.unwrap();

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_and_approve_taker_multi_raindex(
        &setup,
        setup.token1,
        taker,
        setup.raindex_b,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::SpendUpTo,
            amount: "160".to_string(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("Should succeed with spend mode across multiple raindexes");
    assert!(result.is_ready(), "Expected Ready variant");
    let result = result.take_orders_info().unwrap();

    let decoded = takeOrders5Call::abi_decode(result.calldata()).expect("Should decode calldata");
    let config = decoded.config;

    assert!(
        !config.IOIsInput,
        "IOIsInput should be false for spend mode"
    );

    assert_eq!(
        result.raindex(),
        setup.raindex_b,
        "Should select raindex B (can spend more: 80*2=160 vs 50*2=100)"
    );

    for config_item in &config.orders {
        let config_order = &config_item.order;
        assert_eq!(
            config_order.owner, order_v4_b.owner,
            "All orders should be from raindex B"
        );
    }
}

#[tokio::test]
async fn test_get_take_orders_calldata_returns_approval_when_no_allowance() {
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_taker(
        &setup,
        setup.token1,
        taker,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: "100".to_string(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("Should succeed with approval result");

    assert!(
        result.is_needs_approval(),
        "Expected NeedsApproval variant when taker has no allowance"
    );
    let approval = result.approval_info().unwrap();

    assert_eq!(
        approval.token(),
        setup.token1,
        "Approval token should be sell_token"
    );
    assert_eq!(
        approval.spender(),
        setup.raindex,
        "Approval spender should be raindex"
    );
    assert!(
        !approval.calldata().is_empty(),
        "Approval calldata should not be empty"
    );
}

#[tokio::test]
async fn test_get_take_orders_calldata_returns_approval_when_insufficient_allowance() {
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_taker(
        &setup,
        setup.token1,
        taker,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let insufficient_allowance = U256::from(10).pow(U256::from(18));
    approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        insufficient_allowance,
    )
    .await;

    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: "100".to_string(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("Should succeed with approval result");

    assert!(
        result.is_needs_approval(),
        "Expected NeedsApproval variant when allowance < max_sell_cap"
    );
    let approval = result.approval_info().unwrap();

    assert_eq!(
        approval.token(),
        setup.token1,
        "Approval token should be sell_token"
    );
    assert_eq!(
        approval.spender(),
        setup.raindex,
        "Approval spender should be raindex"
    );
}

#[tokio::test]
async fn test_get_take_orders_calldata_returns_take_orders_when_sufficient_allowance() {
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

    let taker = setup.local_evm.signer_wallets[1].default_signer().address();
    fund_taker(
        &setup,
        setup.token1,
        taker,
        U256::from(10).pow(U256::from(22)),
    )
    .await;

    let buy_target = Float::parse("100".to_string()).unwrap();
    let price_cap = Float::parse(high_price_cap()).unwrap();
    let max_sell_cap = buy_target.mul(price_cap).unwrap();
    let sufficient_allowance = max_sell_cap.to_fixed_decimal(18).unwrap();
    approve_taker(
        &setup,
        setup.token1,
        taker,
        setup.raindex,
        sufficient_allowance,
    )
    .await;

    let result = client
        .get_take_orders_calldata(TakeOrdersRequest {
            chain_id: 123,
            taker: taker.to_string(),
            sell_token: setup.token1.to_string(),
            buy_token: setup.token2.to_string(),
            mode: TakeOrdersMode::BuyUpTo,
            amount: "100".to_string(),
            price_cap: high_price_cap(),
        })
        .await
        .expect("Should succeed with take_orders result");

    assert!(
        result.is_ready(),
        "Expected Ready variant when allowance is sufficient"
    );
    let take_orders = result.take_orders_info().unwrap();

    assert_eq!(
        take_orders.raindex(),
        setup.raindex,
        "Raindex address should match"
    );
    assert!(
        !take_orders.calldata().is_empty(),
        "Calldata should not be empty"
    );
}
