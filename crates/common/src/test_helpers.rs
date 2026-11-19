use raindex_app_settings::spec_version::SpecVersion;

pub fn test_dotrain() -> String {
    format!(
        r#"
version: {version}
networks:
    mainnet:
        rpcs:
            - https://mainnet.infura.io
        chain-id: 1
    testnet:
        rpcs:
            - https://testnet.infura.io
        chain-id: 1337
subgraphs:
    mainnet: https://mainnet-subgraph.com
    testnet: https://testnet-subgraph.com
metaboards:
    mainnet: https://mainnet-metaboard.com
    testnet: https://testnet-metaboard.com
raindexes:
    mainnet:
        address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
        network: mainnet
        subgraph: mainnet
    testnet:
        address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
        network: testnet
        subgraph: testnet
tokens:
    token1:
        network: mainnet
        address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
        decimals: 18
        label: Wrapped Ether
        symbol: WETH
    token2:
        network: mainnet
        address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
        decimals: 6
        label: USD Coin
        symbol: USDC
rainlangs:
    scenario1:
        address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
        network: mainnet
    rainlang2:
        address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
        network: testnet
sentry: true
accounts:
    account1: 0x0000000000000000000000000000000000000001
    account2: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
orders:
    order1:
        rainlang: scenario1
        raindex: mainnet
        inputs:
            - token: token1
              vault-id: 1
        outputs:
            - token: token2
              vault-id: 2
scenarios:
    scenario1:
        bindings:
            key1: 10
        scenarios:
            scenario2:
                bindings:
                    key2: 20
                runs: 10
deployments:
    deployment1:
        order: order1
        scenario: scenario1.scenario2
    deployment2:
        order: order1
        scenario: scenario1
builder:
    name: Test builder
    description: Test description
    short-description: Test short description
    deployments:
        deployment1:
            name: Test deployment
            description: Test description
            deposits:
                - token: token1
                  presets:
                    - 100
                    - 2000
            fields:
                - binding: key1
                  name: Binding test
                  presets:
                    - value: value2
            select-tokens:
                - key: token2
                  name: Test token
                  description: Test description
charts:
    chart1:
        scenario: scenario1.scenario2
        plots:
            plot1:
                title: Test title
                subtitle: Test subtitle
                marks:
                    - type: dot
                      options:
                        x: 1
                        y: 2
                        r: 3
                        fill: red
                        stroke: blue
                        transform:
                            type: hexbin
                            content:
                                outputs:
                                    x: 1
                                    y: 2
                                    r: 3
                                    z: 4
                                    stroke: green
                                    fill: blue
                                options:
                                    x: 1
                                    y: 2
                                    bin-width: 10
                    - type: line
                      options:
                        transform:
                            type: binx
                            content:
                                outputs:
                                    x: 1
                                options:
                                    thresholds: 10
                    - type: recty
                      options:
                        x0: 1
                        x1: 2
                        y0: 3
                        y1: 4
                x:
                   label: Test x label
                   anchor: start
                   label-anchor: start
                   label-arrow: none
                y:
                   label: Test y label
                   anchor: start
                   label-anchor: start
                   label-arrow: none
                margin: 10
                margin-left: 20
                margin-right: 30
                margin-top: 40
                margin-bottom: 50
                inset: 60
---
#key1 !Test binding
#key2 !Test binding
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
:;
"#,
        version = SpecVersion::current()
    )
}

#[cfg(not(target_family = "wasm"))]
pub mod local_evm {
    use alloy::primitives::{Address, B256, U256};
    use rain_math_float::Float;
    use raindex_subgraph_client::types::common::{SgBigInt, SgBytes, SgErc20, SgRaindex, SgVault};
    use raindex_test_fixtures::LocalEvm;

    pub struct TestSetup {
        pub local_evm: LocalEvm,
        pub owner: Address,
        pub token1: Address,
        pub token2: Address,
        pub token1_sg: SgErc20,
        pub token2_sg: SgErc20,
        pub raindex: Address,
    }

    pub async fn setup_test() -> TestSetup {
        let mut local_evm = LocalEvm::new().await;
        let owner = local_evm.signer_wallets[0].default_signer().address();

        let token1 = local_evm
            .deploy_new_token("Token1", "Token1", 18, U256::MAX, owner)
            .await;
        let token2 = local_evm
            .deploy_new_token("Token2", "Token2", 18, U256::MAX, owner)
            .await;
        let raindex = *local_evm.raindex.address();

        TestSetup {
            token1: *token1.address(),
            token2: *token2.address(),
            token1_sg: SgErc20 {
                id: SgBytes(token1.address().to_string()),
                address: SgBytes(token1.address().to_string()),
                name: Some("Token1".to_string()),
                symbol: Some("Token1".to_string()),
                decimals: Some(SgBigInt(18.to_string())),
            },
            token2_sg: SgErc20 {
                id: SgBytes(token2.address().to_string()),
                address: SgBytes(token2.address().to_string()),
                name: Some("Token2".to_string()),
                symbol: Some("Token2".to_string()),
                decimals: Some(SgBigInt(18.to_string())),
            },
            local_evm,
            owner,
            raindex,
        }
    }

    pub fn standard_deposit_amount() -> U256 {
        U256::from(10).pow(U256::from(20))
    }

    pub async fn fund_standard_two_token_vault(setup: &TestSetup, vault_id: B256) {
        let amount = standard_deposit_amount();
        setup
            .local_evm
            .deposit(setup.owner, setup.token1, amount, 18, vault_id)
            .await;
        setup
            .local_evm
            .deposit(setup.owner, setup.token2, amount, 18, vault_id)
            .await;
    }

    pub async fn fund_taker(setup: &TestSetup, token: Address, taker: Address, amount: U256) {
        let token_contract = setup
            .local_evm
            .tokens
            .iter()
            .find(|t| *t.address() == token)
            .expect("Token should exist in setup.local_evm.tokens");

        token_contract
            .transfer(taker, amount)
            .from(setup.owner)
            .send()
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap();
    }

    pub async fn approve_taker(
        setup: &TestSetup,
        token: Address,
        taker: Address,
        spender: Address,
        amount: U256,
    ) {
        let token_contract = setup
            .local_evm
            .tokens
            .iter()
            .find(|t| *t.address() == token)
            .expect("Token should exist in setup.local_evm.tokens");

        token_contract
            .approve(spender, amount)
            .from(taker)
            .send()
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap();
    }

    pub async fn fund_and_approve_taker(
        setup: &TestSetup,
        token: Address,
        taker: Address,
        spender: Address,
        amount: U256,
    ) {
        fund_taker(setup, token, taker, amount).await;
        approve_taker(setup, token, taker, spender, U256::MAX).await;
    }

    pub async fn fund_and_approve_taker_multi_raindex(
        setup: &MultiRaindexTestSetup,
        token: Address,
        taker: Address,
        spender: Address,
        amount: U256,
    ) {
        let token_contract = setup
            .local_evm
            .tokens
            .iter()
            .find(|t| *t.address() == token)
            .expect("Token should exist in setup.local_evm.tokens");

        token_contract
            .transfer(taker, amount)
            .from(setup.owner)
            .send()
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap();

        token_contract
            .approve(spender, U256::MAX)
            .from(taker)
            .send()
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap();
    }

    pub struct MultiRaindexTestSetup {
        pub local_evm: LocalEvm,
        pub owner: Address,
        pub token1: Address,
        pub token2: Address,
        pub token1_sg: SgErc20,
        pub token2_sg: SgErc20,
        pub raindex_a: Address,
        pub raindex_b: Address,
    }

    pub async fn setup_multi_raindex_test() -> MultiRaindexTestSetup {
        use raindex_test_fixtures::Raindex;

        let mut local_evm = LocalEvm::new().await;
        let owner = local_evm.signer_wallets[0].default_signer().address();

        let token1 = local_evm
            .deploy_new_token("Token1", "Token1", 18, U256::MAX, owner)
            .await;
        let token2 = local_evm
            .deploy_new_token("Token2", "Token2", 18, U256::MAX, owner)
            .await;

        let raindex_a = *local_evm.raindex.address();
        let raindex_b_instance = Raindex::deploy(local_evm.provider.clone())
            .await
            .expect("Should deploy second raindex");
        let raindex_b = *raindex_b_instance.address();

        MultiRaindexTestSetup {
            token1: *token1.address(),
            token2: *token2.address(),
            token1_sg: SgErc20 {
                id: SgBytes(token1.address().to_string()),
                address: SgBytes(token1.address().to_string()),
                name: Some("Token1".to_string()),
                symbol: Some("Token1".to_string()),
                decimals: Some(SgBigInt(18.to_string())),
            },
            token2_sg: SgErc20 {
                id: SgBytes(token2.address().to_string()),
                address: SgBytes(token2.address().to_string()),
                name: Some("Token2".to_string()),
                symbol: Some("Token2".to_string()),
                decimals: Some(SgBigInt(18.to_string())),
            },
            local_evm,
            owner,
            raindex_a,
            raindex_b,
        }
    }

    pub async fn deposit_to_raindex(
        setup: &MultiRaindexTestSetup,
        raindex: Address,
        token: Address,
        amount: U256,
        vault_id: B256,
    ) {
        use raindex_test_fixtures::Raindex;

        let token_contract = setup
            .local_evm
            .tokens
            .iter()
            .find(|t| *t.address() == token)
            .expect("Token should exist");

        token_contract
            .approve(raindex, amount)
            .from(setup.owner)
            .send()
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap();

        let raindex_instance = Raindex::new(raindex, setup.local_evm.provider.clone());
        let raw_amount = Float::from_fixed_decimal(amount, 18).unwrap().get_inner();

        raindex_instance
            .deposit5(token, vault_id, raw_amount, vec![])
            .from(setup.owner)
            .send()
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap();
    }

    pub fn create_vault(vault_id: B256, setup: &TestSetup, token: &SgErc20) -> SgVault {
        create_vault_with_balance_and_raindex(
            vault_id,
            &setup.local_evm.anvil.addresses()[0],
            setup.raindex,
            token,
            "6",
        )
    }

    pub fn create_vault_for_raindex(
        vault_id: B256,
        setup: &MultiRaindexTestSetup,
        raindex: Address,
        token: &SgErc20,
    ) -> SgVault {
        create_vault_with_balance_and_raindex(
            vault_id,
            &setup.local_evm.anvil.addresses()[0],
            raindex,
            token,
            "1000",
        )
    }

    pub fn create_vault_with_balance_and_raindex(
        vault_id: B256,
        owner: &Address,
        raindex: Address,
        token: &SgErc20,
        balance: &str,
    ) -> SgVault {
        SgVault {
            id: SgBytes(vault_id.to_string()),
            token: token.clone(),
            balance: SgBytes(Float::parse(balance.to_string()).unwrap().as_hex()),
            vault_id: SgBytes(vault_id.to_string()),
            owner: SgBytes(owner.to_string()),
            raindex: SgRaindex {
                id: SgBytes(raindex.to_string()),
            },
            orders_as_input: vec![],
            orders_as_output: vec![],
            balance_changes: vec![],
        }
    }
}

pub mod orders {
    use alloy::primitives::{Address, U256};
    use raindex_bindings::IRaindexV7::{EvaluableV4, OrderV4, IOV2};

    pub fn make_basic_order(input_token: Address, output_token: Address) -> OrderV4 {
        OrderV4 {
            owner: Address::from([1u8; 20]),
            nonce: U256::from(1).into(),
            evaluable: EvaluableV4 {
                interpreter: Address::from([2u8; 20]),
                store: Address::from([3u8; 20]),
                bytecode: alloy::primitives::Bytes::from(vec![0x01, 0x02]),
            },
            validInputs: vec![IOV2 {
                token: input_token,
                vaultId: U256::from(100).into(),
            }],
            validOutputs: vec![IOV2 {
                token: output_token,
                vaultId: U256::from(200).into(),
            }],
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub mod deploy {
        use crate::add_order::AddOrderArgs;
        use crate::dotrain_order::DotrainOrder;
        use alloy::hex::encode_prefixed;
        use alloy::primitives::{Address, B256};
        use alloy::sol_types::{SolCall, SolValue};

        use super::super::local_evm::TestSetup;

        pub async fn deploy_order(setup: &TestSetup, dotrain: String) -> (String, B256) {
            let dotrain_order = DotrainOrder::create(dotrain.clone(), None).await.unwrap();
            let deployment = dotrain_order
                .dotrain_yaml()
                .get_deployment("test-deployment")
                .unwrap();
            let calldata = AddOrderArgs::new_from_deployment(dotrain, deployment, None)
                .await
                .unwrap()
                .try_into_call(vec![setup.local_evm.url()])
                .await
                .unwrap()
                .abi_encode();

            let (event, _) = setup.local_evm.add_order(&calldata, setup.owner).await;
            let order_bytes = encode_prefixed(event.order.abi_encode());
            let order_hash = B256::from(event.orderHash);
            (order_bytes, order_hash)
        }

        use super::super::local_evm::MultiRaindexTestSetup;
        use raindex_bindings::IRaindexV7::OrderV4;

        pub async fn deploy_order_to_raindex(
            setup: &MultiRaindexTestSetup,
            raindex: Address,
            dotrain: String,
        ) -> (String, B256, OrderV4) {
            use alloy::network::TransactionBuilder;
            use alloy::rpc::types::TransactionRequest;
            use alloy::serde::WithOtherFields;
            use raindex_test_fixtures::Raindex;

            let dotrain_order = DotrainOrder::create(dotrain.clone(), None).await.unwrap();
            let deployment = dotrain_order
                .dotrain_yaml()
                .get_deployment("test-deployment")
                .unwrap();
            let calldata = AddOrderArgs::new_from_deployment(dotrain, deployment, None)
                .await
                .unwrap()
                .try_into_call(vec![setup.local_evm.url()])
                .await
                .unwrap()
                .abi_encode();

            let tx_req = WithOtherFields::new(
                TransactionRequest::default()
                    .with_input(calldata.to_vec())
                    .with_to(raindex)
                    .with_from(setup.owner),
            );

            let tx = setup
                .local_evm
                .send_transaction(tx_req)
                .await
                .expect("Should add order");

            let log = tx
                .inner
                .inner
                .logs()
                .iter()
                .find_map(|v| v.log_decode::<Raindex::AddOrderV3>().ok())
                .expect("Should have AddOrderV3 event")
                .inner
                .data;

            let order_bytes = encode_prefixed(log.order.abi_encode());
            let order_hash = B256::from(log.orderHash);
            let order_v4 =
                OrderV4::abi_decode(&log.order.abi_encode()).expect("Should decode OrderV4");

            (order_bytes, order_hash, order_v4)
        }
    }
}

pub mod candidates {
    use crate::take_orders::TakeOrderCandidate;
    use alloy::primitives::Address;
    use rain_math_float::Float;

    use super::orders::make_basic_order;

    pub fn make_candidate(raindex: Address, max_output: Float, ratio: Float) -> TakeOrderCandidate {
        TakeOrderCandidate {
            raindex,
            order: make_basic_order(Address::from([4u8; 20]), Address::from([5u8; 20])),
            input_io_index: 0,
            output_io_index: 0,
            max_output,
            ratio,
            signed_context: vec![],
        }
    }

    pub fn make_simulation_candidate(max_output: Float, ratio: Float) -> TakeOrderCandidate {
        make_candidate(Address::from([0xAAu8; 20]), max_output, ratio)
    }
}

pub mod quotes {
    use crate::raindex_client::order_quotes::{RaindexOrderQuote, RaindexOrderQuoteValue};
    use rain_math_float::Float;
    use raindex_quote::Pair;

    pub fn make_quote_value(
        max_output: Float,
        max_input: Float,
        ratio: Float,
    ) -> RaindexOrderQuoteValue {
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

    pub fn make_quote(
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
}

#[cfg(not(target_family = "wasm"))]
pub mod dotrain {
    use alloy::primitives::Address;
    use raindex_app_settings::spec_version::SpecVersion;

    use super::local_evm::{MultiRaindexTestSetup, TestSetup};

    pub struct DotrainBuilder {
        rpc_url: String,
        rainlang: Address,
        raindex: Address,
        token1: Address,
        token2: Address,
        vault_id: String,
        max_output: String,
        ratio: String,
        inputs: Vec<(String, Option<String>)>,
        outputs: Vec<(String, Option<String>)>,
    }

    impl DotrainBuilder {
        pub fn new(setup: &TestSetup) -> Self {
            Self {
                rpc_url: setup.local_evm.url(),
                rainlang: setup.local_evm.rainlang,
                raindex: setup.raindex,
                token1: setup.token1,
                token2: setup.token2,
                vault_id: "0x01".to_string(),
                max_output: "100".to_string(),
                ratio: "2".to_string(),
                inputs: vec![("t1".to_string(), None), ("t2".to_string(), None)],
                outputs: vec![
                    ("t1".to_string(), Some("0x01".to_string())),
                    ("t2".to_string(), Some("0x01".to_string())),
                ],
            }
        }

        pub fn from_multi_raindex_setup(setup: &MultiRaindexTestSetup) -> Self {
            Self {
                rpc_url: setup.local_evm.url(),
                rainlang: setup.local_evm.rainlang,
                raindex: setup.raindex_a,
                token1: setup.token1,
                token2: setup.token2,
                vault_id: "0x01".to_string(),
                max_output: "100".to_string(),
                ratio: "2".to_string(),
                inputs: vec![("t1".to_string(), None)],
                outputs: vec![("t2".to_string(), Some("0x01".to_string()))],
            }
        }

        pub fn with_vault_id(mut self, vault_id: &str) -> Self {
            self.vault_id = vault_id.to_string();
            for (_, vault) in self.outputs.iter_mut() {
                *vault = Some(vault_id.to_string());
            }
            self
        }

        pub fn with_max_output(mut self, max_output: &str) -> Self {
            self.max_output = max_output.to_string();
            self
        }

        pub fn with_ratio(mut self, ratio: &str) -> Self {
            self.ratio = ratio.to_string();
            self
        }

        pub fn with_raindex(mut self, raindex: Address) -> Self {
            self.raindex = raindex;
            self
        }

        pub fn with_single_io(mut self, vault_id: &str) -> Self {
            self.inputs = vec![("t1".to_string(), None)];
            self.outputs = vec![("t2".to_string(), Some(vault_id.to_string()))];
            self
        }

        pub fn with_multi_io(
            mut self,
            inputs: Vec<(String, Option<String>)>,
            outputs: Vec<(String, Option<String>)>,
        ) -> Self {
            self.inputs = inputs;
            self.outputs = outputs;
            self
        }

        pub fn build(self) -> String {
            let inputs_yaml = self
                .inputs
                .iter()
                .map(|(token, vault_id)| {
                    if let Some(vid) = vault_id {
                        format!(
                            "            - token: {}\n              vault-id: {}",
                            token, vid
                        )
                    } else {
                        format!("            - token: {}", token)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            let outputs_yaml = self
                .outputs
                .iter()
                .map(|(token, vault_id)| {
                    if let Some(vid) = vault_id {
                        format!(
                            "            - token: {}\n              vault-id: {}",
                            token, vid
                        )
                    } else {
                        format!("            - token: {}", token)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            format!(
                r#"
version: {spec_version}
networks:
    test-network:
        rpcs:
            - {rpc_url}
        chain-id: 123
        network-id: 123
        currency: ETH
rainlangs:
    test-rainlang:
        network: test-network
        address: {rainlang}
tokens:
    t1:
        network: test-network
        address: {token1}
        decimals: 18
        label: Token1
        symbol: Token1
    t2:
        network: test-network
        address: {token2}
        decimals: 18
        label: Token2
        symbol: Token2
raindex:
    test-raindex:
        address: {raindex}
orders:
    test-order:
        inputs:
{inputs}
        outputs:
{outputs}
scenarios:
    test-scenario:
        rainlang: test-rainlang
        bindings:
            max-amount: 1000
deployments:
    test-deployment:
        scenario: test-scenario
        order: test-order
---
#max-amount !Max output amount
#calculate-io
amount price: {max_output} {ratio};
#handle-add-order
:;
#handle-io
:;
"#,
                rpc_url = self.rpc_url,
                raindex = self.raindex,
                rainlang = self.rainlang,
                token1 = self.token1,
                token2 = self.token2,
                spec_version = SpecVersion::current(),
                max_output = self.max_output,
                ratio = self.ratio,
                inputs = inputs_yaml,
                outputs = outputs_yaml,
            )
        }
    }

    pub fn create_dotrain_config(setup: &TestSetup) -> String {
        DotrainBuilder::new(setup).build()
    }

    pub fn create_dotrain_config_with_vault_id(setup: &TestSetup, vault_id: &str) -> String {
        DotrainBuilder::new(setup).with_vault_id(vault_id).build()
    }

    pub fn create_dotrain_config_with_params(
        setup: &TestSetup,
        max_output: &str,
        ratio: &str,
    ) -> String {
        DotrainBuilder::new(setup)
            .with_max_output(max_output)
            .with_ratio(ratio)
            .build()
    }

    pub fn create_dotrain_config_with_vault_and_ratio(
        setup: &TestSetup,
        vault_id: &str,
        max_output: &str,
        ratio: &str,
    ) -> String {
        DotrainBuilder::new(setup)
            .with_single_io(vault_id)
            .with_max_output(max_output)
            .with_ratio(ratio)
            .build()
    }

    pub fn create_dotrain_config_for_raindex(
        setup: &MultiRaindexTestSetup,
        raindex: Address,
        vault_id: &str,
        max_output: &str,
        ratio: &str,
    ) -> String {
        DotrainBuilder::from_multi_raindex_setup(setup)
            .with_raindex(raindex)
            .with_single_io(vault_id)
            .with_max_output(max_output)
            .with_ratio(ratio)
            .build()
    }
}

#[cfg(not(target_family = "wasm"))]
pub mod subgraph {
    use alloy::primitives::{Address, B256};
    use raindex_app_settings::spec_version::SpecVersion;
    use raindex_subgraph_client::types::common::{SgBigInt, SgBytes, SgOrder, SgRaindex, SgVault};
    use serde_json::json;

    use super::local_evm::{MultiRaindexTestSetup, TestSetup};

    pub fn create_sg_order(
        setup: &TestSetup,
        order_bytes: String,
        order_hash: B256,
        inputs: Vec<SgVault>,
        outputs: Vec<SgVault>,
    ) -> SgOrder {
        SgOrder {
            id: SgBytes(order_hash.to_string()),
            raindex: SgRaindex {
                id: SgBytes(setup.raindex.to_string()),
            },
            order_bytes: SgBytes(order_bytes),
            order_hash: SgBytes(order_hash.to_string()),
            owner: SgBytes(setup.local_evm.anvil.addresses()[0].to_string()),
            outputs,
            inputs,
            active: true,
            add_events: vec![],
            meta: None,
            timestamp_added: SgBigInt(0.to_string()),
            trades: vec![],
            remove_events: vec![],
        }
    }

    fn vaults_to_json(vaults: &[SgVault], raindex: Address) -> Vec<serde_json::Value> {
        vaults
            .iter()
            .map(|v| {
                json!({
                    "id": v.id.0,
                    "owner": v.owner.0,
                    "vaultId": v.vault_id.0,
                    "balance": v.balance.0,
                    "token": {
                        "id": v.token.id.0,
                        "address": v.token.address.0,
                        "name": v.token.name.clone().unwrap_or_default(),
                        "symbol": v.token.symbol.clone().unwrap_or_default(),
                        "decimals": v.token.decimals.clone().map(|d| d.0).unwrap_or_default()
                    },
                    "raindex": { "id": raindex.to_string() },
                    "ordersAsOutput": [],
                    "ordersAsInput": [],
                    "balanceChanges": []
                })
            })
            .collect()
    }

    pub fn create_sg_order_json(
        setup: &TestSetup,
        order_bytes: &str,
        order_hash: B256,
        inputs: Vec<SgVault>,
        outputs: Vec<SgVault>,
    ) -> serde_json::Value {
        create_sg_order_json_with_raindex_and_owner(
            setup.raindex,
            setup.owner,
            order_bytes,
            order_hash,
            inputs,
            outputs,
        )
    }

    pub fn create_sg_order_json_with_raindex(
        setup: &MultiRaindexTestSetup,
        raindex: Address,
        order_bytes: &str,
        order_hash: B256,
        inputs: Vec<SgVault>,
        outputs: Vec<SgVault>,
    ) -> serde_json::Value {
        create_sg_order_json_with_raindex_and_owner(
            raindex,
            setup.owner,
            order_bytes,
            order_hash,
            inputs,
            outputs,
        )
    }

    fn create_sg_order_json_with_raindex_and_owner(
        raindex: Address,
        owner: Address,
        order_bytes: &str,
        order_hash: B256,
        inputs: Vec<SgVault>,
        outputs: Vec<SgVault>,
    ) -> serde_json::Value {
        let inputs_json = vaults_to_json(&inputs, raindex);
        let outputs_json = vaults_to_json(&outputs, raindex);

        json!({
            "id": order_hash.to_string(),
            "orderBytes": order_bytes,
            "orderHash": order_hash.to_string(),
            "owner": owner.to_string(),
            "outputs": outputs_json,
            "inputs": inputs_json,
            "raindex": { "id": raindex.to_string() },
            "active": true,
            "timestampAdded": "1739448802",
            "meta": null,
            "addEvents": [{
                "transaction": {
                    "id": "0x0000000000000000000000000000000000000000000000000000000000000001",
                    "from": owner.to_string(),
                    "blockNumber": "1",
                    "timestamp": "1739448802"
                }
            }],
            "trades": [],
            "removeEvents": []
        })
    }

    pub fn get_minimal_yaml_for_chain(
        chain_id: u32,
        rpc_url: &str,
        sg_url: &str,
        raindex_address: &str,
    ) -> String {
        format!(
            r#"
version: {spec_version}
networks:
    test-network:
        rpcs:
            - {rpc_url}
        chain-id: {chain_id}
        network-id: {chain_id}
        currency: ETH
subgraphs:
    test-sg: {sg_url}
metaboards:
    test-mb: http://localhost:0/notused
raindexes:
    test-raindex:
        address: {raindex_address}
        network: test-network
        subgraph: test-sg
        local-db-remote: remote
        deployment-block: 0
rainlangs:
    test-rainlang:
        network: test-network
        address: 0x1111111111111111111111111111111111111111
tokens:
    test-token:
        network: test-network
        address: 0x2222222222222222222222222222222222222222
        decimals: 18
        label: TestToken
        symbol: TST
"#,
            spec_version = SpecVersion::current(),
            chain_id = chain_id,
            rpc_url = rpc_url,
            sg_url = sg_url,
            raindex_address = raindex_address,
        )
    }

    pub fn get_multi_raindex_yaml(
        chain_id: u32,
        rpc_url: &str,
        sg_url: &str,
        raindex_a: &str,
        raindex_b: &str,
    ) -> String {
        format!(
            r#"
version: {spec_version}
networks:
    test-network:
        rpcs:
            - {rpc_url}
        chain-id: {chain_id}
        network-id: {chain_id}
        currency: ETH
subgraphs:
    test-sg: {sg_url}
metaboards:
    test-mb: http://localhost:0/notused
raindexes:
    raindex-a:
        address: {raindex_a}
        network: test-network
        subgraph: test-sg
        local-db-remote: remote
        deployment-block: 0
    raindex-b:
        address: {raindex_b}
        network: test-network
        subgraph: test-sg
        local-db-remote: remote
        deployment-block: 0
rainlangs:
    test-rainlang:
        network: test-network
        address: 0x1111111111111111111111111111111111111111
tokens:
    test-token:
        network: test-network
        address: 0x2222222222222222222222222222222222222222
        decimals: 18
        label: TestToken
        symbol: TST
"#,
            spec_version = SpecVersion::current(),
            chain_id = chain_id,
            rpc_url = rpc_url,
            sg_url = sg_url,
            raindex_a = raindex_a,
            raindex_b = raindex_b,
        )
    }
}
