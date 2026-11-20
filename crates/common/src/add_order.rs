use crate::transaction::read_call;
#[cfg(not(target_family = "wasm"))]
use crate::write_tx::{execute_write_tx, WriteTransactionStatus};
use crate::{
    dotrain_order::DotrainOrderError,
    rainlang::compose_to_rainlang,
    transaction::{TransactionArgs, TransactionArgsError},
};
use alloy::primitives::{hex::FromHexError, Address, Bytes, B256};
#[cfg(not(target_family = "wasm"))]
use alloy::primitives::{FixedBytes, U256};
use alloy::rpc::types::TransactionRequest;
use alloy::sol_types::SolCall;
use dotrain::error::ComposeError;
use rain_interpreter_bindings::IParserV2::parse2Return;
use rain_interpreter_bindings::Rainlang::{
    expressionDeployerAddressCall, interpreterAddressCall, parserAddressCall, storeAddressCall,
};
use rain_interpreter_dispair::DISPaiR;
#[cfg(not(target_family = "wasm"))]
use rain_interpreter_eval::{
    error::ForkCallError,
    fork::{Forker, NewForkedEvm},
};
use rain_interpreter_parser::{Parser2, ParserError, ParserV2};
use rain_metadata::{
    types::dotrain::order_builder_state_v1::OrderBuilderStateV1,
    types::raindex_signed_context_oracle::RaindexSignedContextOracleV1, ContentEncoding,
    ContentLanguage, ContentType, Error as RainMetaError, KnownMagic, RainMetaDocumentV1Item,
};
use rain_metadata_bindings::MetaBoard::emitMetaCall;
use raindex_app_settings::deployment::DeploymentCfg;
use raindex_bindings::provider::{mk_read_provider, ReadProviderError};
use raindex_bindings::IRaindexV7::{addOrder5Call, EvaluableV4, OrderConfigV4, TaskV3, IOV2};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::HashMap;
use thiserror::Error;
use url::Url;

pub static RAINDEX_ORDER_ENTRYPOINTS: [&str; 2] = ["calculate-io", "handle-io"];
pub static RAINDEX_ADDORDER_POST_TASK_ENTRYPOINTS: [&str; 1] = ["handle-add-order"];

#[derive(Error, Debug)]
pub enum AddOrderArgsError {
    #[error("Empty Front Matter")]
    EmptyFrontmatter,
    #[error(transparent)]
    ReadProviderError(#[from] ReadProviderError),
    #[error(transparent)]
    ParserError(#[from] ParserError),
    #[error(transparent)]
    FromHexError(#[from] FromHexError),
    #[error(transparent)]
    TransactionArgs(#[from] TransactionArgsError),
    #[error(transparent)]
    RainMetaError(#[from] RainMetaError),
    #[error(transparent)]
    ComposeError(#[from] ComposeError),
    #[error(transparent)]
    DotrainOrderError(Box<DotrainOrderError>),
    #[cfg(not(target_family = "wasm"))]
    #[error(transparent)]
    ForkCallError(Box<ForkCallError>),
    #[cfg(not(target_family = "wasm"))]
    #[error(transparent)]
    WriteTransactionError(#[from] crate::write_tx::WriteTransactionError),
    #[error("Input token not found for index: {0}")]
    InputTokenNotFound(String),
    #[error("Output token not found for index: {0}")]
    OutputTokenNotFound(String),
    #[error("Invalid input args: {0}")]
    InvalidArgs(String),
}

impl From<DotrainOrderError> for AddOrderArgsError {
    fn from(err: DotrainOrderError) -> Self {
        Self::DotrainOrderError(Box::new(err))
    }
}

#[cfg(not(target_family = "wasm"))]
impl From<ForkCallError> for AddOrderArgsError {
    fn from(err: ForkCallError) -> Self {
        Self::ForkCallError(Box::new(err))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename = "kebab-case")]
pub struct AddOrderArgs {
    pub dotrain: String,
    pub inputs: Vec<IOV2>,
    pub outputs: Vec<IOV2>,
    pub rainlang: Address,
    pub bindings: HashMap<String, String>,
    pub additional_meta: Option<Vec<RainMetaDocumentV1Item>>,
}

impl AddOrderArgs {
    /// create a new  instance from Deployment
    pub async fn new_from_deployment(
        dotrain: String,
        deployment: DeploymentCfg,
        additional_meta: Option<Vec<RainMetaDocumentV1Item>>,
    ) -> Result<AddOrderArgs, AddOrderArgsError> {
        let random_vault_id = B256::random();

        let mut inputs = vec![];
        for (i, input) in deployment.order.inputs.iter().enumerate() {
            let input_token = input
                .token
                .as_ref()
                .ok_or_else(|| AddOrderArgsError::InputTokenNotFound(i.to_string()))?;

            inputs.push(IOV2 {
                token: input_token.address,
                vaultId: input.vault_id.map(B256::from).unwrap_or(random_vault_id),
            });
        }

        let mut outputs = vec![];
        for (i, output) in deployment.order.outputs.iter().enumerate() {
            let output_token = output
                .token
                .as_ref()
                .ok_or_else(|| AddOrderArgsError::OutputTokenNotFound(i.to_string()))?;

            outputs.push(IOV2 {
                token: output_token.address,
                vaultId: output.vault_id.map(B256::from).unwrap_or(random_vault_id),
            });
        }

        // If the order has an oracle URL and one isn't already present in
        // additional_meta, add a RaindexSignedContextOracleV1 meta item.
        let additional_meta = {
            let mut meta = additional_meta.unwrap_or_default();
            if let Some(ref oracle_url) = deployment.order.oracle_url {
                let already_has_oracle = meta.iter().any(|item| {
                    RaindexSignedContextOracleV1::find_in_items(std::slice::from_ref(item))
                        .ok()
                        .flatten()
                        .is_some()
                });
                if !already_has_oracle {
                    let oracle = RaindexSignedContextOracleV1::parse(oracle_url)
                        .map_err(AddOrderArgsError::RainMetaError)?;
                    meta.push(oracle.to_meta_item());
                }
            }
            if meta.is_empty() {
                None
            } else {
                Some(meta)
            }
        };

        Ok(AddOrderArgs {
            dotrain: dotrain.to_string(),
            inputs,
            outputs,
            rainlang: deployment.scenario.rainlang.address,
            bindings: deployment.scenario.bindings.to_owned(),
            additional_meta,
        })
    }

    /// Read DISPaiR addresses from the rainlang contract.
    async fn read_dispair(&self, rpcs: &[String]) -> Result<DISPaiR, AddOrderArgsError> {
        let deployer: Address =
            read_call(rpcs, self.rainlang, expressionDeployerAddressCall {}).await?;
        let interpreter: Address =
            read_call(rpcs, self.rainlang, interpreterAddressCall {}).await?;
        let store: Address = read_call(rpcs, self.rainlang, storeAddressCall {}).await?;
        let parser: Address = read_call(rpcs, self.rainlang, parserAddressCall {}).await?;
        Ok(DISPaiR::new(deployer, interpreter, store, parser))
    }

    /// Call parser to parse rainlang into bytecode and constants.
    async fn try_parse_rainlang(
        &self,
        rpcs: Vec<String>,
        rainlang: String,
    ) -> Result<Vec<u8>, AddOrderArgsError> {
        let dispair = self.read_dispair(&rpcs).await?;

        let urls = rpcs
            .iter()
            .map(|rpc| rpc.parse::<Url>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(TransactionArgsError::Url)?;
        let provider = mk_read_provider(&urls)?;
        let parser: ParserV2 = dispair.into();
        let rainlang_parsed: parse2Return = parser
            .parse_text(rainlang.as_str(), &provider)
            .await
            .map_err(AddOrderArgsError::ParserError)?;

        Ok(rainlang_parsed.bytecode.into())
    }

    /// Generate RainlangSource meta
    fn try_generate_meta(&self, rainlang: String) -> Result<Vec<u8>, AddOrderArgsError> {
        let mut meta_docs = Vec::new();

        let rainlang_meta_doc = RainMetaDocumentV1Item {
            payload: ByteBuf::from(rainlang.as_bytes()),
            magic: KnownMagic::RainlangSourceV1,
            content_type: ContentType::OctetStream,
            content_encoding: ContentEncoding::None,
            content_language: ContentLanguage::None,
        };
        meta_docs.push(rainlang_meta_doc);

        if let Some(existing_meta) = &self.additional_meta {
            meta_docs.extend(existing_meta.iter().filter_map(|i| match i.magic {
                KnownMagic::RainlangSourceV1 | KnownMagic::DotrainSourceV1 => None,
                _ => Some(i.clone()),
            }));
        }

        let meta_doc_bytes =
            RainMetaDocumentV1Item::cbor_encode_seq(&meta_docs, KnownMagic::RainMetaDocumentV1)
                .map_err(AddOrderArgsError::RainMetaError)?;

        Ok(meta_doc_bytes)
    }

    /// Compose to rainlang string
    pub fn compose_to_rainlang(&self) -> Result<String, AddOrderArgsError> {
        let res = compose_to_rainlang(
            self.dotrain.clone(),
            self.bindings.clone(),
            &RAINDEX_ORDER_ENTRYPOINTS,
        )?;
        Ok(res)
    }

    /// Compose the addOrder2 post action
    pub fn compose_addorder_post_task(&self) -> Result<String, AddOrderArgsError> {
        let res = compose_to_rainlang(
            self.dotrain.clone(),
            self.bindings.clone(),
            &RAINDEX_ADDORDER_POST_TASK_ENTRYPOINTS,
        )?;
        Ok(res)
    }

    /// Generate an addOrder call from given dotrain
    pub async fn try_into_call(
        &self,
        rpcs: Vec<String>,
    ) -> Result<addOrder5Call, AddOrderArgsError> {
        let rainlang = self.compose_to_rainlang()?;
        let bytecode = self
            .try_parse_rainlang(rpcs.clone(), rainlang.clone())
            .await?;

        let meta = self.try_generate_meta(rainlang)?;

        let dispair = self.read_dispair(&rpcs).await?;

        // get the evaluable for the post action
        let post_rainlang = self.compose_addorder_post_task()?;
        let post_bytecode = self
            .try_parse_rainlang(rpcs.clone(), post_rainlang.clone())
            .await?;

        let post_evaluable = EvaluableV4 {
            interpreter: dispair.interpreter,
            store: dispair.store,
            bytecode: post_bytecode.into(),
        };

        let post_task = TaskV3 {
            evaluable: post_evaluable,
            signedContext: vec![],
        };

        Ok(addOrder5Call {
            config: OrderConfigV4 {
                validInputs: self.inputs.clone(),
                validOutputs: self.outputs.clone(),
                evaluable: EvaluableV4 {
                    interpreter: dispair.interpreter,
                    store: dispair.store,
                    bytecode: bytecode.into(),
                },
                meta: meta.into(),
                nonce: B256::random(),
                secret: B256::random(),
            },
            tasks: vec![post_task],
        })
    }

    pub fn try_into_emit_meta_call(&self) -> Result<Option<emitMetaCall>, AddOrderArgsError> {
        match self.additional_meta.as_ref() {
            Some(meta_docs) => {
                match meta_docs
                    .iter()
                    .find(|document| document.magic == KnownMagic::OrderBuilderStateV1)
                {
                    Some(doc) => {
                        let builder_state = OrderBuilderStateV1::try_from(doc.clone())?;
                        let subject_hash = builder_state.dotrain_hash();
                        let meta_document = RainMetaDocumentV1Item {
                            payload: ByteBuf::from(self.dotrain.as_bytes()),
                            magic: KnownMagic::DotrainSourceV1,
                            content_type: ContentType::OctetStream,
                            content_encoding: ContentEncoding::None,
                            content_language: ContentLanguage::None,
                        };
                        let meta = RainMetaDocumentV1Item::cbor_encode_seq(
                            &vec![meta_document.clone()],
                            KnownMagic::RainMetaDocumentV1,
                        )?;
                        Ok(Some(emitMetaCall {
                            subject: subject_hash,
                            meta: Bytes::copy_from_slice(&meta),
                        }))
                    }
                    None => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    pub async fn get_add_order_call_parameters(
        &self,
        transaction_args: TransactionArgs,
    ) -> Result<TransactionRequest, AddOrderArgsError> {
        let add_order_call = self.try_into_call(transaction_args.clone().rpcs).await?;
        let tx_request = transaction_args
            .try_into_transaction_request(add_order_call, transaction_args.raindex_address)?;
        Ok(tx_request)
    }

    #[cfg(not(target_family = "wasm"))]
    pub async fn execute<S: Fn(WriteTransactionStatus)>(
        &self,
        transaction_args: TransactionArgs,
        transaction_status_changed: S,
    ) -> Result<(), AddOrderArgsError> {
        let (ledger_client, _) = transaction_args.clone().try_into_ledger_client().await?;

        let tx_request = self.get_add_order_call_parameters(transaction_args).await?;

        execute_write_tx(ledger_client, tx_request, 4, transaction_status_changed).await?;

        Ok(())
    }

    pub async fn get_add_order_calldata(
        &self,
        transaction_args: TransactionArgs,
    ) -> Result<Vec<u8>, AddOrderArgsError> {
        Ok(self
            .try_into_call(transaction_args.clone().rpcs)
            .await?
            .abi_encode())
    }

    #[cfg(not(target_family = "wasm"))]
    pub async fn simulate_execute(
        &self,
        transaction_args: TransactionArgs,
        from: Option<Address>,
    ) -> Result<(), AddOrderArgsError> {
        let from_address = if let Some(v) = from {
            v.0 .0
        } else {
            let (_, Address(FixedBytes(address))) =
                transaction_args.clone().try_into_ledger_client().await?;
            address
        };

        let mut err: Option<AddOrderArgsError> = None;

        if transaction_args.rpcs.is_empty() {
            return Err(AddOrderArgsError::InvalidArgs(
                "rpcs cannot be empty".into(),
            ));
        }
        for rpc in transaction_args.rpcs.clone() {
            match Forker::new_with_fork(
                NewForkedEvm {
                    fork_url: rpc.clone(),
                    fork_block_number: None,
                },
                None,
                None,
            )
            .await
            {
                Ok(mut forker) => {
                    let call = match self.try_into_call(vec![rpc.clone()]).await {
                        Ok(c) => c,
                        Err(e) => {
                            err = Some(e);
                            continue;
                        }
                    };
                    match forker
                        .alloy_call_committing(
                            Address::from(from_address),
                            transaction_args.raindex_address,
                            call,
                            U256::ZERO,
                            true,
                        )
                        .await
                    {
                        Ok(_) => return Ok(()),
                        Err(e) => {
                            err = Some(AddOrderArgsError::ForkCallError(Box::new(e)));
                            continue;
                        }
                    }
                }
                Err(e) => {
                    err = Some(AddOrderArgsError::ForkCallError(Box::new(e)));
                }
            }
        }
        if let Some(err) = err {
            return Err(err);
        }

        Ok(())
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use crate::dotrain_order::DotrainOrder;
    use alloy::primitives::Bytes;
    use rain_metadata::{
        types::dotrain::source_v1::DotrainSourceV1, Error as RainMetaError, KnownMagic,
    };
    use raindex_app_settings::{
        network::NetworkCfg,
        order::{OrderCfg, OrderIOCfg},
        rainlang::RainlangCfg,
        scenario::ScenarioCfg,
        spec_version::SpecVersion,
        token::TokenCfg,
        yaml::{default_document, default_documents},
    };
    use raindex_test_fixtures::LocalEvm;
    use std::{
        collections::BTreeMap,
        str::FromStr,
        sync::{Arc, RwLock},
    };
    use strict_yaml_rust::StrictYaml;
    use url::Url;

    #[test]
    fn test_try_generate_meta() {
        let dotrain_body = String::from(
            "
#calculate-io
max-amount: 100e18,
price: 2e18;

#handle-io
max-amount: 100e18,
price: 2e18;
",
        );
        let args = AddOrderArgs {
            dotrain: "".into(),
            inputs: vec![],
            outputs: vec![],
            bindings: HashMap::new(),
            rainlang: Address::default(),
            additional_meta: None,
        };

        let meta_bytes = args.try_generate_meta(dotrain_body).unwrap();
        assert_eq!(
            meta_bytes,
            vec![
                255, 10, 137, 198, 116, 238, 120, 116, 163, 0, 88, 93, 10, 35, 99, 97, 108, 99,
                117, 108, 97, 116, 101, 45, 105, 111, 10, 109, 97, 120, 45, 97, 109, 111, 117, 110,
                116, 58, 32, 49, 48, 48, 101, 49, 56, 44, 10, 112, 114, 105, 99, 101, 58, 32, 50,
                101, 49, 56, 59, 10, 10, 35, 104, 97, 110, 100, 108, 101, 45, 105, 111, 10, 109,
                97, 120, 45, 97, 109, 111, 117, 110, 116, 58, 32, 49, 48, 48, 101, 49, 56, 44, 10,
                112, 114, 105, 99, 101, 58, 32, 50, 101, 49, 56, 59, 10, 1, 27, 255, 19, 16, 158,
                65, 51, 111, 242, 2, 120, 24, 97, 112, 112, 108, 105, 99, 97, 116, 105, 111, 110,
                47, 111, 99, 116, 101, 116, 45, 115, 116, 114, 101, 97, 109
            ]
        );
    }

    #[test]
    fn test_try_generate_meta_with_additional_meta_filters_reserved() {
        let dotrain_body = "/* test */".to_string();
        let dotrain_hash = DotrainSourceV1(dotrain_body.clone()).hash();
        let builder_state = OrderBuilderStateV1 {
            dotrain_hash,
            field_values: BTreeMap::new(),
            deposits: BTreeMap::new(),
            select_tokens: BTreeMap::new(),
            vault_ids: BTreeMap::new(),
            selected_deployment: "dep".to_string(),
        };
        let additional_meta = vec![
            // Should be filtered out
            RainMetaDocumentV1Item {
                payload: ByteBuf::from("ignored-rainlang".as_bytes()),
                magic: KnownMagic::RainlangSourceV1,
                content_type: ContentType::OctetStream,
                content_encoding: ContentEncoding::None,
                content_language: ContentLanguage::None,
            },
            // Should be filtered out
            RainMetaDocumentV1Item::from(DotrainSourceV1("ignored-dotrain".to_string())),
            // Should be retained
            RainMetaDocumentV1Item::try_from(builder_state.clone()).unwrap(),
        ];

        let args = AddOrderArgs {
            dotrain: dotrain_body.clone(),
            inputs: vec![],
            outputs: vec![],
            bindings: HashMap::new(),
            rainlang: Address::default(),
            additional_meta: Some(additional_meta),
        };

        let meta_bytes = args.try_generate_meta("rainlang-body".to_string()).unwrap();
        let decoded = RainMetaDocumentV1Item::cbor_decode(&meta_bytes).unwrap();

        // Rainlang meta is always present and should match the composed rainlang payload
        assert_eq!(decoded[0].magic, KnownMagic::RainlangSourceV1);
        assert_eq!(decoded[0].payload.as_ref(), "rainlang-body".as_bytes());
        // Only the builder state from additional meta should remain (reserved magics filtered)
        assert_eq!(decoded.len(), 2);
        let builder_meta = decoded
            .iter()
            .find(|item| item.magic == KnownMagic::OrderBuilderStateV1)
            .expect("builder state meta not found");
        assert_eq!(
            OrderBuilderStateV1::try_from(builder_meta.clone()).unwrap(),
            builder_state
        );
    }

    #[test]
    fn test_try_into_emit_meta_call_with_builder_state() {
        let dotrain_body = "/* dotrain template */".to_string();
        let dotrain_source = DotrainSourceV1(dotrain_body.clone());
        let builder_state = OrderBuilderStateV1 {
            dotrain_hash: dotrain_source.hash(),
            field_values: BTreeMap::new(),
            deposits: BTreeMap::new(),
            select_tokens: BTreeMap::new(),
            vault_ids: BTreeMap::new(),
            selected_deployment: "dep".to_string(),
        };
        let args = AddOrderArgs {
            dotrain: dotrain_body.clone(),
            inputs: vec![],
            outputs: vec![],
            bindings: HashMap::new(),
            rainlang: Address::default(),
            additional_meta: Some(vec![
                RainMetaDocumentV1Item::try_from(builder_state).unwrap()
            ]),
        };

        let emit_call = args
            .try_into_emit_meta_call()
            .unwrap()
            .expect("emitMetaCall missing");
        let decoded_meta = RainMetaDocumentV1Item::cbor_decode(emit_call.meta.as_ref()).unwrap();

        assert_eq!(emit_call.subject, dotrain_source.hash());
        assert_eq!(decoded_meta.len(), 1);
        let dotrain_meta = DotrainSourceV1::try_from(decoded_meta[0].clone()).unwrap();
        assert_eq!(dotrain_meta.0, dotrain_body);
    }

    #[test]
    fn test_try_into_emit_meta_call_invalid_builder_state_payload() {
        let invalid_builder_state = RainMetaDocumentV1Item {
            payload: ByteBuf::from(vec![1, 2, 3]),
            magic: KnownMagic::OrderBuilderStateV1,
            content_type: ContentType::OctetStream,
            content_encoding: ContentEncoding::None,
            content_language: ContentLanguage::None,
        };
        let args = AddOrderArgs {
            dotrain: "body".to_string(),
            inputs: vec![],
            outputs: vec![],
            bindings: HashMap::new(),
            rainlang: Address::default(),
            additional_meta: Some(vec![invalid_builder_state]),
        };

        let err = args.try_into_emit_meta_call().unwrap_err();
        assert!(matches!(
            err,
            AddOrderArgsError::RainMetaError(RainMetaError::SerdeCborError(_))
        ));
    }

    #[test]
    fn test_try_generate_meta_empty_dotrain() {
        let args = AddOrderArgs {
            dotrain: "".into(),
            inputs: vec![],
            outputs: vec![],
            bindings: HashMap::new(),
            rainlang: Address::default(),
            additional_meta: None,
        };
        let meta_bytes = args.try_generate_meta("".to_string()).unwrap();
        assert_eq!(
            meta_bytes,
            vec![
                255, 10, 137, 198, 116, 238, 120, 116, 163, 0, 64, 1, 27, 255, 19, 16, 158, 65, 51,
                111, 242, 2, 120, 24, 97, 112, 112, 108, 105, 99, 97, 116, 105, 111, 110, 47, 111,
                99, 116, 101, 116, 45, 115, 116, 114, 101, 97, 109
            ]
        );
    }

    #[tokio::test]
    async fn test_add_order_random_vault_id_generation() {
        let network = NetworkCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "test-network".to_string(),
            rpcs: vec![Url::parse("https://some-rpc.com").unwrap()],
            chain_id: 137,
            label: None,
            network_id: None,
            currency: None,
        };
        let network_arc = Arc::new(network);
        let rainlang = RainlangCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            network: network_arc.clone(),
            address: Address::default(),
        };
        let rainlang_arc = Arc::new(rainlang);
        let scenario = ScenarioCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            documents: default_documents(),
            key: "test-scenario".to_string(),
            bindings: HashMap::new(),
            runs: None,
            blocks: None,
            rainlang: rainlang_arc.clone(),
        };
        let token1 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token1".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token2 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token2".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token3 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token3".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token1_arc = Arc::new(token1);
        let token2_arc = Arc::new(token2);
        let token3_arc = Arc::new(token3);
        let known_vault_id = U256::from(1);
        let order = OrderCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            inputs: vec![
                OrderIOCfg {
                    token_key: token1_arc.key.clone(),
                    token: Some(token1_arc.clone()),
                    vaultless: false,
                    vault_id: None,
                },
                OrderIOCfg {
                    token_key: token2_arc.key.clone(),
                    token: Some(token2_arc.clone()),
                    vaultless: false,
                    vault_id: Some(known_vault_id),
                },
            ],
            outputs: vec![OrderIOCfg {
                token_key: token3_arc.key.clone(),
                token: Some(token3_arc.clone()),
                vaultless: false,
                vault_id: None,
            }],
            network: network_arc.clone(),
            rainlang: None,
            raindex: None,
            oracle_url: None,
        };
        let deployment = DeploymentCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            scenario: Arc::new(scenario),
            order: Arc::new(order),
        };

        let dotrain = format!(
            "
version: {spec_version}
---
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
_ _: 0 0;
",
            spec_version = SpecVersion::current()
        );
        let result = AddOrderArgs::new_from_deployment(dotrain.to_string(), deployment, None)
            .await
            .unwrap();

        // input1 vault id should be same as known_vault_id
        assert_eq!(
            result.inputs[1].vaultId,
            B256::from(U256::from(known_vault_id))
        );

        // input0 and output0 vaults should be the same random value
        assert_eq!(result.inputs[0].vaultId, result.outputs[0].vaultId);
    }

    #[tokio::test]
    async fn test_into_add_order_call() {
        let local_evm = LocalEvm::new_with_tokens(2).await;
        let network = NetworkCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "test-network".to_string(),
            rpcs: vec![Url::parse(&local_evm.url()).unwrap()],
            chain_id: 137,
            label: None,
            network_id: None,
            currency: None,
        };
        let network_arc = Arc::new(network);
        let rainlang = RainlangCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            network: network_arc.clone(),
            address: local_evm.rainlang,
        };
        let rainlang_arc = Arc::new(rainlang);
        let scenario = ScenarioCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            documents: default_documents(),
            key: "test-scenario".to_string(),
            bindings: HashMap::new(),
            runs: None,
            blocks: None,
            rainlang: rainlang_arc.clone(),
        };
        let token1 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token1".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token2 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token2".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token3 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token3".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token1_arc = Arc::new(token1);
        let token2_arc = Arc::new(token2);
        let token3_arc = Arc::new(token3);
        let order = OrderCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            inputs: vec![
                OrderIOCfg {
                    token_key: token1_arc.key.clone(),
                    token: Some(token1_arc.clone()),
                    vaultless: false,
                    vault_id: Some(U256::from(2)),
                },
                OrderIOCfg {
                    token_key: token2_arc.key.clone(),
                    token: Some(token2_arc.clone()),
                    vaultless: false,
                    vault_id: Some(U256::from(1)),
                },
            ],
            outputs: vec![OrderIOCfg {
                token_key: token3_arc.key.clone(),
                token: Some(token3_arc.clone()),
                vaultless: false,
                vault_id: Some(U256::from(4)),
            }],
            network: network_arc.clone(),
            rainlang: None,
            raindex: None,
            oracle_url: None,
        };
        let deployment = DeploymentCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            scenario: Arc::new(scenario),
            order: Arc::new(order),
        };

        let dotrain = format!(
            "
version: {spec_version}
---
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
_ _: 0 0;
",
            spec_version = SpecVersion::current()
        );

        let result = AddOrderArgs::new_from_deployment(dotrain.to_string(), deployment, None)
            .await
            .unwrap();

        let add_order_call = result.try_into_call(vec![local_evm.url()]).await.unwrap();

        assert_eq!(add_order_call.config.validInputs.len(), 2);
        assert_eq!(add_order_call.config.validOutputs.len(), 1);
        assert_eq!(add_order_call.tasks.len(), 1);

        assert_eq!(
            add_order_call.config.validInputs[0].vaultId,
            B256::from(U256::from(2))
        );
        assert_eq!(
            add_order_call.config.validInputs[1].vaultId,
            B256::from(U256::from(1))
        );
        assert_eq!(
            add_order_call.config.validOutputs[0].vaultId,
            B256::from(U256::from(4))
        );

        assert_eq!(add_order_call.tasks[0].evaluable.bytecode.len(), 111);

        assert_eq!(add_order_call.config.meta.len(), 105);

        assert_eq!(
            add_order_call.config.validInputs[0].token,
            Address::default()
        );

        assert_eq!(
            add_order_call.config.validInputs[1].token,
            Address::default()
        );

        assert_eq!(
            add_order_call.config.validOutputs[0].token,
            Address::default()
        );

        assert_eq!(
            add_order_call.tasks[0].evaluable.interpreter,
            *local_evm.interpreter.address()
        );

        assert_eq!(
            add_order_call.tasks[0].evaluable.store,
            *local_evm.store.address()
        );

        assert_eq!(add_order_call.tasks[0].evaluable.bytecode.len(), 111);
        assert_eq!(add_order_call.tasks[0].signedContext.len(), 0);
    }

    #[tokio::test]
    async fn test_add_order_post_action() {
        let network = NetworkCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "test-network".to_string(),
            rpcs: vec![Url::parse("https://some-rpc.com").unwrap()],
            chain_id: 137,
            label: None,
            network_id: None,
            currency: None,
        };
        let network_arc = Arc::new(network);
        let rainlang = RainlangCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            network: network_arc.clone(),
            address: Address::default(),
        };
        let rainlang_arc = Arc::new(rainlang);
        let scenario = ScenarioCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            documents: default_documents(),
            key: "test-scenario".to_string(),
            bindings: HashMap::new(),
            runs: None,
            blocks: None,
            rainlang: rainlang_arc.clone(),
        };
        let token1 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token1".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token2 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token2".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token3 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token3".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token1_arc = Arc::new(token1);
        let token2_arc = Arc::new(token2);
        let token3_arc = Arc::new(token3);
        let known_vault_id = U256::from(1);
        let order = OrderCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            inputs: vec![
                OrderIOCfg {
                    token_key: token1_arc.key.clone(),
                    token: Some(token1_arc.clone()),
                    vaultless: false,
                    vault_id: None,
                },
                OrderIOCfg {
                    token_key: token2_arc.key.clone(),
                    token: Some(token2_arc.clone()),
                    vaultless: false,
                    vault_id: Some(known_vault_id),
                },
            ],
            outputs: vec![OrderIOCfg {
                token_key: token3_arc.key.clone(),
                token: Some(token3_arc.clone()),
                vaultless: false,
                vault_id: None,
            }],
            network: network_arc.clone(),
            rainlang: None,
            raindex: None,
            oracle_url: None,
        };
        let deployment = DeploymentCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            scenario: Arc::new(scenario),
            order: Arc::new(order),
        };

        let dotrain = format!(
            "
version: {spec_version}
---
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
_ _: 0 0;
",
            spec_version = SpecVersion::current()
        );
        let result =
            AddOrderArgs::new_from_deployment(dotrain.to_string(), deployment.clone(), None)
                .await
                .unwrap();

        let post_action = result.compose_addorder_post_task().unwrap();

        assert_eq!(post_action, "/* 0. handle-add-order */ \n_ _: 0 0;");
    }

    #[tokio::test]
    async fn test_compose_addorder_post_task_empty_dotrain() {
        let local_evm = LocalEvm::new().await;
        let deployment = get_deployment(&local_evm.url(), local_evm.rainlang);
        let result = AddOrderArgs::new_from_deployment("".to_string(), deployment.clone(), None)
            .await
            .unwrap();
        let err = result.compose_addorder_post_task().unwrap_err();
        assert!(matches!(
            err,
            AddOrderArgsError::ComposeError(ComposeError::Reject(_))
        ));
    }

    #[tokio::test]
    async fn test_compose_addorder_post_task_missing_bindings() {
        let local_evm = LocalEvm::new().await;
        let deployment = get_deployment(&local_evm.url(), local_evm.rainlang);
        let result = AddOrderArgs::new_from_deployment(
            format!(
                "
version: {spec_version}
---
#key1 !Test binding
#calculate-io
_ _: 0 0;
#handle-add-order
_ _: 0 key1;
",
                spec_version = SpecVersion::current()
            )
            .to_string(),
            deployment.clone(),
            None,
        )
        .await
        .unwrap();
        let err = result.compose_addorder_post_task().unwrap_err();
        assert!(matches!(
            err,
            AddOrderArgsError::ComposeError(ComposeError::Problems(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_simulate_execute_ok() {
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
        address: {rainlang}
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
_ _: 16 52;
#handle-add-order
:;
#handle-io
:;
"#,
            rpc_url = local_evm.url(),
            raindex = raindex.address(),
            rainlang = local_evm.rainlang,
            token1 = token1.address(),
            token2 = token2.address(),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.clone(), None).await.unwrap();
        let deployment = dotrain_order
            .dotrain_yaml()
            .get_deployment("some-key")
            .unwrap();
        AddOrderArgs::new_from_deployment(dotrain, deployment, None)
            .await
            .unwrap()
            .simulate_execute(
                TransactionArgs {
                    raindex_address: *raindex.address(),
                    rpcs: vec![local_evm.url()],
                    ..Default::default()
                },
                Some(token1_holder),
            )
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_simulate_execute_err() {
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
        address: {rainlang}
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
_ _: 16 52;
#handle-add-order
:;
#handle-io
:;
"#,
            rpc_url = local_evm.url(),
            raindex = raindex.address(),
            rainlang = local_evm.rainlang,
            token1 = token1.address(),
            token2 = token2.address(),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.clone(), None).await.unwrap();
        let deployment = dotrain_order
            .dotrain_yaml()
            .get_deployment("some-key")
            .unwrap();
        AddOrderArgs::new_from_deployment(dotrain, deployment, None)
            .await
            .unwrap()
            .simulate_execute(
                TransactionArgs {
                    // send the tx to random address
                    raindex_address: Address::random(),
                    rpcs: vec![local_evm.url()],
                    ..Default::default()
                },
                Some(token1_holder),
            )
            .await
            .expect_err("expected to fail but resolved");
    }

    fn get_deployment(rpc_url: &str, rainlang_address: Address) -> DeploymentCfg {
        let network = NetworkCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "test-network".to_string(),
            rpcs: vec![Url::parse(rpc_url).unwrap()],
            chain_id: 137,
            label: None,
            network_id: None,
            currency: None,
        };
        let network_arc = Arc::new(network);
        let rainlang = RainlangCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            network: network_arc.clone(),
            address: rainlang_address,
        };
        let rainlang_arc = Arc::new(rainlang);
        let scenario = ScenarioCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            documents: default_documents(),
            key: "test-scenario".to_string(),
            bindings: HashMap::new(),
            runs: None,
            blocks: None,
            rainlang: rainlang_arc.clone(),
        };
        let token1 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token1".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token2 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token2".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token3 = TokenCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            address: Address::default(),
            network: network_arc.clone(),
            decimals: Some(18),
            label: None,
            symbol: Some("Token3".to_string()),
            logo_uri: None,
            extensions: None,
        };
        let token1_arc = Arc::new(token1);
        let token2_arc = Arc::new(token2);
        let token3_arc = Arc::new(token3);
        let order = OrderCfg {
            document: Arc::new(RwLock::new(StrictYaml::String("".to_string()))),
            key: "".to_string(),
            inputs: vec![
                OrderIOCfg {
                    token_key: token1_arc.key.clone(),
                    token: Some(token1_arc.clone()),
                    vaultless: false,
                    vault_id: Some(U256::from(2)),
                },
                OrderIOCfg {
                    token_key: token2_arc.key.clone(),
                    token: Some(token2_arc.clone()),
                    vaultless: false,
                    vault_id: Some(U256::from(1)),
                },
            ],
            outputs: vec![OrderIOCfg {
                token_key: token3_arc.key.clone(),
                token: Some(token3_arc.clone()),
                vaultless: false,
                vault_id: Some(U256::from(4)),
            }],
            network: network_arc.clone(),
            rainlang: None,
            raindex: None,
            oracle_url: None,
        };
        DeploymentCfg {
            document: default_document(),
            key: "".to_string(),
            scenario: Arc::new(scenario),
            order: Arc::new(order),
        }
    }

    #[tokio::test]
    async fn test_try_parse_rainlang() {
        let local_evm = LocalEvm::new_with_tokens(2).await;
        let deployment = get_deployment(&local_evm.url(), local_evm.rainlang);

        let dotrain = format!(
            "
version: {spec_version}
---
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
_ _: 0 0;
",
            spec_version = SpecVersion::current()
        );
        let add_order_args =
            AddOrderArgs::new_from_deployment(dotrain.to_string(), deployment, None)
                .await
                .unwrap();
        let rainlang = add_order_args.compose_to_rainlang().unwrap();
        let res = add_order_args
            .try_parse_rainlang(vec![local_evm.url()], rainlang)
            .await
            .unwrap();
        assert_eq!(
            res,
            vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 21, 2, 0, 0, 0, 12, 2, 2, 0, 2, 1, 16, 0, 0, 1,
                16, 0, 0, 0, 0, 0, 0
            ]
        );
    }

    #[tokio::test]
    async fn test_try_parse_rainlang_invalid_url() {
        let deployment = get_deployment("https://testtest.com", Address::random());
        let dotrain = format!(
            "
version: {spec_version}
---
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
_ _: 0 0;
",
            spec_version = SpecVersion::current()
        );
        let add_order_args =
            AddOrderArgs::new_from_deployment(dotrain.to_string(), deployment, None)
                .await
                .unwrap();
        let rainlang = add_order_args.compose_to_rainlang().unwrap();
        let err = add_order_args
            .try_parse_rainlang(vec!["invalid-url".to_string()], rainlang)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            AddOrderArgsError::TransactionArgs(TransactionArgsError::Url(_))
        ));
    }

    #[tokio::test]
    async fn test_try_parse_rainlang_missing_rpc_data() {
        let rpc_url = "https://testtest.com/".to_string();
        let deployment = get_deployment(&rpc_url, Address::random());
        let dotrain = format!(
            "
version: {spec_version}
---
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
_ _: 0 0;
",
            spec_version = SpecVersion::current()
        );
        let add_order_args =
            AddOrderArgs::new_from_deployment(dotrain.to_string(), deployment, None)
                .await
                .unwrap();
        let rainlang = add_order_args.compose_to_rainlang().unwrap();
        let err = add_order_args
            .try_parse_rainlang(vec![rpc_url.clone()], rainlang)
            .await
            .unwrap_err();
        assert!(
            matches!(
                &err,
                AddOrderArgsError::TransactionArgs(TransactionArgsError::Transport(_))
            ),
            "unexpected error variant: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_try_parse_rainlang_malformed_rainlang() {
        let local_evm = LocalEvm::new_with_tokens(2).await;
        let deployment = get_deployment(&local_evm.url(), local_evm.rainlang);
        let dotrain = format!(
            "
version: {spec_version}
---
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
_ _: 0 0;
",
            spec_version = SpecVersion::current()
        );
        let add_order_args =
            AddOrderArgs::new_from_deployment(dotrain.to_string(), deployment, None)
                .await
                .unwrap();
        let rainlang = add_order_args.compose_to_rainlang().unwrap();
        let err = add_order_args
            .try_parse_rainlang(vec![local_evm.url()], rainlang.as_str()[..10].to_string())
            .await
            .unwrap_err();
        assert!(matches!(err, AddOrderArgsError::ParserError(_)));
    }

    #[tokio::test]
    async fn test_compose_to_rainlang() {
        let local_evm = LocalEvm::new().await;
        let dotrain = format!(
            r#"
networks:
    test:
        rpcs:
            - {rpc_url}
        chain-id: 137
        network-id: 137
        currency: MATIC
rainlangs:
    test:
        address: 0x1234567890123456789012345678901234567890
scenarios:
    test:
        bindings:
            key1: 10
            key2: 20
        rainlang: test
---
#key1 !Test binding
#key2 !Test binding
#calculate-io
_ _: key1 key2;
#handle-io
:;
#handle-add-order
:;"#,
            rpc_url = local_evm.url(),
        );

        let add_order_args = AddOrderArgs {
            dotrain: dotrain.clone(),
            inputs: vec![],
            outputs: vec![],
            rainlang: local_evm.rainlang,
            bindings: HashMap::from([
                ("key1".to_string(), "10".to_string()),
                ("key2".to_string(), "20".to_string()),
            ]),
            additional_meta: None,
        };
        let rainlang = add_order_args.compose_to_rainlang().unwrap();
        assert_eq!(
            rainlang,
            "/* 0. calculate-io */ \n_ _: 10 20;\n\n/* 1. handle-io */ \n:;"
        );
    }

    #[tokio::test]
    async fn test_compose_to_rainlang_invalid_dotrain() {
        let add_order_args = AddOrderArgs {
            dotrain: "invalid-dotrain".to_string(),
            inputs: vec![],
            outputs: vec![],
            rainlang: Address::random(),
            bindings: HashMap::from([
                ("key1".to_string(), "10".to_string()),
                ("key2".to_string(), "20".to_string()),
            ]),
            additional_meta: None,
        };
        let err = add_order_args.compose_to_rainlang().unwrap_err();
        assert!(matches!(
            err,
            AddOrderArgsError::ComposeError(ComposeError::Problems(_))
        ));
    }

    #[tokio::test]
    async fn test_compose_to_rainlang_missing_bindings() {
        let dotrain = r#"
networks:
    test:
        rpcs:
            - https://testtest.com
        chain-id: 137
        network-id: 137
        currency: MATIC
rainlangs:
    test:
        address: 0x1234567890123456789012345678901234567890
scenarios:
    test:
        bindings:
            key1: 10
            key2: 20
        rainlang: test
---
#key1 !Test binding
#key2 !Test binding
#calculate-io
_ _: key1 key2;
#handle-io
:;
#handle-add-order
:;"#;
        let add_order_args = AddOrderArgs {
            dotrain: dotrain.to_string(),
            inputs: vec![],
            outputs: vec![],
            rainlang: Address::random(),
            bindings: HashMap::new(),
            additional_meta: None,
        };
        let err = add_order_args.compose_to_rainlang().unwrap_err();
        assert!(matches!(
            err,
            AddOrderArgsError::ComposeError(ComposeError::Problems(_))
        ));
    }

    #[tokio::test]
    async fn test_get_add_order_call_parameters() {
        let local_evm = LocalEvm::new_with_tokens(2).await;
        let dotrain = format!(
            r#"
networks:
    test:
        rpcs:
            - {rpc_url}
        chain-id: 137
        network-id: 137
        currency: MATIC
rainlangs:
    test:
        address: 0x1234567890123456789012345678901234567890
scenarios:
    test:
        rainlang: test
---
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
:;"#,
            rpc_url = local_evm.url(),
        );
        let add_order_args = AddOrderArgs {
            dotrain: dotrain.to_string(),
            inputs: vec![IOV2 {
                token: *local_evm.tokens[0].address(),
                vaultId: B256::from(U256::from(2)),
            }],
            outputs: vec![IOV2 {
                token: *local_evm.tokens[1].address(),
                vaultId: B256::from(U256::from(4)),
            }],
            rainlang: local_evm.rainlang,
            bindings: HashMap::new(),
            additional_meta: None,
        };

        let add_order_call = addOrder5Call {
            config: OrderConfigV4 {
                evaluable: EvaluableV4 {
                    interpreter: *local_evm.interpreter.address(),
                    store: *local_evm.store.address(),
                    bytecode: Bytes::from_str(
                        "0x000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015020000000c02020002011000000110000000000000",
                    )
                    .unwrap(),
                },
                validInputs: vec![IOV2 {
                    token: *local_evm.tokens[0].address(),
                    vaultId: B256::from(U256::from(2)),
                }],
                validOutputs: vec![IOV2 {
                    token: *local_evm.tokens[1].address(),
                    vaultId: B256::from(U256::from(4)),
                }],
                nonce: alloy::primitives::private::rand::random::<U256>().into(),
                secret: alloy::primitives::private::rand::random::<U256>().into(),
                meta: Bytes::from_str("0xff0a89c674ee7874a30058382f2a20302e2063616c63756c6174652d696f202a2f200a5f205f3a203020303b0a0a2f2a20312e2068616e646c652d696f202a2f200a3a3b011bff13109e41336ff20278186170706c69636174696f6e2f6f637465742d73747265616d").unwrap(),
            },
            tasks: vec![
                TaskV3 {
                    evaluable: EvaluableV4 {
                        interpreter: *local_evm.interpreter.address(),
                        store: *local_evm.store.address(),
                        bytecode: Bytes::from_str("0x0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000701000000000000").unwrap(),
                    },
                    signedContext: vec![],
                },
            ],
        };

        let res = add_order_args
            .get_add_order_call_parameters(TransactionArgs {
                rpcs: vec![local_evm.url().to_string()],
                raindex_address: *local_evm.raindex.address(),
                max_priority_fee_per_gas: Some(100),
                max_fee_per_gas: Some(200),
                ..Default::default()
            })
            .await
            .unwrap();

        let input = res.input.input.expect("tx request has no input");
        let decoded = addOrder5Call::abi_decode(&input).unwrap();

        assert_eq!(decoded.config.evaluable, add_order_call.config.evaluable);
        assert_eq!(
            decoded.config.validInputs,
            add_order_call.config.validInputs
        );
        assert_eq!(
            decoded.config.validOutputs,
            add_order_call.config.validOutputs
        );
        assert_eq!(decoded.config.meta, add_order_call.config.meta);
        assert_eq!(decoded.tasks, add_order_call.tasks);
        assert_eq!(res.to, Some((*local_evm.raindex.address()).into()));
        assert_eq!(res.max_priority_fee_per_gas, Some(100));
        assert_eq!(res.max_fee_per_gas, Some(200));
    }

    #[tokio::test]
    async fn test_get_add_order_calldata() {
        let local_evm = LocalEvm::new().await;
        let deployment = get_deployment(&local_evm.url(), local_evm.rainlang);
        let dotrain = format!(
            "
version: {spec_version}
---
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
_ _: 0 0;
",
            spec_version = SpecVersion::current()
        );
        let result = AddOrderArgs::new_from_deployment(dotrain.to_string(), deployment, None)
            .await
            .unwrap();
        let calldata: Bytes = result
            .get_add_order_calldata(TransactionArgs {
                rpcs: vec![local_evm.url().to_string()],
                ..Default::default()
            })
            .await
            .unwrap()
            .into();

        let expected_bytes: Bytes = addOrder5Call {
            config: OrderConfigV4 {
                evaluable: EvaluableV4 {
                    interpreter: *local_evm.interpreter.address(),
                    store: *local_evm.store.address(),
                    bytecode: Bytes::from_str("0x000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015020000000c02020002011000000110000000000000").unwrap(),
                },
                validInputs: vec![
                    IOV2 {
                        token: Address::default(),
                        vaultId: B256::from(U256::from(2)),
                    },
                    IOV2 {
                        token: Address::default(),
                        vaultId: B256::from(U256::from(1)),
                    },
                ],
                validOutputs: vec![IOV2 {
                    token: Address::default(),
                    vaultId: B256::from(U256::from(4)),
                }],
                nonce: U256::from(0).into(),
                secret: U256::from(0).into(),
                meta: Bytes::from_str("0xff0a89c674ee7874a30058382f2a20302e2063616c63756c6174652d696f202a2f200a5f205f3a203020303b0a0a2f2a20312e2068616e646c652d696f202a2f200a3a3b011bff13109e41336ff20278186170706c69636174696f6e2f6f637465742d73747265616d").unwrap(),
            },
            tasks: vec![TaskV3 {
                evaluable: EvaluableV4 {
                    interpreter: *local_evm.interpreter.address(),
                    store: *local_evm.store.address(),
                    bytecode: Bytes::from_str("0x00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000f010000020200020110000001100000").unwrap(),
                },
                signedContext: vec![],
            }],
        }
        .abi_encode()
        .into();

        // Nonce and secret are random, so we can't compare the whole calldata
        assert_eq!(calldata[..164], expected_bytes[..164]);
        assert_eq!(calldata[228..], expected_bytes[228..]);
    }

    #[tokio::test]
    async fn test_get_add_order_calldata_invalid_rpc_url() {
        let local_evm = LocalEvm::new().await;
        let deployment = get_deployment(&local_evm.url(), local_evm.rainlang);
        let dotrain = format!(
            "
version: {spec_version}
---
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
_ _: 0 0;
",
            spec_version = SpecVersion::current()
        );
        let result = AddOrderArgs::new_from_deployment(dotrain.to_string(), deployment, None)
            .await
            .unwrap();
        let rpc_url = "https://testtest.com/".to_string();
        let err = result
            .get_add_order_calldata(TransactionArgs {
                rpcs: vec![rpc_url.clone()],
                ..Default::default()
            })
            .await
            .unwrap_err();

        assert!(
            matches!(
                &err,
                AddOrderArgsError::TransactionArgs(TransactionArgsError::Transport(_))
            ),
            "unexpected error variant: {err:?}"
        );
    }
}
