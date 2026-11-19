use crate::QuoteTarget;

use alloy::{primitives::Address, sol_types::SolCall};
use rain_error_decoding::{AbiDecodeFailedErrors, AbiDecodedErrorType};
use rain_interpreter_eval::{
    error::ForkCallError,
    fork::{Forker, NewForkedEvm},
    trace::{RainEvalResult, RainEvalResultFromRawCallResultError},
};
use raindex_bindings::IRaindexV7::quote3Call;
use url::Url;

pub struct NewQuoteDebugger {
    pub fork_url: Url,
    pub fork_block_number: Option<u64>,
}
pub struct QuoteDebugger {
    forker: Forker,
}

#[derive(Debug, thiserror::Error)]
pub enum QuoteDebuggerError {
    #[error("Forker error: {0}")]
    ForkerError(Box<ForkCallError>),
    #[error("Quote error: {0}")]
    QuoteError(#[from] crate::error::Error),
    #[error(transparent)]
    RainEvalResultConversion(#[from] RainEvalResultFromRawCallResultError),
}

impl From<ForkCallError> for QuoteDebuggerError {
    fn from(err: ForkCallError) -> Self {
        Self::ForkerError(Box::new(err))
    }
}

impl QuoteDebugger {
    pub async fn new(args: NewQuoteDebugger) -> Result<Self, QuoteDebuggerError> {
        let forker = Forker::new_with_fork(
            NewForkedEvm {
                fork_url: args.fork_url.to_string(),
                fork_block_number: args.fork_block_number,
            },
            None,
            None,
        )
        .await?;

        Ok(Self { forker })
    }

    pub async fn debug(
        &mut self,
        quote_target: QuoteTarget,
    ) -> Result<
        (
            RainEvalResult,
            Option<Result<AbiDecodedErrorType, AbiDecodeFailedErrors>>,
        ),
        QuoteDebuggerError,
    > {
        quote_target.validate()?;

        let quote_call = quote3Call {
            quoteConfig: quote_target.quote_config.clone(),
        };

        let res = self.forker.call(
            Address::default().as_slice(),
            quote_target.raindex.as_slice(),
            &quote_call.abi_encode(),
        )?;

        let mut abi_decoded_error = None;
        if res.exit_reason.is_revert() {
            abi_decoded_error =
                Some(AbiDecodedErrorType::selector_registry_abi_decode(&res.result, None).await);
        }

        Ok((res.try_into()?, abi_decoded_error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::utils::parse_ether;
    use alloy::primitives::{fixed_bytes, U256};
    use alloy::sol_types::{SolCall, SolValue};
    use httpmock::MockServer;
    use raindex_app_settings::spec_version::SpecVersion;
    use raindex_bindings::IRaindexV7::{OrderV4, QuoteV3};
    use raindex_common::add_order::AddOrderArgs;
    use raindex_common::dotrain_order::DotrainOrder;
    use raindex_test_fixtures::LocalEvm;
    use std::str::FromStr;

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_quote_debugger() {
        let local_evm = LocalEvm::new_with_tokens(2).await;

        let raindex = &local_evm.raindex;
        let token1_holder = local_evm.signer_wallets[0].default_signer().address();
        let token1 = local_evm.tokens[0].clone();
        let token2 = local_evm.tokens[1].clone();

        let dotrain = format!(
            r#"
version: {spec_version}
networks:
    some-key:
        rpcs:
            - {rpc_url}
        chain-id: 123
        network-id: 123
        currency: ETH
rainlangs:
    some-key:
        address: {rainlang_address}
tokens:
    t1:
        network: some-key
        address: {token2}
        decimals: 18
        label: Token2
        symbol: Token2
    t2:
        network: some-key
        address: {token1}
        decimals: 18
        label: Token1
        symbol: token1
raindex:
    some-key:
        address: {raindex}
orders:
    some-key:
        inputs:
            - token: t1
        outputs:
            - token: t2
              vault-id: 0x01
scenarios:
    some-key:
        rainlang: some-key
        bindings:
            key1: 10
deployments:
    some-key:
        scenario: some-key
        order: some-key
---
#key1 !Test binding
#calculate-io
amount price: 16 52;
#handle-add-order
:;
#handle-io
:;
"#,
            rpc_url = local_evm.url(),
            raindex = raindex.address(),
            rainlang_address = local_evm.rainlang,
            token1 = token1.address(),
            token2 = token2.address(),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.clone(), None).await.unwrap();
        let deployment = dotrain_order
            .dotrain_yaml()
            .get_deployment("some-key")
            .unwrap();
        let calldata = AddOrderArgs::new_from_deployment(dotrain, deployment, None)
            .await
            .unwrap()
            .try_into_call(vec![local_evm.url()])
            .await
            .unwrap()
            .abi_encode();

        let vault_id =
            fixed_bytes!("0x0000000000000000000000000000000000000000000000000000000000000001");

        let order = local_evm
            .add_order_and_deposit(
                &calldata,
                token1_holder,
                *token1.address(),
                parse_ether("1000").unwrap(),
                18,
                vault_id,
            )
            .await
            .0
            .order;

        let mut debugger = QuoteDebugger::new(NewQuoteDebugger {
            fork_url: Url::from_str(&local_evm.url()).unwrap(),
            fork_block_number: None,
        })
        .await
        .unwrap();

        let order = OrderV4::abi_decode(&order.abi_encode()).unwrap();

        let quote_target = QuoteTarget {
            raindex: *raindex.address(),
            quote_config: QuoteV3 {
                order,
                inputIOIndex: U256::from(0),
                outputIOIndex: U256::from(0),
                signedContext: vec![],
            },
        };

        let res = debugger.debug(quote_target).await.unwrap();

        assert_eq!(res.0.traces.len(), 1);
        assert_eq!(res.0.traces[0].stack, vec![U256::from(52), U256::from(16)]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_quote_debugger_partial() {
        let local_evm = LocalEvm::new_with_tokens(2).await;

        let raindex = &local_evm.raindex;
        let token1_holder = local_evm.signer_wallets[0].default_signer().address();
        let token1 = local_evm.tokens[0].clone();
        let token2 = local_evm.tokens[1].clone();

        let dotrain = format!(
            r#"
version: {spec_version}
networks:
    some-key:
        rpcs:
            - {rpc_url}
        chain-id: 123
        network-id: 123
        currency: ETH
rainlangs:
    some-key:
        address: {rainlang_address}
tokens:
    t1:
        network: some-key
        address: {token2}
        decimals: 18
        label: Token2
        symbol: Token2
    t2:
        network: some-key
        address: {token1}
        decimals: 18
        label: Token1
        symbol: token1
raindex:
    some-key:
        address: {raindex}
orders:
    some-key:
        inputs:
            - token: t1
        outputs:
            - token: t2
              vault-id: 0x01
scenarios:
    some-key:
        rainlang: some-key
        bindings:
            key1: 10
deployments:
    some-key:
        scenario: some-key
        order: some-key
---
#key1 !Test binding
#calculate-io
amount price: 16 52,
current-time: call<'some-source>(),
_: 123,
_ _: amount price;
#handle-add-order
:;
#handle-io
:;
#some-source
_: 1;
"#,
            rpc_url = local_evm.url(),
            raindex = raindex.address(),
            rainlang_address = local_evm.rainlang,
            token1 = token1.address(),
            token2 = token2.address(),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.clone(), None).await.unwrap();
        let deployment = dotrain_order
            .dotrain_yaml()
            .get_deployment("some-key")
            .unwrap();
        let calldata = AddOrderArgs::new_from_deployment(dotrain, deployment, None)
            .await
            .unwrap()
            .try_into_call(vec![local_evm.url()])
            .await
            .unwrap()
            .abi_encode();

        let vault_id =
            fixed_bytes!("0x0000000000000000000000000000000000000000000000000000000000000001");

        let order = local_evm
            .add_order_and_deposit(
                &calldata,
                token1_holder,
                *token1.address(),
                parse_ether("1000").unwrap(),
                18,
                vault_id,
            )
            .await
            .0
            .order;

        let mut debugger = QuoteDebugger::new(NewQuoteDebugger {
            fork_url: Url::from_str(&local_evm.url()).unwrap(),
            fork_block_number: None,
        })
        .await
        .unwrap();

        let order = OrderV4::abi_decode(&order.abi_encode()).unwrap();

        let quote_target = QuoteTarget {
            raindex: *raindex.address(),
            quote_config: QuoteV3 {
                order,
                inputIOIndex: U256::from(0),
                outputIOIndex: U256::from(0),
                signedContext: vec![],
            },
        };

        let res = debugger.debug(quote_target).await.unwrap();

        assert_eq!(res.0.traces.len(), 2);
        assert_eq!(
            res.0.traces[0].stack,
            vec![52, 16, 123, 1, 52, 16]
                .into_iter()
                .map(U256::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(res.0.traces[1].stack, vec![U256::from(1)]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_quote_debugger_debug_err() {
        let local_evm = LocalEvm::new_with_tokens(2).await;

        let raindex = &local_evm.raindex;
        let token1_holder = local_evm.signer_wallets[0].default_signer().address();
        let token1 = local_evm.tokens[0].clone();
        let token2 = local_evm.tokens[1].clone();

        let dotrain = format!(
            r#"
version: {spec_version}
networks:
    some-key:
        rpcs:
            - {rpc_url}
        chain-id: 123
        network-id: 123
        currency: ETH
rainlangs:
    some-key:
        address: {rainlang_address}
tokens:
    t1:
        network: some-key
        address: {token2}
        decimals: 18
        label: Token2
        symbol: Token2
    t2:
        network: some-key
        address: {token1}
        decimals: 18
        label: Token1
        symbol: token1
raindex:
    some-key:
        address: {raindex}
orders:
    some-key:
        inputs:
            - token: t1
        outputs:
            - token: t2
              vault-id: 0x01
scenarios:
    some-key:
        rainlang: some-key
        bindings:
            key1: 10
deployments:
    some-key:
        scenario: some-key
        order: some-key
---
#key1 !Test binding
#calculate-io
amount price: 16 52,
current-time: call<'some-source>(),
_: 123,
_ _: amount price;
#handle-add-order
:;
#handle-io
:;
#some-source
_: 1;
"#,
            rpc_url = local_evm.url(),
            raindex = raindex.address(),
            rainlang_address = local_evm.rainlang,
            token1 = token1.address(),
            token2 = token2.address(),
            spec_version = SpecVersion::current(),
        );

        let dotrain_order = DotrainOrder::create(dotrain.clone(), None).await.unwrap();
        let deployment = dotrain_order
            .dotrain_yaml()
            .get_deployment("some-key")
            .unwrap();
        let calldata = AddOrderArgs::new_from_deployment(dotrain, deployment, None)
            .await
            .unwrap()
            .try_into_call(vec![local_evm.url()])
            .await
            .unwrap()
            .abi_encode();

        let vault_id =
            fixed_bytes!("0x0000000000000000000000000000000000000000000000000000000000000001");

        let order = local_evm
            .add_order_and_deposit(
                &calldata,
                token1_holder,
                *token1.address(),
                parse_ether("1000").unwrap(),
                18,
                vault_id,
            )
            .await
            .0
            .order;

        let mut debugger = QuoteDebugger::new(NewQuoteDebugger {
            fork_url: Url::from_str(&local_evm.url()).unwrap(),
            fork_block_number: None,
        })
        .await
        .unwrap();

        let order = OrderV4::abi_decode(&order.abi_encode()).unwrap();

        let quote_target = QuoteTarget {
            raindex: *raindex.address(),
            quote_config: QuoteV3 {
                order,
                inputIOIndex: U256::from(1),
                outputIOIndex: U256::from(2),
                signedContext: vec![],
            },
        };

        let err = debugger.debug(quote_target).await.unwrap_err();

        assert!(matches!(
            err,
            QuoteDebuggerError::QuoteError(crate::error::Error::InvalidQuoteTarget(target))
            if target == U256::from(1)
        ));
    }

    #[tokio::test]
    async fn test_quote_debugger_new_err() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.path("/rpc");
            then.status(400);
        });

        let res = QuoteDebugger::new(NewQuoteDebugger {
            fork_url: Url::from_str(&server.url("/rpc")).unwrap(),
            fork_block_number: None,
        })
        .await;

        assert!(matches!(
            res,
            Err(QuoteDebuggerError::ForkerError(err))
            if err.to_string().to_lowercase().contains("http error 400")
        ));
    }
}
