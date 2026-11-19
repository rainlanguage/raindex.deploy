use crate::transaction::TransactionArgsError;
use crate::{
    add_order::{RAINDEX_ADDORDER_POST_TASK_ENTRYPOINTS, RAINDEX_ORDER_ENTRYPOINTS},
    rainlang::compose_to_rainlang,
};
use alloy::primitives::Address;
use dotrain::{error::ComposeError, types::patterns::FRONTMATTER_SEPARATOR, RainDocument};
use futures::future::join_all;
use rain_interpreter_parser::{Parser2, ParserError, ParserV2};
pub use rain_metadata::types::authoring::v2::*;
use raindex_app_settings::yaml::{
    clone_section_entry, context::ContextProfile, dotrain::DotrainYaml, raindex::RaindexYaml,
    FieldErrorKind, YamlError, YamlParsable,
};
use raindex_app_settings::{
    remote_networks::ParseRemoteNetworksError,
    remote_tokens::ParseRemoteTokensError,
    yaml::{dotrain::DotrainYamlValidation, raindex::RaindexYamlValidation},
};
use raindex_app_settings::{scenario::ScenarioCfg, spec_version::SpecVersion};
use raindex_bindings::provider::{mk_read_provider, ReadProviderError};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use strict_yaml_rust::{strict_yaml::Hash as StrictYamlHash, StrictYaml, StrictYamlLoader};
use thiserror::Error;
use wasm_bindgen_utils::prelude::*;

/// Endpoint URLs can embed API keys; a parseable placeholder keeps required URL fields valid.
const STRIPPED_URL_PLACEHOLDER: &str = "https://example.com";

/// DotrainOrder represents a parsed and validated dotrain configuration that combines
/// YAML frontmatter with Rainlang code for raindex operations.
///
/// A dotrain file contains:
/// - YAML frontmatter defining networks, tokens, orders, scenarios, and deployments
/// - Rainlang code sections for order evaluation logic
///
/// This struct provides methods to compose scenarios and deployments into Rainlang code
/// with scenario-specific bindings applied.
///
/// # Examples
///
/// ```javascript
/// // Create from dotrain text
/// const result = await DotrainOrder.create(dotrainText);
/// if (result.error) {
///   console.error('Failed:', result.error.readableMsg);
/// } else {
///   const dotrainOrder = result.value;
///   // Do something with the dotrainOrder
/// }
///
/// // Compile scenario to Rainlang
/// const result = await dotrainOrder.composeScenarioToRainlang("my-scenario");
/// if (result.error) {
///   console.error('Failed:', result.error.readableMsg);
/// } else {
///   const rainlang = result.value;
///   // Do something with the rainlang
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[wasm_bindgen]
pub struct DotrainOrder {
    dotrain: String,
    dotrain_yaml: DotrainYaml,
}

impl PartialEq for DotrainOrder {
    fn eq(&self, other: &Self) -> bool {
        self.dotrain == other.dotrain
    }
}

#[derive(Error, Debug)]
pub enum DotrainOrderError {
    #[error("DotrainOrder is not initialized")]
    DotrainOrderNotInitialized,

    #[error("Scenario {0} not found")]
    ScenarioNotFound(String),

    #[error("Metaboard {0} not found")]
    MetaboardNotFound(String),

    #[error(transparent)]
    ComposeError(#[from] ComposeError),

    #[error(transparent)]
    AuthoringMetaV2Error(#[from] AuthoringMetaV2Error),

    #[error(transparent)]
    FetchAuthoringMetaV2WordError(#[from] Box<FetchAuthoringMetaV2WordError>),

    #[error(transparent)]
    ReadProviderError(#[from] ReadProviderError),

    #[error(transparent)]
    TransactionArgsError(#[from] TransactionArgsError),

    #[error(transparent)]
    ParserError(#[from] ParserError),

    #[error("{0}")]
    CleanUnusedFrontmatterError(String),

    #[error("Spec version mismatch: got {1}, should be {0}")]
    SpecVersionMismatch(String, String),

    #[error("Spec version missing: should be {0}")]
    MissingSpecVersion(String),

    #[error("Deployment {0} not found")]
    DeploymentNotFound(String),

    #[error("Order {0} not found")]
    OrderNotFound(String),

    #[error("Token {0} not found")]
    TokenNotFound(String),

    #[error("Invalid index for vault ID")]
    InvalidVaultIdIndex,

    #[error(transparent)]
    YamlError(#[from] YamlError),

    #[error(transparent)]
    ParseRemoteNetworksError(#[from] ParseRemoteNetworksError),
    #[error(transparent)]
    ParseRemoteTokensError(#[from] ParseRemoteTokensError),
}

impl From<FetchAuthoringMetaV2WordError> for DotrainOrderError {
    fn from(err: FetchAuthoringMetaV2WordError) -> Self {
        Self::FetchAuthoringMetaV2WordError(Box::new(err))
    }
}

impl DotrainOrderError {
    pub fn to_readable_msg(&self) -> String {
        match self {
            DotrainOrderError::DotrainOrderNotInitialized => {
                "DotrainOrder is not initialized. Please call initialize() first.".to_string()
            }
            DotrainOrderError::ScenarioNotFound(name) => {
                format!("Scenario '{name}' is not defined in the configuration.")
            }
            DotrainOrderError::MetaboardNotFound(name) => {
                format!("Metaboard configuration for network '{name}' is missing.")
            }
            DotrainOrderError::ComposeError(e) => {
                format!("Error composing the Rainlang script from the .rain file: {e}")
            }
            DotrainOrderError::AuthoringMetaV2Error(e) => {
                format!("Error processing contract authoring metadata: {e}")
            }
            DotrainOrderError::FetchAuthoringMetaV2WordError(e) => {
                format!("Error fetching words from contract authoring metadata: {e}")
            }
            DotrainOrderError::ReadProviderError(e) => {
                format!("Problem communicating with the rpc: {e}")
            }
            DotrainOrderError::TransactionArgsError(e) => {
                format!("Problem communicating with the rpc: {e}")
            }
            DotrainOrderError::ParserError(e) => {
                format!("Error parsing the Rainlang script: {e}")
            }
            DotrainOrderError::CleanUnusedFrontmatterError(e) => {
                format!("Internal configuration processing error: {e}")
            }
            DotrainOrderError::SpecVersionMismatch(expected, got) => {
                format!("Configuration version mismatch. Expected '{expected}', but found '{got}'. Please update 'version'.")
            }
            DotrainOrderError::MissingSpecVersion(expected) => {
                format!("The required 'version' field is missing. Please add it and set it to '{expected}'.")
            }
            DotrainOrderError::DeploymentNotFound(name) => {
                format!("Deployment '{name}' is not defined in the configuration.")
            }
            DotrainOrderError::OrderNotFound(name) => {
                format!("Order '{name}' is not defined in the configuration.")
            }
            DotrainOrderError::TokenNotFound(name) => {
                format!("Token '{name}' is not defined in the configuration.")
            }
            DotrainOrderError::InvalidVaultIdIndex => {
                "Internal error: Invalid index used for vault ID.".to_string()
            }
            DotrainOrderError::YamlError(e) => {
                format!("Error parsing the YAML configuration: {e}")
            }
            DotrainOrderError::ParseRemoteNetworksError(e) => {
                format!("Error parsing the remote networks configuration: {e}")
            }
            DotrainOrderError::ParseRemoteTokensError(e) => {
                format!("Error parsing the remote tokens configuration: {e}")
            }
        }
    }
}

impl From<DotrainOrderError> for JsValue {
    fn from(value: DotrainOrderError) -> Self {
        JsError::new(&value.to_string()).into()
    }
}

impl From<DotrainOrderError> for WasmEncodedError {
    fn from(value: DotrainOrderError) -> Self {
        WasmEncodedError {
            msg: value.to_string(),
            readable_msg: value.to_readable_msg(),
        }
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type", content = "data")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub enum WordsResult {
    Success(AuthoringMetaV2),
    Error(String),
}

#[derive(Serialize, Debug, Clone)]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct ContractWords {
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub address: Address,
    pub words: WordsResult,
}

impl From<Result<AuthoringMetaV2, DotrainOrderError>> for WordsResult {
    fn from(result: Result<AuthoringMetaV2, DotrainOrderError>) -> Self {
        match result {
            Ok(meta) => WordsResult::Success(meta),
            Err(err) => WordsResult::Error(err.to_string()),
        }
    }
}

#[derive(Serialize, Debug, Clone)]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct ScenarioWords {
    pub scenario: String,
    pub pragma_words: Vec<ContractWords>,
    pub rainlang_words: ContractWords,
}

impl DotrainOrder {
    pub fn dummy() -> Self {
        Self {
            dotrain: "".to_string(),
            dotrain_yaml: DotrainYaml::new(vec![], DotrainYamlValidation::default()).unwrap(),
        }
    }
}

#[wasm_export]
impl DotrainOrder {
    /// Creates a new DotrainOrder instance by parsing dotrain configuration text along with additional configuration.
    ///
    /// Parses the YAML frontmatter and validates the configuration including:
    /// - Spec version compatibility
    /// - Remote network configurations
    /// - Remote token definitions
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// // Basic usage
    /// const result = await DotrainOrder.create(dotrainText);
    /// if (result.error) {
    ///   console.error('Failed:', result.error.readableMsg);
    /// } else {
    ///   const dotrainOrder = result.value;
    ///   // Do something with the dotrainOrder
    /// }
    ///
    /// // With additional settings
    /// const result = await DotrainOrder.create(dotrainText, [additionalConfig]);
    /// if (result.error) {
    ///   console.error('Failed:', result.error.readableMsg);
    /// } else {
    ///   const dotrainOrder = result.value;
    ///   // Do something with the dotrainOrder
    /// }
    /// ```
    #[wasm_export(
        js_name = "create",
        preserve_js_class,
        return_description = "Successfully parsed and validated DotrainOrder instance"
    )]
    pub async fn create(
        #[wasm_export(
            param_description = "Complete dotrain text containing YAML frontmatter and Rainlang code"
        )]
        dotrain: String,
        #[wasm_export(
            param_description = "Optional additional YAML configuration strings to merge with the frontmatter"
        )]
        settings: Option<Vec<String>>,
    ) -> Result<DotrainOrder, DotrainOrderError> {
        Self::create_with_profile(dotrain, settings, ContextProfile::Strict).await
    }

    #[wasm_export(skip)]
    pub async fn create_with_profile(
        dotrain: String,
        settings: Option<Vec<String>>,
        profile: ContextProfile,
    ) -> Result<DotrainOrder, DotrainOrderError> {
        let frontmatter = RainDocument::get_front_matter(&dotrain)
            .unwrap_or("")
            .to_string();

        let mut sources = vec![frontmatter.clone()];
        if let Some(settings) = settings {
            sources.extend(settings);
        }

        let mut raindex_yaml = RaindexYaml::new(sources.clone(), RaindexYamlValidation::default())?;

        let mut dotrain_yaml = DotrainYaml::new_with_profile(
            sources.clone(),
            DotrainYamlValidation::default(),
            profile,
        )?;

        let remote_data = raindex_yaml.fetch_remote_data().await?;
        if !remote_data.remote_networks.is_empty() {
            dotrain_yaml
                .cache
                .update_remote_networks(remote_data.remote_networks);
        }
        if !remote_data.remote_tokens.is_empty() {
            dotrain_yaml
                .cache
                .update_remote_tokens(remote_data.remote_tokens);
        }

        Ok(DotrainOrder {
            dotrain,
            dotrain_yaml,
        })
    }

    /// Returns the original dotrain text used to create this DotrainOrder instance.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = dotrainOrder.dotrain();
    /// if (result.error) {
    ///   console.error('Failed:', result.error.readableMsg);
    /// } else {
    ///   const dotrain = result.value;
    ///   // Do something with the dotrain
    /// }
    /// ```
    #[wasm_export(
        js_name = "dotrain",
        unchecked_return_type = "string",
        return_description = "The complete dotrain text including YAML frontmatter and Rainlang code"
    )]
    pub fn dotrain(&self) -> Result<String, DotrainOrderError> {
        Ok(self.dotrain.clone())
    }

    /// Composes a specific scenario into Rainlang code.
    ///
    /// Takes a scenario name from the dotrain configuration and composes the Rainlang
    /// code with that scenario's bindings applied.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// // Compile a trading scenario
    /// const result = await dotrainOrder.composeScenarioToRainlang("market-making");
    /// if (result.error) {
    ///   console.error('Failed:', result.error.readableMsg);
    /// } else {
    ///   const rainlang = result.value;
    ///   // Do something with the rainlang
    /// }
    /// ```
    #[wasm_export(
        js_name = "composeScenarioToRainlang",
        unchecked_return_type = "string",
        return_description = "Composed Rainlang code with scenario bindings applied"
    )]
    pub async fn compose_scenario_to_rainlang(
        &self,
        #[wasm_export(
            param_description = "Name of the scenario defined in the dotrain YAML frontmatter"
        )]
        scenario: String,
    ) -> Result<String, DotrainOrderError> {
        let scenario = self.dotrain_yaml.get_scenario(&scenario)?;

        Ok(compose_to_rainlang(
            self.dotrain.clone(),
            scenario.bindings.clone(),
            &RAINDEX_ORDER_ENTRYPOINTS,
        )?)
    }

    /// Composes handle-add-order entrypoint for a specific scenario into Rainlang code
    /// for immediate execution after an order is added.
    ///
    /// This is useful for scenarios that need to perform actions immediately after an order is added.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await dotrainOrder.composeScenarioToPostTaskRainlang("scenario");
    /// if (result.error) {
    ///   console.error('Failed:', result.error.readableMsg);
    /// } else {
    ///   const postTaskCode = result.value;
    ///   // Do something with the postTaskCode
    /// }
    /// ```
    #[wasm_export(
        js_name = "composeScenarioToPostTaskRainlang",
        unchecked_return_type = "string",
        return_description = "Composed handle-add-order Rainlang code with scenario bindings applied"
    )]
    pub async fn compose_scenario_to_post_task_rainlang(
        &self,
        #[wasm_export(
            param_description = "Name of the scenario defined in the dotrain YAML frontmatter"
        )]
        scenario: String,
    ) -> Result<String, DotrainOrderError> {
        let scenario = self.dotrain_yaml.get_scenario(&scenario)?;

        Ok(compose_to_rainlang(
            self.dotrain.clone(),
            scenario.bindings.clone(),
            &RAINDEX_ADDORDER_POST_TASK_ENTRYPOINTS,
        )?)
    }

    /// Composes a specific deployment configuration into Rainlang code.
    ///
    /// A deployment combines an order definition with a scenario to create a complete
    /// configuration ready for deployment. This method resolves the deployment's scenario
    /// and composes the Rainlang code with the appropriate bindings.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// // Compose a production deployment
    /// const result = await dotrainOrder.composeDeploymentToRainlang("production");
    /// if (result.error) {
    ///   console.error('Failed:', result.error.readableMsg);
    /// } else {
    ///   const rainlang = result.value;
    ///   // Do something with the rainlang
    /// }
    /// ```
    #[wasm_export(
        js_name = "composeDeploymentToRainlang",
        unchecked_return_type = "string",
        return_description = "Composed Rainlang code for the deployment's scenario"
    )]
    pub async fn compose_deployment_to_rainlang(
        &self,
        #[wasm_export(
            param_description = "Name of the deployment defined in the dotrain YAML frontmatter"
        )]
        deployment: String,
    ) -> Result<String, DotrainOrderError> {
        let scenario = self.dotrain_yaml.get_deployment(&deployment)?.scenario;

        Ok(compose_to_rainlang(
            self.dotrain.clone(),
            scenario.bindings.clone(),
            &RAINDEX_ORDER_ENTRYPOINTS,
        )?)
    }
}

impl DotrainOrder {
    pub fn dotrain_yaml(&self) -> DotrainYaml {
        self.dotrain_yaml.clone()
    }

    pub fn raindex_yaml(&self) -> RaindexYaml {
        RaindexYaml::from_dotrain_yaml(self.dotrain_yaml.clone())
    }

    pub async fn get_pragmas_for_scenario(
        &self,
        scenario: &str,
    ) -> Result<Vec<Address>, DotrainOrderError> {
        let rainlang_cfg = self.dotrain_yaml.get_scenario(scenario)?.rainlang;
        let parser: ParserV2 = rainlang_cfg.address.into();
        let rainlang = self
            .compose_scenario_to_rainlang(scenario.to_string())
            .await?;

        let urls = rainlang_cfg.network.rpcs.clone();
        let provider = mk_read_provider(&urls)?;
        let pragmas = parser.parse_pragma_text(&rainlang, &provider).await?;
        Ok(pragmas)
    }

    pub async fn get_contract_authoring_meta_v2_for_scenario(
        &self,
        scenario: &str,
        address: Address,
    ) -> Result<AuthoringMetaV2, DotrainOrderError> {
        let network = &self.dotrain_yaml.get_scenario(scenario)?.rainlang.network;

        let rpcs = network
            .rpcs
            .iter()
            .map(|rpc| rpc.to_string())
            .collect::<Vec<String>>();
        let metaboard = self.raindex_yaml().get_metaboard(&network.key)?.url;
        Ok(AuthoringMetaV2::fetch_for_contract(address, rpcs, metaboard.to_string()).await?)
    }

    pub async fn get_rainlang_words_for_scenario(
        &self,
        scenario: &str,
    ) -> Result<ContractWords, DotrainOrderError> {
        let rainlang = &self.dotrain_yaml.get_scenario(scenario)?.rainlang.address;

        Ok(ContractWords {
            address: *rainlang,
            words: self
                .get_contract_authoring_meta_v2_for_scenario(scenario, *rainlang)
                .await
                .into(),
        })
    }

    pub async fn get_pragma_words_for_scenario(
        &self,
        scenario: &str,
    ) -> Result<Vec<ContractWords>, DotrainOrderError> {
        let pragma_addresses = self.get_pragmas_for_scenario(scenario).await?;
        let mut futures = vec![];

        for pragma in &pragma_addresses {
            futures.push(self.get_contract_authoring_meta_v2_for_scenario(scenario, *pragma));
        }

        Ok(pragma_addresses
            .into_iter()
            .zip(join_all(futures).await)
            .map(|(address, words)| ContractWords {
                address,
                words: words.into(),
            })
            .collect())
    }

    pub async fn get_all_words_for_scenario(
        &self,
        scenario: &str,
    ) -> Result<ScenarioWords, DotrainOrderError> {
        let rainlang = &self.dotrain_yaml.get_scenario(scenario)?.rainlang.address;
        let mut addresses = vec![*rainlang];
        addresses.extend(self.get_pragmas_for_scenario(scenario).await?);

        let mut futures = vec![];
        for address in addresses.clone() {
            futures.push(self.get_contract_authoring_meta_v2_for_scenario(scenario, address));
        }
        let mut results = join_all(futures).await;

        let rainlang_words = ContractWords {
            address: *rainlang,
            words: results.drain(0..1).nth(0).unwrap().into(),
        };
        let pragma_words = results
            .into_iter()
            .enumerate()
            .map(|(i, v)| ContractWords {
                address: addresses[i + 1],
                words: v.into(),
            })
            .collect();

        Ok(ScenarioWords {
            scenario: scenario.to_string(),
            pragma_words,
            rainlang_words,
        })
    }

    pub async fn get_all_scenarios_all_words(
        &self,
    ) -> Result<Vec<ScenarioWords>, DotrainOrderError> {
        let mut scenarios = vec![];
        for scenario in self.dotrain_yaml.get_scenario_keys()? {
            scenarios.push(self.get_all_words_for_scenario(&scenario).await?);
        }
        Ok(scenarios)
    }

    pub async fn validate_spec_version(&self) -> Result<(), DotrainOrderError> {
        let spec_version = self.raindex_yaml().get_spec_version()?;
        if !SpecVersion::is_current(&spec_version) {
            return Err(DotrainOrderError::SpecVersionMismatch(
                SpecVersion::current(),
                spec_version,
            ));
        }

        Ok(())
    }

    pub fn generate_dotrain_for_deployment(
        &self,
        deployment_key: &str,
    ) -> Result<String, DotrainOrderError> {
        let dotrain_yaml = self.dotrain_yaml();
        let raindex_yaml = self.raindex_yaml();
        let deployment = dotrain_yaml.get_deployment(deployment_key)?;
        let order_cfg = deployment.order.clone();
        let scenario_cfg = deployment.scenario.clone();
        let rainlang_cfg = scenario_cfg.rainlang.clone();

        let network_key = order_cfg.network.key.clone();
        let rainlang_key = rainlang_cfg.key.clone();
        let raindex_key = order_cfg
            .raindex
            .as_ref()
            .map(|raindex_cfg| raindex_cfg.key.clone());
        let subgraph_key = order_cfg
            .raindex
            .as_ref()
            .map(|raindex_cfg| raindex_cfg.subgraph.key.clone());

        let order_key = order_cfg.key.clone();
        let deployment_key = deployment.key.clone();

        let metaboard_key = match raindex_yaml.get_metaboard(&network_key) {
            Ok(cfg) => Some(cfg.key.clone()),
            Err(YamlError::KeyNotFound(_)) => None,
            Err(YamlError::Field {
                kind: FieldErrorKind::Missing(_),
                ..
            }) => None,
            Err(err) => return Err(err.into()),
        };

        let documents = dotrain_yaml.documents.clone();

        let spec_version = raindex_yaml.get_spec_version()?;

        let mut root_hash = StrictYamlHash::new();
        root_hash.insert(
            StrictYaml::String("version".to_string()),
            StrictYaml::String(spec_version.to_string()),
        );

        let mut network_value = clone_section_entry(&documents, "networks", &network_key)
            .map_err(|err| DotrainOrderError::CleanUnusedFrontmatterError(err.to_string()))?;
        Self::strip_rpcs_from_network(&mut network_value);
        let mut networks_hash = StrictYamlHash::new();
        networks_hash.insert(StrictYaml::String(network_key.clone()), network_value);
        root_hash.insert(
            StrictYaml::String("networks".to_string()),
            StrictYaml::Hash(networks_hash),
        );

        let rainlang_value = clone_section_entry(&documents, "rainlangs", &rainlang_key)
            .map_err(|err| DotrainOrderError::CleanUnusedFrontmatterError(err.to_string()))?;
        let mut rainlangs_hash = StrictYamlHash::new();
        rainlangs_hash.insert(StrictYaml::String(rainlang_key.clone()), rainlang_value);
        root_hash.insert(
            StrictYaml::String("rainlangs".to_string()),
            StrictYaml::Hash(rainlangs_hash),
        );

        if let Some(raindex_key) = raindex_key {
            let raindex_value = clone_section_entry(&documents, "raindexes", &raindex_key)
                .map_err(|err| DotrainOrderError::CleanUnusedFrontmatterError(err.to_string()))?;
            let mut raindexes_hash = StrictYamlHash::new();
            raindexes_hash.insert(StrictYaml::String(raindex_key.clone()), raindex_value);
            root_hash.insert(
                StrictYaml::String("raindexes".to_string()),
                StrictYaml::Hash(raindexes_hash),
            );
        }

        if let Some(subgraph_key) = subgraph_key {
            clone_section_entry(&documents, "subgraphs", &subgraph_key)
                .map_err(|err| DotrainOrderError::CleanUnusedFrontmatterError(err.to_string()))?;
            let mut subgraphs_hash = StrictYamlHash::new();
            subgraphs_hash.insert(
                StrictYaml::String(subgraph_key.clone()),
                StrictYaml::String(STRIPPED_URL_PLACEHOLDER.to_string()),
            );
            root_hash.insert(
                StrictYaml::String("subgraphs".to_string()),
                StrictYaml::Hash(subgraphs_hash),
            );
        }

        if let Some(metaboard_key) = metaboard_key {
            clone_section_entry(&documents, "metaboards", &metaboard_key)
                .map_err(|err| DotrainOrderError::CleanUnusedFrontmatterError(err.to_string()))?;
            let mut metaboards_hash = StrictYamlHash::new();
            metaboards_hash.insert(
                StrictYaml::String(metaboard_key.clone()),
                StrictYaml::String(STRIPPED_URL_PLACEHOLDER.to_string()),
            );
            root_hash.insert(
                StrictYaml::String("metaboards".to_string()),
                StrictYaml::Hash(metaboards_hash),
            );
        }

        let mut order_value = clone_section_entry(&documents, "orders", &order_key)
            .map_err(|err| DotrainOrderError::CleanUnusedFrontmatterError(err.to_string()))?;
        Self::strip_vault_ids_from_order(&mut order_value);
        let mut orders_hash = StrictYamlHash::new();
        orders_hash.insert(StrictYaml::String(order_key.clone()), order_value);
        root_hash.insert(
            StrictYaml::String("orders".to_string()),
            StrictYaml::Hash(orders_hash),
        );

        let deployment_value = clone_section_entry(&documents, "deployments", &deployment_key)
            .map_err(|err| DotrainOrderError::CleanUnusedFrontmatterError(err.to_string()))?;
        let mut deployments_hash = StrictYamlHash::new();
        deployments_hash.insert(StrictYaml::String(deployment_key.clone()), deployment_value);
        root_hash.insert(
            StrictYaml::String("deployments".to_string()),
            StrictYaml::Hash(deployments_hash),
        );

        if let Some(builder_yaml) =
            Self::clone_builder_config_for_deployment(&documents, deployment_key.as_str())?
        {
            root_hash.insert(StrictYaml::String("builder".to_string()), builder_yaml);
        }
        let scenario_yaml = Self::scenario_to_yaml(&scenario_cfg)?;
        let mut scenarios_hash = StrictYamlHash::new();
        scenarios_hash.insert(StrictYaml::String(scenario_cfg.key.clone()), scenario_yaml);
        root_hash.insert(
            StrictYaml::String("scenarios".to_string()),
            StrictYaml::Hash(scenarios_hash),
        );

        let pruned_doc = Arc::new(RwLock::new(StrictYaml::Hash(root_hash)));
        let yaml_frontmatter = DotrainYaml::get_yaml_string(pruned_doc.clone())?;

        let rain_document = RainDocument::create(self.dotrain.clone(), None, None, None);
        let dotrain = format!(
            "{}\n{}\n{}",
            yaml_frontmatter,
            FRONTMATTER_SEPARATOR,
            rain_document.body()
        );

        Ok(dotrain)
    }

    fn clone_builder_config_for_deployment(
        documents: &[Arc<RwLock<StrictYaml>>],
        deployment_key: &str,
    ) -> Result<Option<StrictYaml>, DotrainOrderError> {
        let mut builder_value: Option<StrictYaml> = None;

        for document in documents {
            let document_read = document.read().map_err(|_| {
                DotrainOrderError::CleanUnusedFrontmatterError(
                    "Failed to read YAML document while cloning builder config section".to_string(),
                )
            })?;

            if let StrictYaml::Hash(root_hash) = &*document_read {
                if let Some(builder) = root_hash.get(&StrictYaml::String("builder".to_string())) {
                    builder_value = Some(builder.clone());
                    break;
                }
            }
        }

        let Some(StrictYaml::Hash(mut builder_hash)) = builder_value else {
            return Ok(None);
        };

        let Some(deployments_yaml) = builder_hash
            .get(&StrictYaml::String("deployments".to_string()))
            .cloned()
        else {
            return Ok(None);
        };

        let StrictYaml::Hash(deployments_hash) = deployments_yaml else {
            return Err(DotrainOrderError::CleanUnusedFrontmatterError(
                "Builder config deployments section is not a map".to_string(),
            ));
        };

        let Some(deployment_yaml) = deployments_hash
            .get(&StrictYaml::String(deployment_key.to_string()))
            .cloned()
        else {
            return Ok(None);
        };

        let mut filtered_deployments = StrictYamlHash::new();
        filtered_deployments.insert(
            StrictYaml::String(deployment_key.to_string()),
            deployment_yaml,
        );

        builder_hash.insert(
            StrictYaml::String("deployments".to_string()),
            StrictYaml::Hash(filtered_deployments),
        );

        Ok(Some(StrictYaml::Hash(builder_hash)))
    }

    fn strip_rpcs_from_network(network_yaml: &mut StrictYaml) {
        if let StrictYaml::Hash(network_hash) = network_yaml {
            let rpcs_key = StrictYaml::String("rpcs".to_string());
            if let Some(StrictYaml::Array(rpcs)) = network_hash.get_mut(&rpcs_key) {
                for rpc in rpcs.iter_mut() {
                    *rpc = StrictYaml::String(STRIPPED_URL_PLACEHOLDER.to_string());
                }
            }
        }
    }

    fn strip_vault_ids_from_order(order_yaml: &mut StrictYaml) {
        if let StrictYaml::Hash(order_hash) = order_yaml {
            for section in ["inputs", "outputs"] {
                let section_key = StrictYaml::String(section.to_string());
                if let Some(StrictYaml::Array(io_entries)) = order_hash.get_mut(&section_key) {
                    for entry in io_entries.iter_mut() {
                        if let StrictYaml::Hash(io_hash) = entry {
                            io_hash.remove(&StrictYaml::String("vault-id".to_string()));
                        }
                    }
                }
            }
        }
    }

    fn scenario_to_yaml(scenario: &ScenarioCfg) -> Result<StrictYaml, DotrainOrderError> {
        let mut scenario_hash = StrictYamlHash::new();
        scenario_hash.insert(
            StrictYaml::String("rainlang".to_string()),
            StrictYaml::String(scenario.rainlang.key.clone()),
        );

        if let Some(runs) = scenario.runs {
            scenario_hash.insert(
                StrictYaml::String("runs".to_string()),
                StrictYaml::String(runs.to_string()),
            );
        }

        if let Some(blocks) = &scenario.blocks {
            let blocks_yaml = serde_yaml::to_string(blocks).map_err(|err| {
                DotrainOrderError::CleanUnusedFrontmatterError(format!(
                    "Failed to serialise blocks: {err}"
                ))
            })?;
            let blocks_docs = StrictYamlLoader::load_from_str(&blocks_yaml).map_err(|err| {
                DotrainOrderError::CleanUnusedFrontmatterError(format!(
                    "Failed to parse blocks YAML: {err}"
                ))
            })?;
            if let Some(blocks_doc) = blocks_docs.into_iter().next() {
                scenario_hash.insert(StrictYaml::String("blocks".to_string()), blocks_doc);
            }
        }

        Ok(StrictYaml::Hash(scenario_hash))
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use alloy::{hex::encode_prefixed, primitives::B256, sol, sol_types::SolValue};
    use httpmock::MockServer;
    use rain_metadata::{KnownMagic, RainMetaDocumentV1Item};
    use raindex_app_settings::yaml::FieldErrorKind;
    use serde_bytes::ByteBuf;
    use serde_json::json;
    use strict_yaml_rust::{strict_yaml::Hash as StrictYamlHash, StrictYaml, StrictYamlLoader};

    sol!(
        struct AuthoringMetaV2Sol {
            bytes32 word;
            string description;
        }
    );
    sol!(
        struct PragmaV1 { address[] usingWordsFrom; }
    );

    // Normalize Rainlang strings so assertions ignore trailing whitespace changes.
    fn normalize_rainlang(value: &str) -> String {
        value
            .split('\n')
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn test_config_parse() {
        let server = mock_server(vec![]);
        let dotrain = format!(
            r#"
version: {spec_version}
networks:
    polygon:
        rpcs:
            - {rpc_url}
        chain-id: 137
        network-id: 137
        currency: MATIC
rainlangs:
    polygon:
        address: 0x1234567890123456789012345678901234567890
scenarios:
    polygon:
        rainlang: polygon
        bindings:
            key1: 10
---
#key1 !Test binding
#calculate-io
_ _: 0 0;
#handle-io
:;"#,
            rpc_url = server.url("/rpc"),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        assert_eq!(
            dotrain_order
                .raindex_yaml()
                .get_network("polygon")
                .unwrap()
                .rpcs
                .iter()
                .map(|rpc| rpc.to_string())
                .collect::<Vec<String>>()
                .first()
                .unwrap()
                .to_string(),
            server.url("/rpc"),
        );
    }

    #[tokio::test]
    async fn test_rainlang_from_scenario() {
        let server = mock_server(vec![]);
        let dotrain = format!(
            r#"
version: {spec_version}
networks:
    polygon:
        rpcs:
            - {rpc_url}
        chain-id: 137
        network-id: 137
        currency: MATIC
rainlangs:
    polygon:
        address: 0x1234567890123456789012345678901234567890
scenarios:
    polygon:
        bindings:
            key1: 10
        rainlang: polygon
---
#key1 !Test binding
#calculate-io
_ _: 0 0;
#handle-io
:;"#,
            rpc_url = server.url("/rpc"),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let rainlang = dotrain_order
            .compose_scenario_to_rainlang("polygon".to_string())
            .await
            .unwrap();

        assert_eq!(
            normalize_rainlang(&rainlang),
            normalize_rainlang(
                r#"/* 0. calculate-io */
_ _: 0 0;

/* 1. handle-io */
:;"#
            )
        );
    }

    fn split_frontmatter_and_body(dotrain: &str) -> (String, String) {
        dotrain
            .split_once(&format!("\n{sep}\n", sep = FRONTMATTER_SEPARATOR))
            .map(|(frontmatter, body)| (frontmatter.to_string(), body.to_string()))
            .expect("Dotrain string should contain frontmatter separator")
    }

    fn get_root_hash(frontmatter: &str) -> StrictYamlHash {
        let docs = StrictYamlLoader::load_from_str(frontmatter).expect("frontmatter should parse");
        let StrictYaml::Hash(root) = docs.first().expect("yaml doc exists") else {
            panic!("frontmatter root is not a hash")
        };
        root.clone()
    }

    fn assert_no_vault_ids(entries: &[StrictYaml]) {
        for entry in entries {
            let StrictYaml::Hash(io_hash) = entry else {
                panic!("io entry is not a hash");
            };
            assert!(
                !io_hash.contains_key(&StrictYaml::String("vault-id".to_string())),
                "vault-id should be stripped from io entry"
            );
        }
    }

    #[tokio::test]
    async fn test_generate_dotrain_for_deployment_strips_vault_ids_and_unused_sections() {
        let dotrain = format!(
            r#"
version: {spec_version}
networks:
  polygon:
    rpcs:
      - http://example.com
    chain-id: 137
    network-id: 137
    currency: MATIC
rainlangs:
  polygon:
    address: 0x1234567890123456789012345678901234567890
tokens:
  t1:
    network: polygon
    address: 0x1111111111111111111111111111111111111111
  t2:
    network: polygon
    address: 0x2222222222222222222222222222222222222222
orders:
  polygon-order:
    network: polygon
    inputs:
      - token: t1
        vault-id: 1
    outputs:
      - token: t2
        vault-id: 2
deployments:
  polygon-deployment:
    scenario: polygon
    order: polygon-order
scenarios:
  polygon:
    rainlang: polygon
---
#calculate-io
_ _: 0 0;
#handle-io
:;"#,
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let generated = dotrain_order
            .generate_dotrain_for_deployment("polygon-deployment")
            .unwrap();

        let (frontmatter, body) = split_frontmatter_and_body(&generated);
        let root = get_root_hash(&frontmatter);

        assert!(root
            .get(&StrictYaml::String("tokens".to_string()))
            .is_none());

        let StrictYaml::Hash(orders_hash) = root
            .get(&StrictYaml::String("orders".to_string()))
            .expect("orders present")
            .clone()
        else {
            panic!("orders section not a hash");
        };
        let StrictYaml::Hash(order) = orders_hash
            .get(&StrictYaml::String("polygon-order".to_string()))
            .expect("order exists")
            .clone()
        else {
            panic!("order is not a hash");
        };

        let StrictYaml::Array(inputs) = order
            .get(&StrictYaml::String("inputs".to_string()))
            .expect("inputs exist")
            .clone()
        else {
            panic!("inputs not an array");
        };
        let StrictYaml::Array(outputs) = order
            .get(&StrictYaml::String("outputs".to_string()))
            .expect("outputs exist")
            .clone()
        else {
            panic!("outputs not an array");
        };

        assert_no_vault_ids(&inputs);
        assert_no_vault_ids(&outputs);

        assert!(
            normalize_rainlang(body.trim())
                == normalize_rainlang(
                    r#"#calculate-io
_ _: 0 0;
#handle-io
:;"#
                )
        );
    }

    #[tokio::test]
    async fn test_generate_dotrain_for_deployment_includes_only_referenced_entries() {
        let dotrain = format!(
            r#"
version: {spec_version}
networks:
  polygon:
    rpcs:
      - http://example.com
    chain-id: 137
    network-id: 137
    currency: MATIC
rainlangs:
  polygon:
    address: 0x1234567890123456789012345678901234567890
    network: polygon
raindexes:
  primary:
    address: 0x0101010101010101010101010101010101010101
    network: polygon
    subgraph: sg-primary
    deployment-block: 1
  unused:
    address: 0x0202020202020202020202020202020202020202
    network: polygon
    subgraph: sg-unused
    deployment-block: 2
subgraphs:
  sg-primary: https://example.com/sg-primary
  sg-unused: https://example.com/sg-unused
tokens:
  t1:
    network: polygon
    address: 0x1111111111111111111111111111111111111111
orders:
  polygon-order:
    network: polygon
    raindex: primary
    inputs:
      - token: t1
        vault-id: 1
    outputs:
      - token: t1
        vault-id: 2
deployments:
  polygon-deployment:
    scenario: polygon
    order: polygon-order
scenarios:
  polygon:
    rainlang: polygon
builder:
  deployments:
    polygon-deployment:
      name: Used deployment
      fields: []
    unused-deployment:
      name: Unused deployment
      fields: []
---
#calculate-io
_ _: 0 0;
#handle-io
:;"#,
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let generated = dotrain_order
            .generate_dotrain_for_deployment("polygon-deployment")
            .unwrap();

        let (frontmatter, _) = split_frontmatter_and_body(&generated);
        let root = get_root_hash(&frontmatter);

        let StrictYaml::Hash(raindexes) = root
            .get(&StrictYaml::String("raindexes".to_string()))
            .expect("raindexes present")
            .clone()
        else {
            panic!("raindexes not a hash");
        };
        assert!(raindexes.contains_key(&StrictYaml::String("primary".to_string())));
        assert!(!raindexes.contains_key(&StrictYaml::String("unused".to_string())));

        let StrictYaml::Hash(subgraphs) = root
            .get(&StrictYaml::String("subgraphs".to_string()))
            .expect("subgraphs present")
            .clone()
        else {
            panic!("subgraphs not a hash");
        };
        assert!(subgraphs.contains_key(&StrictYaml::String("sg-primary".to_string())));
        assert!(!subgraphs.contains_key(&StrictYaml::String("sg-unused".to_string())));

        let StrictYaml::Hash(builder_config) = root
            .get(&StrictYaml::String("builder".to_string()))
            .expect("builder config present")
            .clone()
        else {
            panic!("builder config not a hash");
        };
        let StrictYaml::Hash(deployments) = builder_config
            .get(&StrictYaml::String("deployments".to_string()))
            .expect("builder config deployments present")
            .clone()
        else {
            panic!("builder config deployments not a hash");
        };
        assert!(deployments.contains_key(&StrictYaml::String("polygon-deployment".to_string())));
        assert!(!deployments.contains_key(&StrictYaml::String("unused-deployment".to_string())));
    }

    #[tokio::test]
    async fn test_generate_dotrain_for_deployment_missing_key() {
        let dotrain = format!(
            r#"
version: {spec_version}
networks:
  polygon:
    rpcs:
      - http://example.com
    chain-id: 137
rainlangs:
  polygon:
    address: 0x1234567890123456789012345678901234567890
scenarios:
  polygon:
    rainlang: polygon
orders:
  polygon:
    network: polygon
    inputs:
      - token: t1
    outputs:
      - token: t1
tokens:
  t1:
    network: polygon
    address: 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
deployments:
  polygon:
    scenario: polygon
    order: polygon
---
#calculate-io
:;"#,
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let err = dotrain_order
            .generate_dotrain_for_deployment("does-not-exist")
            .unwrap_err();

        assert!(matches!(
            err,
            DotrainOrderError::YamlError(YamlError::KeyNotFound(ref key)) if key == "does-not-exist"
        ));
    }

    fn collect_urls(yaml: &StrictYaml, path: String, urls: &mut Vec<(String, String)>) {
        match yaml {
            StrictYaml::String(value) => {
                if url::Url::parse(value).is_ok_and(|parsed| parsed.has_host()) {
                    urls.push((path, value.clone()));
                }
            }
            StrictYaml::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    collect_urls(item, format!("{path}[{i}]"), urls);
                }
            }
            StrictYaml::Hash(hash) => {
                for (key, value) in hash {
                    let key = key.as_str().unwrap_or_default();
                    collect_urls(value, format!("{path}.{key}"), urls);
                }
            }
            _ => {}
        }
    }

    #[tokio::test]
    async fn test_generate_dotrain_for_deployment_replaces_endpoint_urls_keeps_oracle_url() {
        let secrets = [
            "rpcPathKey0123456789",
            "rpcQueryToken42",
            "subgraphPathKeyabcdef",
            "subgraphQueryToken77",
            "metaboardPathKey9876",
            "metaboardQueryToken13",
        ];
        let oracle_url = "https://oracle.example.org/v1/context?pair=eth-usdc";
        let dotrain = format!(
            r#"
version: {spec_version}
networks:
  polygon:
    rpcs:
      - https://eth-mainnet.example.org/v2/{rpc_path}
      - https://rpc.example.org/?apikey={rpc_query}
    chain-id: 137
    network-id: 137
    currency: MATIC
rainlangs:
  polygon:
    address: 0x1234567890123456789012345678901234567890
    network: polygon
raindexes:
  primary:
    address: 0x0101010101010101010101010101010101010101
    network: polygon
    subgraph: sg-primary
    deployment-block: 1
subgraphs:
  sg-primary: https://gateway.thegraph.com/api/{sg_path}/subgraphs/id/abc?key={sg_query}
metaboards:
  polygon: https://gateway.thegraph.com/api/{mb_path}/subgraphs/id/def?key={mb_query}
tokens:
  t1:
    network: polygon
    address: 0x1111111111111111111111111111111111111111
  t2:
    network: polygon
    address: 0x2222222222222222222222222222222222222222
orders:
  polygon-order:
    network: polygon
    raindex: primary
    oracle-url: {oracle_url}
    inputs:
      - token: t1
        vault-id: 1
    outputs:
      - token: t2
        vault-id: 2
deployments:
  polygon-deployment:
    scenario: polygon
    order: polygon-order
scenarios:
  polygon:
    rainlang: polygon
builder:
  name: Builder
  description: Builder description
  short-description: Short
  deployments:
    polygon-deployment:
      name: Deployment
      description: Deployment description
      fields: []
---
#calculate-io
_ _: 0 0;
#handle-io
:;"#,
            spec_version = SpecVersion::current(),
            rpc_path = secrets[0],
            rpc_query = secrets[1],
            sg_path = secrets[2],
            sg_query = secrets[3],
            mb_path = secrets[4],
            mb_query = secrets[5],
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();
        let generated = dotrain_order
            .generate_dotrain_for_deployment("polygon-deployment")
            .unwrap();

        for secret in secrets {
            assert!(!generated.contains(secret), "{secret} leaked");
        }

        let (frontmatter, _body) = split_frontmatter_and_body(&generated);
        let root = get_root_hash(&frontmatter);
        let mut urls = vec![];
        collect_urls(&StrictYaml::Hash(root), String::new(), &mut urls);
        let mut paths: Vec<&str> = urls.iter().map(|(path, _)| path.as_str()).collect();
        paths.sort();
        assert_eq!(
            paths,
            vec![
                ".metaboards.polygon",
                ".networks.polygon.rpcs[0]",
                ".networks.polygon.rpcs[1]",
                ".orders.polygon-order.oracle-url",
                ".subgraphs.sg-primary",
            ]
        );
        for (path, value) in &urls {
            if path == ".orders.polygon-order.oracle-url" {
                assert_eq!(value, oracle_url);
            } else {
                assert_eq!(value, STRIPPED_URL_PLACEHOLDER, "{path} not replaced");
            }
        }

        let tokens_settings = format!(
            r#"
version: {spec_version}
tokens:
  t1:
    network: polygon
    address: 0x1111111111111111111111111111111111111111
  t2:
    network: polygon
    address: 0x2222222222222222222222222222222222222222
"#,
            spec_version = SpecVersion::current()
        );
        let reparsed = DotrainOrder::create(generated, Some(vec![tokens_settings]))
            .await
            .expect("generated dotrain should parse");
        let placeholder = url::Url::parse(STRIPPED_URL_PLACEHOLDER).unwrap();
        let raindex_yaml = reparsed.raindex_yaml();
        assert_eq!(
            raindex_yaml.get_network("polygon").unwrap().rpcs,
            vec![placeholder.clone(), placeholder.clone()]
        );
        assert_eq!(
            raindex_yaml.get_subgraph("sg-primary").unwrap().url,
            placeholder
        );
        assert_eq!(
            raindex_yaml.get_metaboard("polygon").unwrap().url,
            placeholder
        );
        assert_eq!(
            reparsed
                .dotrain_yaml()
                .get_order("polygon-order")
                .unwrap()
                .oracle_url
                .as_deref(),
            Some(oracle_url)
        );
    }

    #[tokio::test]
    async fn test_rainlang_post_from_scenario() {
        let server = mock_server(vec![]);
        let dotrain = format!(
            r#"
version: {spec_version}
networks:
    polygon:
        rpcs:
            - {rpc_url}
        chain-id: 137
        network-id: 137
        currency: MATIC
rainlangs:
    polygon:
        address: 0x1234567890123456789012345678901234567890
scenarios:
    polygon:
        rainlang: polygon
        bindings:
            key1: 10
---
#key1 !Test binding
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
_ _: 1 2;
"#,
            rpc_url = server.url("/rpc"),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let rainlang = dotrain_order
            .compose_scenario_to_post_task_rainlang("polygon".to_string())
            .await
            .unwrap();

        assert_eq!(
            normalize_rainlang(&rainlang),
            normalize_rainlang(
                r#"/* 0. handle-add-order */
_ _: 1 2;"#
            )
        );
    }

    #[tokio::test]
    async fn test_config_merge() {
        let server = mock_server(vec![]);
        let dotrain = format!(
            r#"
version: {spec_version}
networks:
  polygon:
    rpcs:
      - {rpc_url}
    chain-id: 137
    network-id: 137
    currency: MATIC
---
#calculate-io
_ _: 00;

#handle-io
:;"#,
            rpc_url = server.url("/rpc-polygon"),
            spec_version = SpecVersion::current()
        );

        let settings = format!(
            r#"
version: {spec_version}
networks:
    mainnet:
        rpcs:
            - {rpc_url}
        chain-id: 1
        network-id: 1
        currency: ETH"#,
            rpc_url = server.url("/rpc-mainnet"),
            spec_version = SpecVersion::current(),
        );

        let dotrain_order =
            DotrainOrder::create(dotrain.to_string(), Some(vec![settings.to_string()]))
                .await
                .unwrap();

        assert_eq!(
            dotrain_order
                .raindex_yaml()
                .get_network("mainnet")
                .unwrap()
                .rpcs
                .iter()
                .map(|rpc| rpc.to_string())
                .collect::<Vec<String>>()
                .first()
                .unwrap()
                .to_string(),
            server.url("/rpc-mainnet")
        );
    }

    #[tokio::test]
    async fn test_get_pragmas_for_scenario() {
        let pragma_addresses = vec![Address::random()];
        let server = mock_server(pragma_addresses.clone());
        let dotrain = format!(
            r#"
version: {spec_version}
networks:
    sepolia:
        rpcs:
            - {rpc_url}
        chain-id: 0
rainlangs:
    sepolia:
        address: 0x017F5651eB8fa4048BBc17433149c6c035d391A6
scenarios:
    sepolia:
        rainlang: sepolia
        bindings:
            key1: 10
---
#key1 !Test binding
#calculate-io
using-words-from 0xb06202aA3Fe7d85171fB7aA5f17011d17E63f382
_: order-hash(),
_ _: 0 0;
#handle-io
:;"#,
            rpc_url = server.url("/rpc"),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let pragmas = dotrain_order
            .get_pragmas_for_scenario("sepolia")
            .await
            .unwrap();

        assert_eq!(pragmas, pragma_addresses);
    }

    #[tokio::test]
    async fn test_get_contract_authoring_meta_v2_for_scenario() {
        let pragma_addresses = vec![Address::random()];
        let server = mock_server(pragma_addresses.clone());
        let dotrain = format!(
            r#"
    version: {spec_version}
    networks:
        sepolia:
            rpcs:
                - {rpc_url}
            chain-id: 0
    rainlangs:
        sepolia:
            address: 0x3131baC3E2Ec97b0ee93C74B16180b1e93FABd59
    scenarios:
        sepolia:
            rainlang: sepolia
            bindings:
                key1: 10
    metaboards:
        sepolia: {metaboard_url}
    ---
    #key1 !Test binding
    #calculate-io
    using-words-from 0xbc609623F5020f6Fc7481024862cD5EE3FFf52D7
    _: order-hash(),
    _ _: 0 0;
    #handle-io
    :;"#,
            rpc_url = server.url("/rpc"),
            metaboard_url = server.url("/sg"),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let result = dotrain_order
            .get_contract_authoring_meta_v2_for_scenario("sepolia", pragma_addresses[0])
            .await
            .unwrap();

        assert_eq!(&result.words[0].word, "some-word");
        assert_eq!(&result.words[0].description, "some-desc");

        assert_eq!(&result.words[1].word, "some-other-word");
        assert_eq!(&result.words[1].description, "some-other-desc");
    }

    #[tokio::test]
    async fn test_get_pragma_words_for_scenario() {
        let pragma_addresses = vec![Address::random()];
        let server = mock_server(pragma_addresses.clone());
        let dotrain = format!(
            r#"
    version: {spec_version}
    networks:
        sepolia:
            rpcs:
                - {rpc_url}
            chain-id: 0
    rainlangs:
        sepolia:
            address: 0x3131baC3E2Ec97b0ee93C74B16180b1e93FABd59
    scenarios:
        sepolia:
            rainlang: sepolia
            bindings:
                key1: 10
    metaboards:
        sepolia: {metaboard_url}
    ---
    #key1 !Test binding
    #calculate-io
    using-words-from 0xbc609623F5020f6Fc7481024862cD5EE3FFf52D7
    _: order-hash(),
    _ _: 0 0;
    #handle-io
    :;"#,
            rpc_url = server.url("/rpc"),
            metaboard_url = server.url("/sg"),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let result = dotrain_order
            .get_pragma_words_for_scenario("sepolia")
            .await
            .unwrap();

        assert!(result.len() == 1);
        assert_eq!(result[0].address, pragma_addresses[0]);
        assert!(matches!(result[0].words, WordsResult::Success(_)));
        if let WordsResult::Success(authoring_meta) = &result[0].words {
            assert_eq!(&authoring_meta.words[0].word, "some-word");
            assert_eq!(&authoring_meta.words[0].description, "some-desc");

            assert_eq!(&authoring_meta.words[1].word, "some-other-word");
            assert_eq!(&authoring_meta.words[1].description, "some-other-desc");
        }
    }

    #[tokio::test]
    async fn test_get_rainlang_words_for_scenario() {
        let server = mock_server(vec![]);
        let rainlang_addr = Address::random();
        let dotrain = format!(
            r#"
    version: {spec_version}
    networks:
        sepolia:
            rpcs:
                - {rpc_url}
            chain-id: 0
    rainlangs:
        sepolia:
            address: {rainlang_address}
    scenarios:
        sepolia:
            rainlang: sepolia
            bindings:
                key1: 10
    metaboards:
        sepolia: {metaboard_url}
    ---
    #calculate-io
    using-words-from 0xbc609623F5020f6Fc7481024862cD5EE3FFf52D7
    _: order-hash(),
    _ _: 0 0;
    #handle-io
    :;"#,
            rpc_url = server.url("/rpc"),
            metaboard_url = server.url("/sg"),
            rainlang_address = encode_prefixed(rainlang_addr),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let result = dotrain_order
            .get_rainlang_words_for_scenario("sepolia")
            .await
            .unwrap();

        assert_eq!(result.address, rainlang_addr);
        assert!(matches!(result.words, WordsResult::Success(_)));
        if let WordsResult::Success(authoring_meta) = &result.words {
            assert_eq!(&authoring_meta.words[0].word, "some-word");
            assert_eq!(&authoring_meta.words[0].description, "some-desc");

            assert_eq!(&authoring_meta.words[1].word, "some-other-word");
            assert_eq!(&authoring_meta.words[1].description, "some-other-desc");
        }
    }

    #[tokio::test]
    async fn test_get_all_words_for_scenario() {
        let rainlang_addr = Address::random();
        let pragma_addresses = vec![Address::random()];
        let server = mock_server(pragma_addresses.clone());
        let dotrain = format!(
            r#"
    version: {spec_version}
    networks:
        sepolia:
            rpcs:
                - {rpc_url}
            chain-id: 0
    rainlangs:
        sepolia:
            address: {rainlang_address}
    scenarios:
        sepolia:
            rainlang: sepolia
            bindings:
                key1: 10
    metaboards:
        sepolia: {metaboard_url}
    ---
    #key1 !Test binding
    #calculate-io
    using-words-from 0xbc609623F5020f6Fc7481024862cD5EE3FFf52D7
    _: order-hash(),
    _ _: 0 0;
    #handle-io
    :;"#,
            rpc_url = server.url("/rpc"),
            metaboard_url = server.url("/sg"),
            rainlang_address = encode_prefixed(rainlang_addr),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let result = dotrain_order
            .get_all_words_for_scenario("sepolia")
            .await
            .unwrap();

        assert_eq!(&result.scenario, "sepolia");

        assert_eq!(result.rainlang_words.address, rainlang_addr);
        assert!(matches!(
            result.rainlang_words.words,
            WordsResult::Success(_)
        ));
        if let WordsResult::Success(authoring_meta) = &result.rainlang_words.words {
            assert_eq!(&authoring_meta.words[0].word, "some-word");
            assert_eq!(&authoring_meta.words[0].description, "some-desc");

            assert_eq!(&authoring_meta.words[1].word, "some-other-word");
            assert_eq!(&authoring_meta.words[1].description, "some-other-desc");
        }

        assert!(result.pragma_words.len() == 1);
        assert_eq!(result.pragma_words[0].address, pragma_addresses[0]);
        assert!(matches!(
            result.pragma_words[0].words,
            WordsResult::Success(_)
        ));
        if let WordsResult::Success(authoring_meta) = &result.pragma_words[0].words {
            assert_eq!(&authoring_meta.words[0].word, "some-word");
            assert_eq!(&authoring_meta.words[0].description, "some-desc");

            assert_eq!(&authoring_meta.words[1].word, "some-other-word");
            assert_eq!(&authoring_meta.words[1].description, "some-other-desc");
        }
    }

    #[tokio::test]
    async fn test_get_all_scenarios_all_words() {
        let rainlang_addr = Address::random();
        let pragma_addresses = vec![Address::random()];
        let server = mock_server(pragma_addresses.clone());
        let dotrain = format!(
            r#"
    version: {spec_version}
    networks:
        sepolia:
            rpcs:
                - {rpc_url}
            chain-id: 0
    rainlangs:
        sepolia:
            address: {rainlang_address}
    scenarios:
        sepolia:
            rainlang: sepolia
            bindings:
                key1: 10
        other-scenario:
            rainlang: sepolia
            bindings:
                key1: 40
    metaboards:
        sepolia: {metaboard_url}
    ---
    #key1 !Test binding
    #calculate-io
    using-words-from 0xbc609623F5020f6Fc7481024862cD5EE3FFf52D7
    _: order-hash(),
    _ _: 0 0;
    #handle-io
    :;"#,
            rpc_url = server.url("/rpc"),
            metaboard_url = server.url("/sg"),
            rainlang_address = encode_prefixed(rainlang_addr),
            spec_version = SpecVersion::current()
        );
        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let results = dotrain_order.get_all_scenarios_all_words().await.unwrap();

        assert_eq!(results.len(), 2);

        let other_scenario_result = results
            .iter()
            .find(|r| r.scenario == "other-scenario")
            .expect("Did not find results for 'other-scenario'");

        assert_eq!(other_scenario_result.rainlang_words.address, rainlang_addr);
        assert!(matches!(
            other_scenario_result.rainlang_words.words,
            WordsResult::Success(_)
        ));
        if let WordsResult::Success(authoring_meta) = &other_scenario_result.rainlang_words.words {
            assert_eq!(&authoring_meta.words[0].word, "some-word");
            assert_eq!(&authoring_meta.words[0].description, "some-desc");

            assert_eq!(&authoring_meta.words[1].word, "some-other-word");
            assert_eq!(&authoring_meta.words[1].description, "some-other-desc");
        }
        assert!(other_scenario_result.pragma_words.len() == 1);
        assert_eq!(
            other_scenario_result.pragma_words[0].address,
            pragma_addresses[0]
        );
        assert!(matches!(
            other_scenario_result.pragma_words[0].words,
            WordsResult::Success(_)
        ));
        if let WordsResult::Success(authoring_meta) = &other_scenario_result.pragma_words[0].words {
            assert_eq!(&authoring_meta.words[0].word, "some-word");
            assert_eq!(&authoring_meta.words[0].description, "some-desc");

            assert_eq!(&authoring_meta.words[1].word, "some-other-word");
            assert_eq!(&authoring_meta.words[1].description, "some-other-desc");
        }

        let sepolia_result = results
            .iter()
            .find(|r| r.scenario == "sepolia")
            .expect("Did not find results for 'sepolia'");

        assert_eq!(sepolia_result.rainlang_words.address, rainlang_addr);
        assert!(matches!(
            sepolia_result.rainlang_words.words,
            WordsResult::Success(_)
        ));
        if let WordsResult::Success(authoring_meta) = &sepolia_result.rainlang_words.words {
            assert_eq!(&authoring_meta.words[0].word, "some-word");
            assert_eq!(&authoring_meta.words[0].description, "some-desc");

            assert_eq!(&authoring_meta.words[1].word, "some-other-word");
            assert_eq!(&authoring_meta.words[1].description, "some-other-desc");
        }
        assert!(sepolia_result.pragma_words.len() == 1);
        assert_eq!(sepolia_result.pragma_words[0].address, pragma_addresses[0]);
        assert!(matches!(
            sepolia_result.pragma_words[0].words,
            WordsResult::Success(_)
        ));
        if let WordsResult::Success(authoring_meta) = &sepolia_result.pragma_words[0].words {
            assert_eq!(&authoring_meta.words[0].word, "some-word");
            assert_eq!(&authoring_meta.words[0].description, "some-desc");

            assert_eq!(&authoring_meta.words[1].word, "some-other-word");
            assert_eq!(&authoring_meta.words[1].description, "some-other-desc");
        }
    }

    // helper function to mock rpc and sg response
    fn mock_server(with_pragma_addresses: Vec<Address>) -> MockServer {
        let server = MockServer::start();
        // mock contract calls
        server.mock(|when, then| {
            when.path("/rpc").body_contains("0x01ffc9a7ffffffff");
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": B256::left_padding_from(&[0]).to_string()
            }));
        });
        server.mock(|when, then| {
            when.path("/rpc").body_contains("0x01ffc9a7");
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": B256::left_padding_from(&[1]).to_string()
            }));
        });
        server.mock(|when, then| {
            when.path("/rpc").body_contains("0x6f5aa28d");
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": B256::random().to_string()
            }));
        });
        server.mock(|when, then| {
            when.path("/rpc").body_contains("0x5514ca20");
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_prefixed(PragmaV1 {
                    usingWordsFrom: with_pragma_addresses,
                }.abi_encode())
            }));
        });

        // mock sg query
        server.mock(|when, then| {
            when.path("/sg");
            then.status(200).json_body_obj(&json!({
                "data": {
                    "metaV1S": [{
                        "meta": encode_prefixed(
                            RainMetaDocumentV1Item {
                                payload: ByteBuf::from(
                                    vec![
                                        AuthoringMetaV2Sol {
                                            word: B256::right_padding_from("some-word".as_bytes()),
                                            description: "some-desc".to_string(),
                                        },
                                        AuthoringMetaV2Sol {
                                            word: B256::right_padding_from("some-other-word".as_bytes()),
                                            description: "some-other-desc".to_string(),
                                        }
                                    ]
                                    .abi_encode(),
                                ),
                                magic: KnownMagic::AuthoringMetaV2,
                                content_type: rain_metadata::ContentType::OctetStream,
                                content_encoding: rain_metadata::ContentEncoding::None,
                                content_language: rain_metadata::ContentLanguage::None,
                            }
                            .cbor_encode()
                            .unwrap()
                        ),
                        "metaHash": "0x00",
                        "sender": "0x00",
                        "id": "0x00",
                        "metaBoard": {
                            "id": "0x00",
                            "metas": [],
                            "address": "0x00",
                        },
                        "subject": "0x00",
                    }]
                }
            }));
        });
        server
    }

    #[tokio::test]
    async fn test_validate_spec_version_happy() {
        let dotrain = format!(
            "
                version: {spec_version}
                networks:
                    sepolia:
                        rpcs:
                            - http://example.com
                        chain-id: 0
                rainlangs:
                    sepolia:
                        address: 0x3131baC3E2Ec97b0ee93C74B16180b1e93FABd59
                ---
                #calculate-io
                _ _: 0 0;
                #handle-io
                :;",
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        dotrain_order.validate_spec_version().await.unwrap();
    }

    #[tokio::test]
    async fn test_validate_missing_spec_version() {
        let dotrain = "
                networks:
                    sepolia:
                        rpcs:
                            - http://example.com
                        chain-id: 0
                rainlangs:
                    sepolia:
                        address: 0x3131baC3E2Ec97b0ee93C74B16180b1e93FABd59
                ---
                #calculate-io
                _ _: 0 0;
                #handle-io
                :;";

        let err = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            DotrainOrderError::YamlError(YamlError::Field {
                kind: FieldErrorKind::Missing(ref key),
                location
            }) if key == "version" && location == "root"
        ));
    }

    #[tokio::test]
    async fn test_validate_spec_version_unhappy() {
        let dotrain = "
                version: 2
                networks:
                    sepolia:
                        rpc: http://example.com
                        chain-id: 0
                rainlangs:
                    sepolia:
                        address: 0x3131baC3E2Ec97b0ee93C74B16180b1e93FABd59
                ---
                #calculate-io
                _ _: 0 0;
                #handle-io
                :;";

        let err = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap_err();

        assert!(
            matches!(err, DotrainOrderError::YamlError(YamlError::Field {
            kind: FieldErrorKind::InvalidValue { field, .. },
            location,
        }) if field == "version" && location == "root")
        );
    }

    #[tokio::test]
    async fn test_rainlang_from_deployment() {
        let server = mock_server(vec![]);
        let dotrain = format!(
            r#"
version: {spec_version}
networks:
    polygon:
        rpcs:
            - {rpc_url}
        chain-id: 137
        network-id: 137
        currency: MATIC
rainlangs:
    polygon:
        address: 0x1234567890123456789012345678901234567890
scenarios:
    polygon:
        rainlang: polygon
        bindings:
            key1: 10
tokens:
    t1:
        network: polygon
        address: 0x1111111111111111111111111111111111111111
        decimals: 18
        label: Token1
        symbol: Token1
    t2:
        network: polygon
        address: 0x2222222222222222222222222222222222222222
        decimals: 18
        label: Token2
        symbol: token2
orders:
    polygon:
        inputs:
            - token: t1
        outputs:
            - token: t2
deployments:
    polygon:
        scenario: polygon
        order: polygon
---
#key1 !Test binding
#calculate-io
_ _: 0 0;
#handle-io
:;"#,
            rpc_url = server.url("/rpc"),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.to_string(), None)
            .await
            .unwrap();

        let rainlang = dotrain_order
            .compose_deployment_to_rainlang("polygon".to_string())
            .await
            .unwrap();

        assert_eq!(
            normalize_rainlang(&rainlang),
            normalize_rainlang(
                r#"/* 0. calculate-io */
_ _: 0 0;

/* 1. handle-io */
:;"#
            )
        );
    }
}
