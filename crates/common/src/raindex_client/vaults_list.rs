use alloy::{primitives::Bytes, sol_types::SolCall};
use rain_math_float::Float;
use raindex_bindings::Raindex::multicallCall;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wasm_bindgen_utils::prelude::*;

use crate::raindex_client::{vaults::RaindexVault, RaindexError};
use once_cell::sync::Lazy;

static ZERO_FLOAT: Lazy<Float> = Lazy::new(|| Float::parse("0".to_string()).unwrap());

#[derive(Serialize, Deserialize, Debug, Clone)]
#[wasm_bindgen]
pub struct RaindexVaultsList(Vec<RaindexVault>);

impl RaindexVaultsList {
    pub fn new(vaults: Vec<RaindexVault>) -> Self {
        Self(vaults)
    }
    pub fn get_withdrawable_vaults(&self) -> Result<Vec<&RaindexVault>, VaultsListError> {
        self.0
            .iter()
            .map(|vault| {
                vault
                    .token_safe_withdraw_amount(&vault.balance())
                    .and_then(|amount| amount.gt(*ZERO_FLOAT).map_err(RaindexError::from))
                    .map(|is_withdrawable| is_withdrawable.then_some(vault))
                    .map_err(|e| VaultsListError::WithdrawAmountError(e.to_readable_msg()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|vaults| vaults.into_iter().flatten().collect())
    }

    pub fn pick_by_ids(&self, ids: Vec<String>) -> RaindexVaultsList {
        use std::collections::HashSet;
        let ids_set: HashSet<String> = ids.into_iter().collect();
        let filtered_vaults = self
            .0
            .iter()
            .filter(|vault| ids_set.contains(&vault.id().to_string()))
            .cloned()
            .collect();
        RaindexVaultsList::new(filtered_vaults)
    }

    pub fn concat(&self, other: &RaindexVaultsList) -> RaindexVaultsList {
        let mut combined_vaults = Vec::with_capacity(self.0.len() + other.0.len());
        combined_vaults.extend_from_slice(&self.0);
        combined_vaults.extend_from_slice(&other.0);
        RaindexVaultsList::new(combined_vaults)
    }

    pub async fn get_withdraw_calldata(&self) -> Result<Bytes, VaultsListError> {
        let mut calldatas: Vec<Bytes> = Vec::new();
        let vaults_to_withdraw = self.get_withdrawable_vaults()?;
        // If no vaults to withdraw, return error
        if vaults_to_withdraw.is_empty() {
            return Err(VaultsListError::NoWithdrawableVaults);
        }
        let mut raindex_id_iter = vaults_to_withdraw.iter().map(|v| v.raindex());
        let first_raindex_id = raindex_id_iter.next();
        if let Some(first_id) = first_raindex_id {
            if raindex_id_iter.any(|id| id != first_id) {
                return Err(VaultsListError::MultipleRaindexesUsed);
            }
        }
        // Generate multicall calldata for all vaults
        for vault in vaults_to_withdraw {
            let withdraw_amount = vault
                .token_safe_withdraw_amount(&vault.balance())
                .map_err(|e| VaultsListError::WithdrawMulticallError(e.to_readable_msg()))?;
            match vault.build_withdraw_calldata(&withdraw_amount).await {
                Ok(calldata) => calldatas.push(calldata),
                Err(e) => return Err(VaultsListError::WithdrawMulticallError(e.to_readable_msg())),
            }
        }

        if calldatas.len() == 1 {
            // If only one vault, return its calldata directly
            return Ok(calldatas[0].clone());
        }
        // Otherwise, create a multicall with all vaults' calldata
        let multicall = multicallCall { data: calldatas };
        let encoded = multicall.abi_encode();
        Ok(encoded.into())
    }
}

#[cfg(not(target_family = "wasm"))]
impl RaindexVaultsList {
    pub fn items(&self) -> Vec<RaindexVault> {
        self.0.clone()
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl RaindexVaultsList {
    /// Returns all vaults in the list
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const allVaults = vaultsList.items;
    /// allVaults.forEach(vault => {
    ///   console.log(`Vault ID: ${vault.id}, Balance: ${vault.formattedBalance}`);
    /// });
    /// ```
    #[wasm_bindgen(getter)]
    pub fn items(&self) -> Vec<RaindexVault> {
        self.0.clone()
    }
}

#[wasm_export]
impl RaindexVaultsList {
    /// Returns only vaults that have a balance greater than zero
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const withdrawableVaults = vaultsList.getWithdrawableVaults();
    /// console.log(`${withdrawableVaults.length} vaults can be withdrawn from`);
    /// ```
    #[wasm_export(
        js_name = "getWithdrawableVaults",
        return_description = "Array of vaults with non-zero token-withdrawable balance",
        unchecked_return_type = "RaindexVault[]",
        preserve_js_class
    )]
    pub fn get_withdrawable_vaults_wasm(&self) -> Result<Vec<RaindexVault>, VaultsListError> {
        Ok(self
            .get_withdrawable_vaults()?
            .into_iter()
            .cloned()
            .collect())
    }

    /// Generates multicall withdraw calldata for all withdrawable vaults
    ///
    /// Creates a single transaction that can withdraw from multiple vaults
    /// at once, optimizing gas costs and transaction efficiency.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await vaultsList.getWithdrawCalldata();
    /// if (result.error) {
    ///   console.error("Error generating calldata:", result.error.readableMsg);
    ///   return;
    /// }
    /// const calldata = result.value;
    /// // Use calldata for transaction
    /// ```
    #[wasm_export(
        js_name = "getWithdrawCalldata",
        return_description = "Encoded multicall calldata for withdrawing from all vaults",
        unchecked_return_type = "Hex"
    )]
    pub async fn get_withdraw_calldata_wasm(&self) -> Result<Bytes, VaultsListError> {
        let calldata = self.get_withdraw_calldata().await?;
        Ok(calldata)
    }

    /// Filters vaults by a list of IDs and returns a new RaindexVaultsList
    ///
    /// Creates a new vault list containing only vaults whose IDs match
    /// the provided list of IDs. The original list order is preserved;
    /// the input `ids` order does not affect the result ordering.
    ///
    /// Notes:
    /// - Duplicate IDs in `ids` are ignored (deduplicated).
    /// - Unknown IDs are ignored (they do not cause errors).
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const filteredVaults = vaultsList.pickByIds(['0x123', '0x456']);
    /// console.log(`Filtered to ${filteredVaults.items.length} vaults`);
    /// ```
    #[wasm_export(
        js_name = "pickByIds",
        return_description = "New RaindexVaultsList containing only vaults with matching IDs",
        unchecked_return_type = "RaindexVaultsList",
        preserve_js_class
    )]
    pub fn pick_by_ids_wasm(&self, ids: Vec<String>) -> Result<RaindexVaultsList, VaultsListError> {
        Ok(self.pick_by_ids(ids))
    }

    /// Concatenates this vault list with another vault list and returns a new RaindexVaultsList
    ///
    /// Creates a new vault list containing all vaults from both lists.
    /// The order is preserved: items from the current list first, then items from the other list.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const combinedVaults = vaultsList1.concat(vaultsList2);
    /// console.log(`Combined ${combinedVaults.items.length} vaults`);
    /// ```
    #[wasm_export(
        js_name = "concat",
        return_description = "New RaindexVaultsList containing vaults from both lists",
        unchecked_return_type = "RaindexVaultsList",
        preserve_js_class
    )]
    pub fn concat_wasm(
        &self,
        other: &RaindexVaultsList,
    ) -> Result<RaindexVaultsList, VaultsListError> {
        Ok(self.concat(other))
    }
}

#[derive(Error, Debug, Clone)]
pub enum VaultsListError {
    #[error("Failed to get withdraw multicall: {0}")]
    WithdrawMulticallError(String),
    #[error("Failed to normalize withdraw amount: {0}")]
    WithdrawAmountError(String),
    #[error("No withdrawable vaults available")]
    NoWithdrawableVaults,
    #[error("All vaults must share the same raindex for batch withdrawal")]
    MultipleRaindexesUsed,
}

impl VaultsListError {
    pub fn to_readable_msg(&self) -> String {
        match self {
            VaultsListError::WithdrawMulticallError(err) => {
                format!("Failed to generate withdraw multicall: {}", err)
            }
            VaultsListError::WithdrawAmountError(err) => {
                format!("Failed to normalize withdraw amount: {}", err)
            }
            VaultsListError::NoWithdrawableVaults => "No withdrawable vaults available".to_string(),
            VaultsListError::MultipleRaindexesUsed => {
                "All vaults must share the same raindex for batch withdrawal".to_string()
            }
        }
    }
}

impl From<VaultsListError> for JsValue {
    fn from(value: VaultsListError) -> Self {
        JsError::new(&value.to_string()).into()
    }
}

impl From<VaultsListError> for WasmEncodedError {
    fn from(value: VaultsListError) -> Self {
        WasmEncodedError {
            msg: value.to_string(),
            readable_msg: value.to_readable_msg(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_family = "wasm"))]
    mod non_wasm_tests {
        use super::*;
        use crate::raindex_client::{tests::get_test_yaml, RaindexClient};
        use alloy::{primitives::U256, sol_types::SolCall};
        use httpmock::MockServer;
        use raindex_bindings::IRaindexV7::withdraw5Call;
        use serde_json::{json, Value};

        fn get_vault1_json() -> Value {
            json!({
              "id": "0x0123",
              "owner": "0x0000000000000000000000000000000000000000",
              "vaultId": "0x10",
              "balance": "0x0000000000000000000000000000000000000000000000000000000000000000",
              "token": {
                "id": "token1",
                "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                "name": "Token 1",
                "symbol": "TKN1",
                "decimals": "18"
              },
              "raindex": {
                "id": "0x0000000000000000000000000000000000000000"
              },
              "ordersAsOutput": [],
              "ordersAsInput": [],
              "balanceChanges": []
            })
        }

        fn get_vault2_json() -> Value {
            json!({
                "id": "0x0234",
                "owner": "0x0000000000000000000000000000000000000000",
                "vaultId": "0x20",
                "balance": "0x0000000000000000000000000000000000000000000000000000000000000020",
                "token": {
                    "id": "token2",
                    "address": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                    "name": "Token 2",
                    "symbol": "TKN2",
                    "decimals": "18"
                },
                "raindex": {
                    "id": "0x0000000000000000000000000000000000000000"
                },
                "ordersAsOutput": [],
                "ordersAsInput": [],
                "balanceChanges": []
            })
        }

        async fn get_vaults_for(vault1: Value, vault2: Value) -> Vec<RaindexVault> {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [vault1]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [vault2]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    // not used
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let vaults = raindex_client
                .get_vaults(None, None, None, None)
                .await
                .unwrap();
            vaults.items()
        }

        async fn get_vaults() -> Vec<RaindexVault> {
            get_vaults_for(get_vault1_json(), get_vault2_json()).await
        }

        #[tokio::test]
        async fn test_get_vaults_not_empty() {
            let vaults_list = RaindexVaultsList::new(get_vaults().await);
            assert_eq!(vaults_list.0.len(), 2);
        }

        #[tokio::test]
        async fn test_get_withdrawable_vaults() {
            let vaults_list = RaindexVaultsList::new(get_vaults().await);
            let withdrawable_vaults = vaults_list.get_withdrawable_vaults().unwrap();
            assert_eq!(withdrawable_vaults.len(), 1);
            assert_eq!(withdrawable_vaults[0].id().to_string(), "0x0234"); // vault2 has non-zero token-withdrawable balance
        }

        #[tokio::test]
        async fn test_get_withdrawable_vaults_skips_token_dust() {
            let mut dust_vault = get_vault2_json();
            dust_vault["balance"] = Value::String(
                Float::from_fixed_decimal(U256::from(1), 7)
                    .unwrap()
                    .as_hex(),
            );
            dust_vault["token"]["decimals"] = Value::String("6".to_string());

            let vaults_list =
                RaindexVaultsList::new(get_vaults_for(get_vault1_json(), dust_vault).await);

            assert!(vaults_list.get_withdrawable_vaults().unwrap().is_empty());
            assert!(matches!(
                vaults_list.get_withdraw_calldata().await,
                Err(VaultsListError::NoWithdrawableVaults)
            ));
        }

        #[tokio::test]
        async fn test_get_withdraw_calldata() {
            let vaults_list = RaindexVaultsList::new(get_vaults().await);

            let result = vaults_list.get_withdraw_calldata().await;
            let calldata = result.unwrap();
            assert!(!calldata.is_empty());
            assert!(calldata.len() > 2); // should contain vault2's ID
        }

        #[tokio::test]
        async fn test_get_withdraw_calldata_truncates_to_token_decimals() {
            let mut vault = get_vault2_json();
            vault["balance"] = Value::String(
                Float::from_fixed_decimal(U256::from(1234567), 9)
                    .unwrap()
                    .as_hex(),
            );
            vault["token"]["decimals"] = Value::String("6".to_string());
            let vaults_list =
                RaindexVaultsList::new(get_vaults_for(get_vault1_json(), vault).await);

            let calldata = vaults_list.get_withdraw_calldata().await.unwrap();
            let decoded = withdraw5Call::abi_decode(&calldata).unwrap();

            assert_eq!(
                decoded.targetAmount,
                Float::from_fixed_decimal(U256::from(1234), 6)
                    .unwrap()
                    .get_inner()
            );
        }

        #[tokio::test]
        async fn test_pick_by_ids() {
            let vaults_list = RaindexVaultsList::new(get_vaults().await);

            // Test filtering by existing IDs
            let ids = vec!["0x0123".to_string(), "0x0234".to_string()];
            let filtered = vaults_list.pick_by_ids(ids);
            assert_eq!(filtered.items().len(), 2);

            // Verify order preservation: original vault sequence must prevail
            let ids_reversed = vec!["0x0234".to_string(), "0x0123".to_string()];
            let filtered_rev = vaults_list.pick_by_ids(ids_reversed);
            assert_eq!(filtered_rev.items().len(), 2);
            assert_eq!(filtered_rev.items()[0].id().to_string(), "0x0123");
            assert_eq!(filtered_rev.items()[1].id().to_string(), "0x0234");

            // Test filtering by single ID
            let ids = vec!["0x0234".to_string()];
            let filtered = vaults_list.pick_by_ids(ids);
            assert_eq!(filtered.items().len(), 1);
            assert_eq!(filtered.items()[0].id().to_string(), "0x0234");

            // Test filtering by non-existent ID
            let ids = vec!["0x9999".to_string()];
            let filtered = vaults_list.pick_by_ids(ids);
            assert_eq!(filtered.items().len(), 0);

            // Test empty IDs list
            let ids = vec![];
            let filtered = vaults_list.pick_by_ids(ids);
            assert_eq!(filtered.items().len(), 0);

            // Test duplicate IDs are deduped
            let ids_duped = vec!["0x0123".to_string(), "0x0123".to_string()];
            let filtered_duped = vaults_list.pick_by_ids(ids_duped);
            assert_eq!(filtered_duped.items().len(), 1);
            assert_eq!(filtered_duped.items()[0].id().to_string(), "0x0123");
        }

        #[tokio::test]
        async fn test_concat() {
            let vaults = get_vaults().await;
            let first_vault = vec![vaults[0].clone()];
            let second_vault = vec![vaults[1].clone()];

            let vaults_list1 = RaindexVaultsList::new(first_vault);
            let vaults_list2 = RaindexVaultsList::new(second_vault);

            // Test concatenation
            let combined = vaults_list1.concat(&vaults_list2);
            assert_eq!(combined.items().len(), 2);
            assert_eq!(combined.items()[0].id().to_string(), "0x0123");
            assert_eq!(combined.items()[1].id().to_string(), "0x0234");

            // Test concatenation with empty list
            let empty_list = RaindexVaultsList::new(vec![]);
            let combined_with_empty = vaults_list1.concat(&empty_list);
            assert_eq!(combined_with_empty.items().len(), 1);
            assert_eq!(combined_with_empty.items()[0].id().to_string(), "0x0123");

            // Test empty list concatenation
            let empty_combined = empty_list.concat(&vaults_list1);
            assert_eq!(empty_combined.items().len(), 1);
            assert_eq!(empty_combined.items()[0].id().to_string(), "0x0123");
        }
    }

    #[cfg(target_family = "wasm")]
    mod wasm_tests {
        use super::*;
        use wasm_bindgen_test::*;

        #[wasm_bindgen_test]
        async fn test_wasm_get_vaults() {
            let vaults_list = RaindexVaultsList::new(vec![]);
            let vaults = vaults_list.items();
            assert_eq!(vaults.len(), 0);
        }

        #[wasm_bindgen_test]
        async fn test_wasm_get_withdrawable_vaults_empty() {
            let vaults_list = RaindexVaultsList::new(vec![]);
            let result = vaults_list.get_withdrawable_vaults_wasm();
            let withdrawable = result.unwrap();
            assert_eq!(withdrawable.len(), 0);
        }

        #[wasm_bindgen_test]
        async fn test_wasm_get_withdraw_calldata_empty() {
            let vaults_list = RaindexVaultsList::new(vec![]);
            let result = vaults_list.get_withdraw_calldata_wasm().await;
            assert!(result.is_err());
        }

        #[wasm_bindgen_test]
        async fn test_wasm_pick_by_ids_empty() {
            let vaults_list = RaindexVaultsList::new(vec![]);
            let ids = vec!["0x123".to_string()];
            let result = vaults_list.pick_by_ids_wasm(ids);
            let filtered = result.unwrap();
            assert_eq!(filtered.items().len(), 0);
        }

        #[wasm_bindgen_test]
        async fn test_wasm_concat_empty() {
            let vaults_list1 = RaindexVaultsList::new(vec![]);
            let vaults_list2 = RaindexVaultsList::new(vec![]);
            let result = vaults_list1.concat_wasm(&vaults_list2);
            let combined = result.unwrap();
            assert_eq!(combined.items().len(), 0);
        }
    }
}
