use super::*;
use crate::{
    add_order::AddOrderArgs, allowance::read_allowance, deposit::DepositArgs,
    transaction::TransactionArgs,
};
use alloy::{
    primitives::{Bytes, U256},
    sol_types::SolCall,
};
use rain_math_float::Float;
use rain_metaboard_subgraph::metaboard_client::{
    MetaboardSubgraphClient, MetaboardSubgraphClientError,
};
use rain_metaboard_subgraph::types::metas::Bytes as MetaBytes;
use rain_metadata::RainMetaDocumentV1Item;
use raindex_app_settings::{
    order::{OrderIOCfg, VaultType},
    raindex::RaindexCfg,
};
use raindex_bindings::{IRaindexV7::deposit5Call, Raindex::multicallCall, IERC20::approveCall};
use std::{collections::HashMap, str::FromStr, sync::Arc};
use url::Url;

pub enum CalldataFunction {
    Allowance,
    Deposit,
    AddOrder,
    DepositAndAddOrder,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenAllowance {
    pub token: Address,
    pub allowance: U256,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AllowancesResult(pub Vec<TokenAllowance>);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ApprovalCalldataResult {
    NoDeposits,
    Calldatas(Vec<ApprovalCalldata>),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum DepositCalldataResult {
    NoDeposits,
    Calldatas(Vec<Bytes>),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AddOrderCalldataResult(pub Bytes);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DepositAndAddOrderCalldataResult(pub Bytes);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct IOVaultIds(pub HashMap<String, HashMap<String, Option<U256>>>);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WithdrawCalldataResult(pub Vec<Bytes>);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ExtendedApprovalCalldata {
    pub token: Address,
    pub calldata: Bytes,
    pub symbol: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalCall {
    pub to: Address,
    pub calldata: Bytes,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentTransactionArgs {
    pub approvals: Vec<ExtendedApprovalCalldata>,
    pub deployment_calldata: Bytes,
    pub raindex_address: Address,
    pub chain_id: u32,
    pub emit_meta_call: Option<ExternalCall>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ApprovalCalldata {
    pub token: Address,
    pub calldata: Bytes,
}

#[derive(Debug)]
pub struct VaultAndDeposit {
    pub order_io: OrderIOCfg,
    pub deposit_amount: Float,
    pub index: usize,
}

impl RaindexOrderBuilder {
    fn get_raindex(&self) -> Result<Arc<RaindexCfg>, RaindexOrderBuilderError> {
        let deployment = self.get_current_deployment()?;
        deployment
            .deployment
            .as_ref()
            .order
            .as_ref()
            .raindex
            .as_ref()
            .ok_or(RaindexOrderBuilderError::RaindexNotFound)
            .cloned()
    }

    fn get_transaction_args(&self) -> Result<TransactionArgs, RaindexOrderBuilderError> {
        let raindex = self.get_raindex()?;
        Ok(TransactionArgs {
            raindex_address: raindex.address,
            rpcs: raindex
                .network
                .rpcs
                .clone()
                .into_iter()
                .map(|url| url.to_string())
                .collect(),
            ..Default::default()
        })
    }

    async fn get_deposits_as_map(
        &self,
    ) -> Result<HashMap<Address, Float>, RaindexOrderBuilderError> {
        let mut map: HashMap<Address, Float> = HashMap::new();
        for d in self.get_deposits()? {
            let token_info = self.get_token_info(d.token.clone()).await?;
            let amount = Float::parse(d.amount)?;
            map.insert(token_info.address, amount);
        }
        Ok(map)
    }

    async fn get_vaults_and_deposits(
        &self,
        deployment: &OrderBuilderDeploymentCfg,
    ) -> Result<Vec<VaultAndDeposit>, RaindexOrderBuilderError> {
        let deposits_map = self.get_deposits_as_map().await?;
        let results = deployment
            .deployment
            .order
            .outputs
            .clone()
            .into_iter()
            .enumerate()
            .filter_map(|(index, output)| {
                output.token.as_ref().and_then(|token| {
                    deposits_map.get(&token.address).map(|amount| {
                        Ok(VaultAndDeposit {
                            order_io: output.clone(),
                            deposit_amount: *amount,
                            index,
                        })
                    })
                })
            })
            .collect::<Result<Vec<_>, RaindexOrderBuilderError>>()?;
        Ok(results)
    }

    pub fn prepare_calldata_generation(
        &mut self,
        calldata_function: CalldataFunction,
    ) -> Result<OrderBuilderDeploymentCfg, RaindexOrderBuilderError> {
        let deployment = self.get_current_deployment()?;
        self.check_select_tokens()?;
        match calldata_function {
            CalldataFunction::Deposit => {
                self.populate_vault_ids(&deployment)?;
            }
            CalldataFunction::AddOrder | CalldataFunction::DepositAndAddOrder => {
                self.check_field_values()?;
                self.populate_vault_ids(&deployment)?;
                self.update_bindings(&deployment)?;
            }
            _ => {}
        }
        self.get_current_deployment()
    }

    pub async fn check_allowances(
        &mut self,
        owner: String,
    ) -> Result<AllowancesResult, RaindexOrderBuilderError> {
        let deployment = self.prepare_calldata_generation(CalldataFunction::Allowance)?;

        let vaults_and_deposits = self.get_vaults_and_deposits(&deployment).await?;

        let owner = Address::from_str(&owner)?;

        let mut results = Vec::new();
        for VaultAndDeposit {
            order_io,
            deposit_amount: _,
            index: _,
        } in vaults_and_deposits
        {
            let tx_args = self.get_transaction_args()?;
            let rpcs = tx_args
                .rpcs
                .iter()
                .map(|rpc| Url::parse(rpc))
                .collect::<Result<Vec<_>, _>>()?;

            let token = order_io
                .token
                .as_ref()
                .ok_or(RaindexOrderBuilderError::SelectTokensNotSet)?
                .address;

            let erc20 = ERC20::new(rpcs, token);
            let allowance = erc20.allowance(owner, tx_args.raindex_address).await?;

            results.push(TokenAllowance { token, allowance });
        }

        Ok(AllowancesResult(results))
    }

    pub async fn generate_approval_calldatas(
        &mut self,
        owner: String,
    ) -> Result<ApprovalCalldataResult, RaindexOrderBuilderError> {
        let deposits_map = self.get_deposits_as_map().await?;
        if deposits_map.is_empty() {
            return Ok(ApprovalCalldataResult::NoDeposits);
        }

        let owner = Address::from_str(&owner)?;

        let mut calldatas = Vec::new();

        for (token_address, deposit_amount) in &deposits_map {
            let tx_args = self.get_transaction_args()?;
            let rpcs = tx_args
                .rpcs
                .iter()
                .map(|rpc| Url::parse(rpc))
                .collect::<Result<Vec<_>, _>>()?;

            let erc20 = ERC20::new(rpcs, *token_address);
            let decimals = erc20.decimals().await?;

            // An allowance read needs only the token, owner and raindex spender -
            // no deposit context.
            let allowance = read_allowance(
                &tx_args.rpcs,
                *token_address,
                owner,
                tx_args.raindex_address,
            )
            .await?;
            let allowance_float = Float::from_fixed_decimal(allowance, decimals)?;

            if !allowance_float.eq(*deposit_amount)? {
                let calldata = approveCall {
                    spender: tx_args.raindex_address,
                    amount: deposit_amount.to_fixed_decimal(decimals)?,
                }
                .abi_encode();

                calldatas.push(ApprovalCalldata {
                    token: *token_address,
                    calldata: Bytes::copy_from_slice(&calldata),
                });
            }
        }

        Ok(ApprovalCalldataResult::Calldatas(calldatas))
    }

    fn populate_vault_ids(
        &mut self,
        deployment: &OrderBuilderDeploymentCfg,
    ) -> Result<(), RaindexOrderBuilderError> {
        self.dotrain_order
            .dotrain_yaml()
            .get_order(&deployment.deployment.order.key)?
            .populate_vault_ids()?;
        Ok(())
    }

    fn update_bindings(
        &mut self,
        deployment: &OrderBuilderDeploymentCfg,
    ) -> Result<(), RaindexOrderBuilderError> {
        self.dotrain_order
            .dotrain_yaml()
            .get_scenario(&deployment.deployment.scenario.key)?
            .update_bindings(
                self.field_values
                    .keys()
                    .map(|k| Ok((k.clone(), self.get_field_value(k.clone())?.value.clone())))
                    .collect::<Result<HashMap<String, String>, RaindexOrderBuilderError>>()?,
            )?;
        Ok(())
    }

    async fn prepare_add_order_args(
        &mut self,
        deployment: &OrderBuilderDeploymentCfg,
    ) -> Result<AddOrderArgs, RaindexOrderBuilderError> {
        let dotrain_builder_state_instance_v1 =
            self.generate_dotrain_builder_state_instance_v1()?;
        let dotrain_builder_state_meta =
            RainMetaDocumentV1Item::try_from(dotrain_builder_state_instance_v1)?;

        let dotrain_for_deployment = self
            .dotrain_order
            .generate_dotrain_for_deployment(&deployment.deployment.key)?;

        let add_order_args = AddOrderArgs::new_from_deployment(
            dotrain_for_deployment,
            deployment.deployment.as_ref().clone(),
            Some(vec![dotrain_builder_state_meta]),
        )
        .await?;

        Ok(add_order_args)
    }

    pub async fn generate_deposit_calldatas(
        &mut self,
    ) -> Result<DepositCalldataResult, RaindexOrderBuilderError> {
        let deployment = self.prepare_calldata_generation(CalldataFunction::Deposit)?;

        let vaults_and_deposits = self.get_vaults_and_deposits(&deployment).await?;
        if vaults_and_deposits.is_empty() {
            return Ok(DepositCalldataResult::NoDeposits);
        }

        let mut calldatas = Vec::new();
        for VaultAndDeposit {
            order_io,
            deposit_amount,
            index,
        } in vaults_and_deposits
        {
            if deposit_amount.eq(Float::parse("0".to_string())?)? {
                continue;
            }

            let token = order_io
                .token
                .as_ref()
                .ok_or(RaindexOrderBuilderError::SelectTokensNotSet)?;
            let vault_id = order_io
                .vault_id
                .ok_or(RaindexOrderBuilderError::VaultIdNotFound(index.to_string()))?;

            let decimals = if let Some(decimals) = token.decimals {
                decimals
            } else {
                let tx_args = self.get_transaction_args()?;
                let rpcs = tx_args
                    .rpcs
                    .iter()
                    .map(|rpc| Url::parse(rpc))
                    .collect::<Result<Vec<_>, _>>()?;
                let erc20 = ERC20::new(rpcs, token.address);
                erc20.decimals().await?
            };

            let deposit_args = DepositArgs {
                token: token.address,
                amount: deposit_amount,
                vault_id: vault_id.into(),
                decimals,
            };
            let calldata = deposit5Call::try_from(deposit_args)
                .map_err(crate::deposit::DepositError::from)?
                .abi_encode();
            calldatas.push(Bytes::copy_from_slice(&calldata));
        }

        Ok(DepositCalldataResult::Calldatas(calldatas))
    }

    pub async fn generate_add_order_calldata(
        &mut self,
    ) -> Result<AddOrderCalldataResult, RaindexOrderBuilderError> {
        let deployment = self.prepare_calldata_generation(CalldataFunction::AddOrder)?;

        let add_order_args = self.prepare_add_order_args(&deployment).await?;
        let transaction_args = self.get_transaction_args()?;

        let add_order_call = add_order_args
            .try_into_call(transaction_args.rpcs.clone())
            .await?;

        Ok(AddOrderCalldataResult(Bytes::copy_from_slice(
            &add_order_call.abi_encode(),
        )))
    }

    pub async fn generate_deposit_and_add_order_calldatas(
        &mut self,
    ) -> Result<DepositAndAddOrderCalldataResult, RaindexOrderBuilderError> {
        let deployment = self.prepare_calldata_generation(CalldataFunction::DepositAndAddOrder)?;

        let mut calls = Vec::new();

        let deposit_calldatas = self.generate_deposit_calldatas().await?;

        let deposit_calldatas = match deposit_calldatas {
            DepositCalldataResult::Calldatas(calldatas) => calldatas,
            DepositCalldataResult::NoDeposits => Vec::new(),
        };

        let add_order_args = self.prepare_add_order_args(&deployment).await?;
        let transaction_args = self.get_transaction_args()?;

        let add_order_call = add_order_args
            .try_into_call(transaction_args.rpcs.clone())
            .await?;
        let add_order_calldata = Bytes::copy_from_slice(&add_order_call.abi_encode());

        calls.push(Bytes::copy_from_slice(&add_order_calldata.0));

        for calldata in deposit_calldatas.iter() {
            calls.push(Bytes::copy_from_slice(calldata));
        }

        Ok(DepositAndAddOrderCalldataResult(Bytes::copy_from_slice(
            &multicallCall { data: calls }.abi_encode(),
        )))
    }

    pub fn set_vault_id(
        &mut self,
        r#type: VaultType,
        token: String,
        vault_id: Option<String>,
    ) -> Result<(), RaindexOrderBuilderError> {
        let deployment = self.get_current_deployment()?;
        self.dotrain_order
            .dotrain_yaml()
            .get_order(&deployment.deployment.order.key)?
            .update_vault_id(r#type, token, vault_id)?;

        Ok(())
    }

    pub fn get_vault_ids(&self) -> Result<IOVaultIds, RaindexOrderBuilderError> {
        let deployment = self.get_current_deployment()?;

        let mut input_map = HashMap::new();
        for input in deployment.deployment.order.inputs.iter() {
            let token_key = input
                .token
                .as_ref()
                .map(|t| t.key.clone())
                .ok_or(RaindexOrderBuilderError::SelectTokensNotSet)?;
            input_map.insert(token_key, input.vault_id);
        }

        let mut output_map = HashMap::new();
        for output in deployment.deployment.order.outputs.iter() {
            let token_key = output
                .token
                .as_ref()
                .map(|t| t.key.clone())
                .ok_or(RaindexOrderBuilderError::SelectTokensNotSet)?;
            output_map.insert(token_key, output.vault_id);
        }

        let map = HashMap::from([
            ("input".to_string(), input_map),
            ("output".to_string(), output_map),
        ]);
        Ok(IOVaultIds(map))
    }

    pub fn has_any_vault_id(&self) -> Result<bool, RaindexOrderBuilderError> {
        let map = self.get_vault_ids()?;
        Ok(map
            .0
            .values()
            .any(|token_map| token_map.values().any(|vault_id| vault_id.is_some())))
    }

    pub fn update_scenario_bindings(&mut self) -> Result<(), RaindexOrderBuilderError> {
        let deployment = self.get_current_deployment()?;
        self.update_bindings(&deployment)?;
        Ok(())
    }

    pub async fn get_deployment_transaction_args(
        &mut self,
        owner: String,
    ) -> Result<DeploymentTransactionArgs, RaindexOrderBuilderError> {
        let deployment = self.prepare_calldata_generation(CalldataFunction::DepositAndAddOrder)?;

        let mut approvals = Vec::new();
        let approval_calldata = self.generate_approval_calldatas(owner).await?;
        if let ApprovalCalldataResult::Calldatas(calldatas) = approval_calldata {
            let mut output_token_infos = HashMap::new();
            for output in deployment.deployment.order.outputs.clone() {
                if output.token.is_none() {
                    return Err(RaindexOrderBuilderError::SelectTokensNotSet);
                }
                let token = output.token.as_ref().unwrap();
                let token_info = self.get_token_info(token.key.clone()).await?;
                output_token_infos.insert(token.address, token_info);
            }

            for calldata in calldatas.iter() {
                let token_info = output_token_infos.get(&calldata.token).ok_or(
                    RaindexOrderBuilderError::TokenNotFound(calldata.token.to_string()),
                )?;
                approvals.push(ExtendedApprovalCalldata {
                    token: calldata.token,
                    calldata: calldata.calldata.clone(),
                    symbol: token_info.symbol.clone(),
                });
            }
        }

        let deposit_calldata_result = self.generate_deposit_calldatas().await?;
        let deposit_calldatas = match deposit_calldata_result {
            DepositCalldataResult::Calldatas(calldatas) => calldatas,
            DepositCalldataResult::NoDeposits => Vec::new(),
        };

        let add_order_args = self.prepare_add_order_args(&deployment).await?;

        let transaction_args = self.get_transaction_args()?;
        let add_order_call = add_order_args
            .try_into_call(transaction_args.rpcs.clone())
            .await?;

        let mut calls = Vec::new();
        calls.push(Bytes::copy_from_slice(&add_order_call.abi_encode()));
        for calldata in deposit_calldatas.iter() {
            calls.push(Bytes::copy_from_slice(calldata));
        }

        let deployment_calldata =
            Bytes::copy_from_slice(&multicallCall { data: calls }.abi_encode());

        let emit_meta_call = if self.should_emit_meta_call().await? {
            let client = self.get_metaboard_client()?;
            let addresses = client.get_metaboard_addresses(None, None).await?;
            let metaboard_address = addresses
                .first()
                .ok_or_else(|| RaindexOrderBuilderError::NoAddressInMetaboardSubgraph)?;

            let calldata = add_order_args.try_into_emit_meta_call()?;
            calldata.map(|calldata| ExternalCall {
                to: *metaboard_address,
                calldata: Bytes::copy_from_slice(&calldata.abi_encode()),
            })
        } else {
            None
        };

        Ok(DeploymentTransactionArgs {
            approvals,
            deployment_calldata,
            raindex_address: deployment
                .deployment
                .order
                .raindex
                .as_ref()
                .ok_or(RaindexOrderBuilderError::RaindexNotFound)?
                .address,
            chain_id: deployment.deployment.order.network.chain_id,
            emit_meta_call,
        })
    }

    fn get_metaboard_client(&self) -> Result<MetaboardSubgraphClient, RaindexOrderBuilderError> {
        let deployment = self.get_current_deployment()?;
        let raindex_yaml = self.dotrain_order.raindex_yaml();
        let metaboard_cfg = raindex_yaml.get_metaboard(&deployment.deployment.order.network.key)?;
        Ok(MetaboardSubgraphClient::new(metaboard_cfg.url.clone()))
    }

    async fn should_emit_meta_call(&self) -> Result<bool, RaindexOrderBuilderError> {
        let dotrain_builder_state = self.generate_dotrain_builder_state_instance_v1()?;
        let subject = dotrain_builder_state.dotrain_hash();

        let client = self.get_metaboard_client()?;
        match client
            .get_metabytes_by_subject(&MetaBytes(format!("0x{}", alloy::hex::encode(subject))))
            .await
        {
            Ok(metas) => Ok(metas.is_empty()),
            Err(MetaboardSubgraphClientError::Empty(_)) => Ok(true),
            Err(err) => Err(RaindexOrderBuilderError::MetaboardSubgraphClientError(err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raindex_order_builder::tests::{
        initialize_builder, initialize_builder_with_select_tokens,
    };
    use rain_metadata::{types::dotrain::source_v1::DotrainSourceV1, RainMetaDocumentV1Item};

    #[tokio::test]
    async fn test_generate_deposit_calldatas() {
        let mut builder = initialize_builder(Some("other-deployment".to_string())).await;

        let res = builder.generate_deposit_calldatas().await.unwrap();
        match res {
            DepositCalldataResult::Calldatas(_) => {
                panic!("should not be calldatas");
            }
            DepositCalldataResult::NoDeposits => {}
        }

        builder
            .set_deposit("token1".to_string(), "1200".to_string())
            .await
            .unwrap();

        let res = builder.generate_deposit_calldatas().await.unwrap();
        match res {
            DepositCalldataResult::Calldatas(calldatas) => {
                assert_eq!(calldatas.len(), 1);
                assert_eq!(calldatas[0].len(), 164);
            }
            DepositCalldataResult::NoDeposits => {
                panic!("should not be no deposits");
            }
        }

        builder
            .set_deposit("token1".to_string(), "0".to_string())
            .await
            .unwrap();

        let res = builder.generate_deposit_calldatas().await.unwrap();
        match res {
            DepositCalldataResult::Calldatas(calldatas) => {
                assert!(calldatas.is_empty());
            }
            DepositCalldataResult::NoDeposits => {
                panic!("should not be no deposits");
            }
        }
    }

    #[tokio::test]
    async fn test_missing_select_tokens() {
        let mut builder = initialize_builder_with_select_tokens().await;

        let err = builder
            .check_allowances(Address::random().to_string())
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            RaindexOrderBuilderError::TokenMustBeSelected("token3".to_string()).to_string()
        );
        assert_eq!(
            err.to_readable_msg(),
            "The token 'token3' must be selected to proceed."
        );

        let err = builder.generate_deposit_calldatas().await.unwrap_err();
        assert_eq!(
            err.to_string(),
            RaindexOrderBuilderError::TokenMustBeSelected("token3".to_string()).to_string()
        );
        assert_eq!(
            err.to_readable_msg(),
            "The token 'token3' must be selected to proceed."
        );

        let err = builder.generate_add_order_calldata().await.unwrap_err();
        assert_eq!(
            err.to_string(),
            RaindexOrderBuilderError::TokenMustBeSelected("token3".to_string()).to_string()
        );
        assert_eq!(
            err.to_readable_msg(),
            "The token 'token3' must be selected to proceed."
        );

        let err = builder
            .generate_deposit_and_add_order_calldatas()
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            RaindexOrderBuilderError::TokenMustBeSelected("token3".to_string()).to_string()
        );
        assert_eq!(
            err.to_readable_msg(),
            "The token 'token3' must be selected to proceed."
        );
    }

    #[tokio::test]
    async fn test_missing_field_values() {
        let mut builder = initialize_builder(None).await;

        let err = builder.generate_add_order_calldata().await.unwrap_err();
        assert_eq!(
            err.to_string(),
            RaindexOrderBuilderError::FieldValueNotSet("Field 2 name".to_string()).to_string()
        );
        assert_eq!(
            err.to_readable_msg(),
            "The value for field 'Field 2 name' is required but has not been set."
        );

        let err = builder
            .generate_deposit_and_add_order_calldatas()
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            RaindexOrderBuilderError::FieldValueNotSet("Field 2 name".to_string()).to_string()
        );
        assert_eq!(
            err.to_readable_msg(),
            "The value for field 'Field 2 name' is required but has not been set."
        );
    }

    #[tokio::test]
    async fn test_prepare_add_order_args_injects_builder_meta() {
        let mut builder = initialize_builder(None).await;
        builder
            .set_field_value("binding-1".to_string(), "10".to_string())
            .unwrap();
        builder
            .set_field_value("binding-2".to_string(), "0".to_string())
            .unwrap();

        let deployment = builder.get_current_deployment().unwrap();
        let trimmed_dotrain = builder
            .dotrain_order
            .generate_dotrain_for_deployment(&deployment.deployment.key)
            .unwrap();

        let add_order_args = builder
            .prepare_add_order_args(&deployment)
            .await
            .expect("add order args");

        let additional_meta = add_order_args
            .additional_meta
            .as_ref()
            .expect("meta missing");
        assert_eq!(additional_meta.len(), 1);
        assert_eq!(
            additional_meta[0].magic,
            rain_metadata::KnownMagic::OrderBuilderStateV1
        );

        let emit_meta_call = add_order_args
            .try_into_emit_meta_call()
            .expect("emit meta call err")
            .expect("emit meta call missing");
        let decoded = RainMetaDocumentV1Item::cbor_decode(emit_meta_call.meta.as_ref()).unwrap();
        assert_eq!(decoded.len(), 1);
        let dotrain_source = DotrainSourceV1::try_from(decoded[0].clone()).unwrap();
        assert_eq!(dotrain_source.0, trimmed_dotrain);

        assert_eq!(
            emit_meta_call.subject,
            DotrainSourceV1(trimmed_dotrain).hash()
        );
    }

    #[tokio::test]
    async fn test_get_vault_ids() {
        let builder = initialize_builder(None).await;
        let res = builder.get_vault_ids().unwrap();
        assert_eq!(res.0.len(), 2);
        assert_eq!(res.0["input"]["token1"], Some(U256::from(1)));
        assert_eq!(res.0["output"]["token2"], Some(U256::from(1)));

        let mut builder = initialize_builder(Some("other-deployment".to_string())).await;

        let res = builder.get_vault_ids().unwrap();
        assert_eq!(res.0.len(), 2);
        assert_eq!(res.0["input"]["token1"], None);
        assert_eq!(res.0["output"]["token1"], None);

        builder
            .set_vault_id(
                VaultType::Input,
                "token1".to_string(),
                Some("999".to_string()),
            )
            .unwrap();
        builder
            .set_vault_id(
                VaultType::Output,
                "token1".to_string(),
                Some("888".to_string()),
            )
            .unwrap();

        let res = builder.get_vault_ids().unwrap();
        assert_eq!(res.0.len(), 2);
        assert_eq!(res.0["input"]["token1"], Some(U256::from(999)));
        assert_eq!(res.0["output"]["token1"], Some(U256::from(888)));
    }

    #[tokio::test]
    async fn test_has_any_vault_id() {
        let mut builder = initialize_builder(Some("other-deployment".to_string())).await;
        assert!(!builder.has_any_vault_id().unwrap());
        builder
            .set_vault_id(
                VaultType::Input,
                "token1".to_string(),
                Some("1".to_string()),
            )
            .unwrap();
        assert!(builder.has_any_vault_id().unwrap());
    }

    #[tokio::test]
    async fn test_update_scenario_bindings() {
        let mut builder = initialize_builder(Some("other-deployment".to_string())).await;

        let deployment = builder.get_current_deployment().unwrap();
        assert!(!deployment
            .deployment
            .scenario
            .bindings
            .contains_key("binding-1"));
        assert!(!deployment
            .deployment
            .scenario
            .bindings
            .contains_key("binding-2"));

        builder
            .set_field_value("binding-1".to_string(), "100".to_string())
            .unwrap();
        builder
            .set_field_value("binding-2".to_string(), "200".to_string())
            .unwrap();
        builder.update_scenario_bindings().unwrap();

        let deployment = builder.get_current_deployment().unwrap();
        assert_eq!(deployment.deployment.scenario.bindings["binding-1"], "100");
        assert_eq!(deployment.deployment.scenario.bindings["binding-2"], "200");
    }

    #[cfg(not(target_family = "wasm"))]
    mod non_wasm_tests {
        use super::*;
        use crate::raindex_order_builder::tests::get_yaml;
        use httpmock::{Method::POST, MockServer};
        use serde_json::json;

        async fn initialize_builder_with_metaboard_url(url: &str) -> RaindexOrderBuilder {
            let yaml = get_yaml().replace("https://metaboard.com", url);
            RaindexOrderBuilder::new_with_deployment(yaml, None, "some-deployment".to_string())
                .await
                .unwrap()
        }

        #[tokio::test]
        async fn test_should_emit_meta_call_false_when_meta_exists() {
            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.method(POST).path("/");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "metaV1S": [
                            {
                                "meta": "0x01",
                                "metaHash": "0x00",
                                "sender": "0x00",
                                "id": "0x00",
                                "metaBoard": {
                                    "id": "0x00",
                                    "metas": [],
                                    "address": "0x00"
                                },
                                "subject": "0x00"
                            }
                        ]
                    }
                }));
            });

            let mut builder = initialize_builder_with_metaboard_url(&server.url("/")).await;
            builder
                .set_field_value("binding-1".to_string(), "10".to_string())
                .unwrap();
            builder
                .set_field_value("binding-2".to_string(), "0".to_string())
                .unwrap();

            assert!(!builder.should_emit_meta_call().await.unwrap());
        }

        #[tokio::test]
        async fn test_should_emit_meta_call_true_when_no_meta() {
            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.method(POST).path("/");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "metaV1S": []
                    }
                }));
            });

            let mut builder = initialize_builder_with_metaboard_url(&server.url("/")).await;
            builder
                .set_field_value("binding-1".to_string(), "10".to_string())
                .unwrap();
            builder
                .set_field_value("binding-2".to_string(), "0".to_string())
                .unwrap();

            assert!(builder.should_emit_meta_call().await.unwrap());
        }

        #[tokio::test]
        async fn test_should_emit_meta_call_propagates_errors() {
            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.method(POST).path("/");
                then.status(500);
            });

            let mut builder = initialize_builder_with_metaboard_url(&server.url("/")).await;
            builder
                .set_field_value("binding-1".to_string(), "10".to_string())
                .unwrap();
            builder
                .set_field_value("binding-2".to_string(), "0".to_string())
                .unwrap();

            let err = builder.should_emit_meta_call().await.unwrap_err();
            assert!(matches!(
                err,
                RaindexOrderBuilderError::MetaboardSubgraphClientError(_)
            ));
        }
    }
}
