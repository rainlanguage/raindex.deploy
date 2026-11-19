use crate::utils::timing::Timing;
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, Bytes, B256, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::serde::WithOtherFields;
use alloy::sol_types::SolCall;
use raindex_bindings::provider::ReadProvider;
use raindex_bindings::IRaindexV7::{takeOrders5Call, TakeOrdersConfigV6};
use raindex_bindings::IERC20::approveCall;
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::erc20::ERC20;

#[derive(Debug, Error)]
pub enum PreflightError {
    #[error("Balance check failed: {0}")]
    BalanceCheckFailed(String),
    #[error("Allowance check failed: {0}")]
    AllowanceCheckFailed(String),
    #[error("Simulation failed: {0}")]
    SimulationFailed(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
    #[error("ERC20 error: {0}")]
    Erc20Error(#[from] crate::erc20::Error),
}

#[derive(Debug, Clone)]
pub struct AllowanceOnlyResult {
    pub current_allowance: U256,
    pub required_amount: U256,
    pub needs_approval: bool,
    pub approval_amount: Option<U256>,
}

impl AllowanceOnlyResult {
    pub fn sufficient(allowance: U256, required: U256) -> Self {
        Self {
            current_allowance: allowance,
            required_amount: required,
            needs_approval: false,
            approval_amount: None,
        }
    }

    pub fn insufficient(allowance: U256, required: U256) -> Self {
        Self {
            current_allowance: allowance,
            required_amount: required,
            needs_approval: true,
            approval_amount: Some(required),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BalanceAndAllowanceResult {
    pub current_balance: U256,
    pub current_allowance: U256,
    pub required_amount: U256,
    pub needs_approval: bool,
    pub approval_amount: Option<U256>,
}

impl BalanceAndAllowanceResult {
    pub fn sufficient(balance: U256, allowance: U256, required: U256) -> Self {
        Self {
            current_balance: balance,
            current_allowance: allowance,
            required_amount: required,
            needs_approval: false,
            approval_amount: None,
        }
    }

    pub fn insufficient_allowance(balance: U256, allowance: U256, required: U256) -> Self {
        Self {
            current_balance: balance,
            current_allowance: allowance,
            required_amount: required,
            needs_approval: true,
            approval_amount: Some(required),
        }
    }
}

pub async fn check_taker_balance(
    erc20: &ERC20,
    taker: Address,
    required: U256,
) -> Result<U256, PreflightError> {
    let balance = erc20.get_account_balance(taker).await?;
    if balance < required {
        return Err(PreflightError::BalanceCheckFailed(format!(
            "Taker balance {} is less than required {}",
            balance, required
        )));
    }
    Ok(balance)
}

pub async fn check_taker_allowance(
    erc20: &ERC20,
    taker: Address,
    raindex: Address,
    required: U256,
) -> Result<AllowanceOnlyResult, PreflightError> {
    let allowance = erc20.allowance(taker, raindex).await?;
    if allowance >= required {
        Ok(AllowanceOnlyResult::sufficient(allowance, required))
    } else {
        Ok(AllowanceOnlyResult::insufficient(allowance, required))
    }
}

pub async fn check_taker_balance_and_allowance(
    erc20: &ERC20,
    taker: Address,
    raindex: Address,
    required: U256,
) -> Result<BalanceAndAllowanceResult, PreflightError> {
    let balance = erc20.get_account_balance(taker).await?;
    if balance < required {
        return Err(PreflightError::BalanceCheckFailed(format!(
            "Taker balance {} is less than required {}",
            balance, required
        )));
    }

    let allowance = erc20.allowance(taker, raindex).await?;
    if allowance >= required {
        Ok(BalanceAndAllowanceResult::sufficient(
            balance, allowance, required,
        ))
    } else {
        Ok(BalanceAndAllowanceResult::insufficient_allowance(
            balance, allowance, required,
        ))
    }
}

pub fn build_approval_calldata(spender: Address, amount: U256) -> Bytes {
    let call = approveCall { spender, amount };
    Bytes::from(call.abi_encode())
}

pub async fn simulate_take_orders(
    provider: &ReadProvider,
    raindex: Address,
    taker: Address,
    config: &TakeOrdersConfigV6,
    block_number: Option<u64>,
) -> Result<(), String> {
    let started_at = Timing::now();
    let order_count = config.orders.len();
    debug!(
        raindex = %raindex,
        taker = %taker,
        block_number = ?block_number,
        order_count,
        "starting take-orders preflight simulation"
    );
    let calldata = takeOrders5Call {
        config: config.clone(),
    }
    .abi_encode();

    let tx = TransactionRequest::default()
        .with_to(raindex)
        .with_from(taker)
        .with_input(calldata);

    let tx = WithOtherFields::new(tx);

    let result = provider
        .call(tx)
        .block(
            block_number
                .map(alloy::eips::BlockId::number)
                .unwrap_or(alloy::eips::BlockId::latest()),
        )
        .await;

    match result {
        Ok(_) => {
            info!(
                raindex = %raindex,
                taker = %taker,
                block_number = ?block_number,
                order_count,
                simulation_success = true,
                duration_ms = started_at.elapsed_ms(),
                "take-orders preflight simulation succeeded"
            );
            Ok(())
        }
        Err(e) => {
            let error = e.to_string();
            warn!(
                raindex = %raindex,
                taker = %taker,
                block_number = ?block_number,
                order_count,
                simulation_success = false,
                error = %error.chars().take(512).collect::<String>(),
                duration_ms = started_at.elapsed_ms(),
                "take-orders preflight simulation failed"
            );
            Err(error)
        }
    }
}

pub async fn find_failing_order_index(
    provider: &ReadProvider,
    raindex: Address,
    taker: Address,
    config: &TakeOrdersConfigV6,
    block_number: Option<u64>,
) -> Option<usize> {
    match diagnose_take_orders_failure(provider, raindex, taker, config, block_number).await {
        TakeOrdersFailureDiagnosis::FailingOrder(index) => Some(index),
        TakeOrdersFailureDiagnosis::Recovered | TakeOrdersFailureDiagnosis::Unidentified => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TakeOrdersFailureDiagnosis {
    FailingOrder(usize),
    Recovered,
    Unidentified,
}

pub(crate) async fn diagnose_take_orders_failure(
    provider: &ReadProvider,
    raindex: Address,
    taker: Address,
    config: &TakeOrdersConfigV6,
    block_number: Option<u64>,
) -> TakeOrdersFailureDiagnosis {
    let started_at = Timing::now();
    if config.orders.is_empty() {
        debug!(raindex = %raindex, "cannot find failing order in empty config");
        return TakeOrdersFailureDiagnosis::Unidentified;
    }

    if config.orders.len() == 1 {
        let first_retry =
            simulate_take_orders(provider, raindex, taker, config, block_number).await;
        if first_retry.is_err()
            && simulate_take_orders(provider, raindex, taker, config, block_number)
                .await
                .is_err()
        {
            warn!(
                raindex = %raindex,
                taker = %taker,
                block_number = ?block_number,
                failing_order_index = 0usize,
                simulations_run = 2usize,
                duration_ms = started_at.elapsed_ms(),
                "identified failing order"
            );
            return TakeOrdersFailureDiagnosis::FailingOrder(0);
        }
        return TakeOrdersFailureDiagnosis::Recovered;
    }

    let mut simulations_run = 0usize;
    for idx in 0..config.orders.len() {
        let single_order_config = single_order_probe_config(config, idx);

        let result =
            simulate_take_orders(provider, raindex, taker, &single_order_config, block_number)
                .await;
        simulations_run += 1;

        if result.is_err() {
            let retry =
                simulate_take_orders(provider, raindex, taker, &single_order_config, block_number)
                    .await;
            simulations_run += 1;
            if retry.is_err() {
                warn!(
                    raindex = %raindex,
                    taker = %taker,
                    block_number = ?block_number,
                    failing_order_index = idx,
                    simulations_run,
                    duration_ms = started_at.elapsed_ms(),
                    "identified failing order"
                );
                return TakeOrdersFailureDiagnosis::FailingOrder(idx);
            }
        }
    }

    if simulate_take_orders(provider, raindex, taker, config, block_number)
        .await
        .is_ok()
    {
        info!(
            raindex = %raindex,
            taker = %taker,
            block_number = ?block_number,
            simulations_run,
            duration_ms = started_at.elapsed_ms(),
            "take-orders preflight recovered during failure diagnosis"
        );
        return TakeOrdersFailureDiagnosis::Recovered;
    }

    warn!(
        raindex = %raindex,
        taker = %taker,
        block_number = ?block_number,
        simulations_run,
        duration_ms = started_at.elapsed_ms(),
        "could not identify failing order"
    );
    TakeOrdersFailureDiagnosis::Unidentified
}

fn single_order_probe_config(config: &TakeOrdersConfigV6, idx: usize) -> TakeOrdersConfigV6 {
    TakeOrdersConfigV6 {
        // `minimumIO` is an aggregate constraint. Keeping the batch minimum
        // when probing one leg would make healthy partial legs fail in exact
        // modes and incorrectly identify them as the cause of the revert.
        minimumIO: B256::ZERO,
        maximumIO: config.maximumIO,
        maximumIORatio: config.maximumIORatio,
        IOIsInput: config.IOIsInput,
        orders: vec![config.orders[idx].clone()],
        data: config.data.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raindex_bindings::IRaindexV7::TakeOrderConfigV5;

    #[test]
    fn test_preflight_error_display() {
        let err = PreflightError::BalanceCheckFailed("test".to_string());
        assert!(err.to_string().contains("Balance check failed"));

        let err = PreflightError::AllowanceCheckFailed("test".to_string());
        assert!(err.to_string().contains("Allowance check failed"));

        let err = PreflightError::SimulationFailed("test".to_string());
        assert!(err.to_string().contains("Simulation failed"));
    }

    #[test]
    fn test_allowance_only_result_needs_approval_when_insufficient() {
        let result = AllowanceOnlyResult::insufficient(U256::from(50), U256::from(100));
        assert!(result.needs_approval);
        assert_eq!(result.approval_amount, Some(U256::from(100)));
    }

    #[test]
    fn test_allowance_only_result_no_approval_when_sufficient() {
        let result = AllowanceOnlyResult::sufficient(U256::from(100), U256::from(100));
        assert!(!result.needs_approval);
        assert_eq!(result.approval_amount, None);
    }

    #[test]
    fn test_balance_and_allowance_result_needs_approval_when_insufficient() {
        let result = BalanceAndAllowanceResult::insufficient_allowance(
            U256::from(1000),
            U256::from(50),
            U256::from(100),
        );
        assert!(result.needs_approval);
        assert_eq!(result.approval_amount, Some(U256::from(100)));
    }

    #[test]
    fn test_balance_and_allowance_result_no_approval_when_sufficient() {
        let result = BalanceAndAllowanceResult::sufficient(
            U256::from(1000),
            U256::from(100),
            U256::from(100),
        );
        assert!(!result.needs_approval);
        assert_eq!(result.approval_amount, None);
    }

    #[test]
    fn test_build_approval_calldata() {
        let spender = Address::ZERO;
        let amount = U256::from(1000);
        let calldata = build_approval_calldata(spender, amount);
        assert!(!calldata.is_empty());
        let decoded = approveCall::abi_decode(&calldata);
        assert!(decoded.is_ok());
        let decoded = decoded.unwrap();
        assert_eq!(decoded.spender, spender);
        assert_eq!(decoded.amount, amount);
    }

    #[test]
    fn single_order_probe_drops_aggregate_minimum_io() {
        let order = TakeOrderConfigV5::default();
        let config = TakeOrdersConfigV6 {
            minimumIO: U256::from(10).into(),
            maximumIO: U256::from(20).into(),
            maximumIORatio: U256::from(2).into(),
            IOIsInput: true,
            orders: vec![order.clone(), TakeOrderConfigV5::default()],
            data: Bytes::from(vec![1]),
        };

        let probe = single_order_probe_config(&config, 0);

        assert_eq!(probe.minimumIO, B256::ZERO);
        assert_eq!(probe.maximumIO, config.maximumIO);
        assert_eq!(probe.maximumIORatio, config.maximumIORatio);
        assert_eq!(probe.IOIsInput, config.IOIsInput);
        assert_eq!(probe.orders, vec![order]);
        assert_eq!(probe.data, config.data);
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod local_evm_tests {
    use super::*;
    use crate::erc20::ERC20;
    use alloy::primitives::B256;
    use raindex_bindings::provider::mk_read_provider;
    use raindex_bindings::IRaindexV7::TakeOrderConfigV5;
    use raindex_test_fixtures::LocalEvm;
    use url::Url;

    async fn setup_local_evm() -> (LocalEvm, Address, Address, Address) {
        let mut local_evm = LocalEvm::new().await;
        let owner = local_evm.signer_wallets[0].default_signer().address();
        let token = local_evm
            .deploy_new_token("TestToken", "TT", 18, U256::MAX, owner)
            .await;
        let raindex = *local_evm.raindex.address();
        (local_evm, owner, *token.address(), raindex)
    }

    fn create_erc20(local_evm: &LocalEvm, token: Address) -> ERC20 {
        let rpc_url = Url::parse(&local_evm.url().to_string()).unwrap();
        ERC20::new(vec![rpc_url], token)
    }

    #[tokio::test]
    async fn test_check_taker_balance_sufficient() {
        let (local_evm, owner, token, _raindex) = setup_local_evm().await;
        let erc20 = create_erc20(&local_evm, token);

        let result = check_taker_balance(&erc20, owner, U256::from(1000)).await;

        assert!(result.is_ok());
        let balance = result.unwrap();
        assert!(balance >= U256::from(1000));
    }

    #[tokio::test]
    async fn test_check_taker_balance_insufficient() {
        let (local_evm, _owner, token, _raindex) = setup_local_evm().await;
        let erc20 = create_erc20(&local_evm, token);

        let random_address = Address::repeat_byte(0x42);
        let result = check_taker_balance(&erc20, random_address, U256::from(1000)).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, PreflightError::BalanceCheckFailed(_)));
    }

    #[tokio::test]
    async fn test_check_taker_allowance_sufficient() {
        let (local_evm, owner, token, raindex) = setup_local_evm().await;

        let token_contract = local_evm
            .tokens
            .iter()
            .find(|t| *t.address() == token)
            .unwrap();
        token_contract
            .approve(raindex, U256::from(1000))
            .from(owner)
            .send()
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap();

        let erc20 = create_erc20(&local_evm, token);
        let result = check_taker_allowance(&erc20, owner, raindex, U256::from(500)).await;

        assert!(result.is_ok());
        let check_result = result.unwrap();
        assert!(!check_result.needs_approval);
        assert!(check_result.current_allowance >= U256::from(500));
    }

    #[tokio::test]
    async fn test_check_taker_allowance_insufficient() {
        let (local_evm, owner, token, raindex) = setup_local_evm().await;
        let erc20 = create_erc20(&local_evm, token);

        let result = check_taker_allowance(&erc20, owner, raindex, U256::from(1000)).await;

        assert!(result.is_ok());
        let check_result = result.unwrap();
        assert!(check_result.needs_approval);
        assert_eq!(check_result.approval_amount, Some(U256::from(1000)));
    }

    #[tokio::test]
    async fn test_check_taker_balance_and_allowance_both_sufficient() {
        let (local_evm, owner, token, raindex) = setup_local_evm().await;

        let token_contract = local_evm
            .tokens
            .iter()
            .find(|t| *t.address() == token)
            .unwrap();
        token_contract
            .approve(raindex, U256::from(1000))
            .from(owner)
            .send()
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap();

        let erc20 = create_erc20(&local_evm, token);
        let result =
            check_taker_balance_and_allowance(&erc20, owner, raindex, U256::from(500)).await;

        assert!(result.is_ok());
        let check_result = result.unwrap();
        assert!(!check_result.needs_approval);
        assert!(check_result.current_balance >= U256::from(500));
        assert!(check_result.current_allowance >= U256::from(500));
    }

    #[tokio::test]
    async fn test_check_taker_balance_and_allowance_insufficient_balance() {
        let (local_evm, _owner, token, raindex) = setup_local_evm().await;
        let erc20 = create_erc20(&local_evm, token);

        let random_address = Address::repeat_byte(0x42);
        let result =
            check_taker_balance_and_allowance(&erc20, random_address, raindex, U256::from(1000))
                .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PreflightError::BalanceCheckFailed(_)
        ));
    }

    #[tokio::test]
    async fn test_check_taker_balance_and_allowance_insufficient_allowance() {
        let (local_evm, owner, token, raindex) = setup_local_evm().await;
        let erc20 = create_erc20(&local_evm, token);

        let result =
            check_taker_balance_and_allowance(&erc20, owner, raindex, U256::from(1000)).await;

        assert!(result.is_ok());
        let check_result = result.unwrap();
        assert!(check_result.needs_approval);
        assert!(check_result.current_balance >= U256::from(1000));
        assert_eq!(check_result.current_allowance, U256::ZERO);
    }

    #[tokio::test]
    async fn test_simulate_take_orders_empty_orders_fails_minimum_io() {
        let (local_evm, owner, _token, raindex) = setup_local_evm().await;

        let rpc_url = Url::parse(&local_evm.url().to_string()).unwrap();
        let provider = mk_read_provider(&[rpc_url]).unwrap();

        let config = TakeOrdersConfigV6 {
            minimumIO: B256::ZERO,
            maximumIO: U256::MAX.into(),
            maximumIORatio: U256::MAX.into(),
            IOIsInput: false,
            orders: vec![],
            data: Bytes::new(),
        };

        let result = simulate_take_orders(&provider, raindex, owner, &config, None).await;
        assert!(
            result.is_err(),
            "Empty orders config should fail simulation"
        );
    }

    #[tokio::test]
    async fn test_simulate_take_orders_invalid_order_fails() {
        let (local_evm, owner, _token, raindex) = setup_local_evm().await;

        let rpc_url = Url::parse(&local_evm.url().to_string()).unwrap();
        let provider = mk_read_provider(&[rpc_url]).unwrap();

        let fake_order = TakeOrderConfigV5 {
            order: raindex_bindings::IRaindexV7::OrderV4 {
                owner: Address::ZERO,
                evaluable: raindex_bindings::IRaindexV7::EvaluableV4 {
                    interpreter: Address::ZERO,
                    store: Address::ZERO,
                    bytecode: Bytes::new(),
                },
                validInputs: vec![],
                validOutputs: vec![],
                nonce: B256::ZERO,
            },
            inputIOIndex: U256::ZERO,
            outputIOIndex: U256::ZERO,
            signedContext: vec![],
        };

        let config = TakeOrdersConfigV6 {
            minimumIO: B256::ZERO,
            maximumIO: U256::MAX.into(),
            maximumIORatio: U256::MAX.into(),
            IOIsInput: false,
            orders: vec![fake_order],
            data: Bytes::new(),
        };

        let result = simulate_take_orders(&provider, raindex, owner, &config, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_find_failing_order_index_empty_orders() {
        let (local_evm, owner, _token, raindex) = setup_local_evm().await;

        let rpc_url = Url::parse(&local_evm.url().to_string()).unwrap();
        let provider = mk_read_provider(&[rpc_url]).unwrap();

        let config = TakeOrdersConfigV6 {
            minimumIO: B256::ZERO,
            maximumIO: U256::MAX.into(),
            maximumIORatio: U256::MAX.into(),
            IOIsInput: false,
            orders: vec![],
            data: Bytes::new(),
        };

        let result = find_failing_order_index(&provider, raindex, owner, &config, None).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_find_failing_order_index_single_failing_order() {
        let (local_evm, owner, _token, raindex) = setup_local_evm().await;

        let rpc_url = Url::parse(&local_evm.url().to_string()).unwrap();
        let provider = mk_read_provider(&[rpc_url]).unwrap();

        let fake_order = TakeOrderConfigV5 {
            order: raindex_bindings::IRaindexV7::OrderV4 {
                owner: Address::ZERO,
                evaluable: raindex_bindings::IRaindexV7::EvaluableV4 {
                    interpreter: Address::ZERO,
                    store: Address::ZERO,
                    bytecode: Bytes::new(),
                },
                validInputs: vec![],
                validOutputs: vec![],
                nonce: B256::ZERO,
            },
            inputIOIndex: U256::ZERO,
            outputIOIndex: U256::ZERO,
            signedContext: vec![],
        };

        let config = TakeOrdersConfigV6 {
            minimumIO: B256::ZERO,
            maximumIO: U256::MAX.into(),
            maximumIORatio: U256::MAX.into(),
            IOIsInput: false,
            orders: vec![fake_order],
            data: Bytes::new(),
        };

        let result = find_failing_order_index(&provider, raindex, owner, &config, None).await;
        assert_eq!(result, Some(0));
    }
}
