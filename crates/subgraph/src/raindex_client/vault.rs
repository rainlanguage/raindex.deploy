use rain_math_float::Float;

use super::*;

impl RaindexSubgraphClient {
    /// Fetch single vault
    pub async fn vault_detail(&self, id: Id) -> Result<SgVault, RaindexSubgraphClientError> {
        let data = self
            .query::<SgVaultDetailQuery, SgIdQueryVariables>(SgIdQueryVariables { id: &id })
            .await?;
        let vault = data.vault.ok_or(RaindexSubgraphClientError::Empty)?;

        Ok(vault)
    }

    fn vaults_list_filter(
        filter_args: &SgVaultsListFilterArgs,
    ) -> Option<SgVaultsListQueryFilters> {
        let balance_not = if filter_args.hide_zero_balance {
            Some(SgBytes(Float::default().get_inner().to_string()))
        } else {
            None
        };

        let or = if filter_args.only_active_orders {
            let active_order_filter = SgOrdersListQueryFilters {
                active: Some(true),
                ..Default::default()
            };
            Some(vec![
                SgVaultsListQueryFilters {
                    orders_as_input_: Some(Box::new(active_order_filter.clone())),
                    ..Default::default()
                },
                SgVaultsListQueryFilters {
                    orders_as_output_: Some(Box::new(active_order_filter)),
                    ..Default::default()
                },
            ])
        } else {
            None
        };

        let has_filters = !filter_args.owners.is_empty()
            || filter_args.hide_zero_balance
            || !filter_args.tokens.is_empty()
            || !filter_args.raindexes.is_empty()
            || filter_args.only_active_orders;

        has_filters.then(|| SgVaultsListQueryFilters {
            owner_in: filter_args.owners.clone(),
            balance_not,
            token_in: filter_args.tokens.clone(),
            raindex_in: filter_args.raindexes.clone(),
            orders_as_input_: None,
            orders_as_output_: None,
            or,
        })
    }

    /// Fetch all vaults, paginated
    pub async fn vaults_list(
        &self,
        filter_args: SgVaultsListFilterArgs,
        pagination_args: SgPaginationArgs,
    ) -> Result<Vec<SgVault>, RaindexSubgraphClientError> {
        let pagination_variables = Self::parse_pagination_args(pagination_args);

        let variables = SgVaultsListQueryVariables {
            first: pagination_variables.first,
            skip: pagination_variables.skip,
            filters: Self::vaults_list_filter(&filter_args),
        };

        let data = self
            .query::<SgVaultsListQuery, SgVaultsListQueryVariables>(variables)
            .await?;

        Ok(data.vaults)
    }

    async fn fetch_all_vaults_pages(
        &self,
        filter_args: SgVaultsListFilterArgs,
    ) -> Result<Vec<SgVault>, RaindexSubgraphClientError> {
        let mut all_pages_merged = vec![];
        let mut page = 1;

        loop {
            let page_data = self
                .vaults_list(
                    filter_args.clone(),
                    SgPaginationArgs {
                        page,
                        page_size: ALL_PAGES_QUERY_PAGE_SIZE,
                    },
                )
                .await?;
            let batch_len = page_data.len();
            all_pages_merged.extend(page_data);
            if (batch_len as u16) < ALL_PAGES_QUERY_PAGE_SIZE {
                break;
            }
            page += 1;
        }
        Ok(all_pages_merged)
    }

    /// Fetch all pages of vaults_list query
    pub async fn vaults_list_all(&self) -> Result<Vec<SgVault>, RaindexSubgraphClientError> {
        self.fetch_all_vaults_pages(SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: true,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: false,
        })
        .await
    }

    pub async fn vaults_count(
        &self,
        filter_args: SgVaultsListFilterArgs,
    ) -> Result<u32, RaindexSubgraphClientError> {
        let mut count = 0u32;
        let mut page = 1;

        loop {
            let page_data = self
                .vaults_list(
                    filter_args.clone(),
                    SgPaginationArgs {
                        page,
                        page_size: ALL_PAGES_QUERY_PAGE_SIZE,
                    },
                )
                .await?;
            let batch_len = page_data.len();
            count += batch_len as u32;
            if (batch_len as u16) < ALL_PAGES_QUERY_PAGE_SIZE {
                break;
            }
            page += 1;
        }

        Ok(count)
    }

    /// Fetch all vault deposits + withdrawals merged paginated, for a single vault
    /// Optionally filter by typename (e.g., "Deposit", "Withdrawal", "TradeVaultBalanceChange", "ClearBounty")
    /// Note: Subgraph filtering happens client-side after fetching, which is less efficient
    /// than local DB filtering. For large datasets, prefer using the local DB path.
    pub async fn vault_balance_changes_list(
        &self,
        id: cynic::Id,
        pagination_args: SgPaginationArgs,
        filter_typenames: Option<&[&str]>,
    ) -> Result<Vec<SgVaultBalanceChangeType>, RaindexSubgraphClientError> {
        let pagination_vars = Self::parse_pagination_args(pagination_args);
        let res = self
            .query_paginated(
                pagination_vars,
                VaultBalanceChangesListPageQueryClient::new(self.url.clone()),
                SgPaginationWithIdQueryVariables {
                    id: SgBytes(id.inner().to_string()),
                    skip: Some(0),
                    first: Some(200),
                },
                200,
            )
            .await?;

        if let Some(typenames) = filter_typenames {
            if !typenames.is_empty() {
                return Ok(res
                    .into_iter()
                    .filter(|item| typenames.iter().any(|t| *t == item.typename()))
                    .collect());
            }
        }

        Ok(res)
    }

    /// Fetch all pages of vault_balance_changes_list query
    pub async fn vault_balance_changes_list_all(
        &self,
        id: cynic::Id,
        filter_typenames: Option<&[&str]>,
    ) -> Result<Vec<SgVaultBalanceChangeType>, RaindexSubgraphClientError> {
        let mut all_pages_merged = vec![];
        let mut page = 1;

        loop {
            let page_data = self
                .vault_balance_changes_list(
                    id.clone(),
                    SgPaginationArgs {
                        page,
                        page_size: ALL_PAGES_QUERY_PAGE_SIZE,
                    },
                    filter_typenames,
                )
                .await?;
            if page_data.is_empty() {
                break;
            }
            all_pages_merged.extend(page_data);
            page += 1
        }
        Ok(all_pages_merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::{
        SgBigInt, SgBytes, SgErc20, SgOrderAsIO, SgRaindex, SgVault, SgVaultsListFilterArgs,
    };
    use crate::utils::float::*;
    use cynic::Id;
    use httpmock::prelude::*;
    use reqwest::Url;
    use serde_json::json;

    fn setup_client(server: &MockServer) -> RaindexSubgraphClient {
        let url = Url::parse(&server.url("")).unwrap();
        RaindexSubgraphClient::new(url)
    }

    fn default_sg_erc20() -> SgErc20 {
        SgErc20 {
            id: SgBytes("0xTokenId".to_string()),
            address: SgBytes("0xTokenAddress".to_string()),
            name: Some("Test Token".to_string()),
            symbol: Some("TTK".to_string()),
            decimals: Some(SgBigInt("18".to_string())),
        }
    }

    fn default_sg_raindex() -> SgRaindex {
        SgRaindex {
            id: SgBytes("0xRaindexId".to_string()),
        }
    }

    fn default_sg_order_as_io() -> SgOrderAsIO {
        SgOrderAsIO {
            id: SgBytes("0xOrderId".to_string()),
            order_hash: SgBytes("0xOrderHash".to_string()),
            active: true,
        }
    }

    fn default_sg_vault() -> SgVault {
        SgVault {
            id: SgBytes("0xVaultIdDefault".to_string()),
            owner: SgBytes("0xOwnerAddressDefault".to_string()),
            vault_id: SgBytes("1234567890".to_string()),
            balance: SgBytes(F1.as_hex()),
            token: default_sg_erc20(),
            raindex: default_sg_raindex(),
            orders_as_output: vec![default_sg_order_as_io()],
            orders_as_input: vec![],
            balance_changes: vec![],
        }
    }

    fn assert_sg_vault_eq(actual: &SgVault, expected: &SgVault) {
        assert_eq!(actual.id, expected.id, "Vault ID mismatch");
        assert_eq!(actual.owner, expected.owner, "Vault owner mismatch");
        assert_eq!(
            actual.vault_id, expected.vault_id,
            "Vault vault_id mismatch"
        );
        assert_eq!(actual.balance, expected.balance, "Vault balance mismatch");
        assert_eq!(actual.token.id, expected.token.id, "Token ID mismatch");
        assert_eq!(
            actual.token.address, expected.token.address,
            "Token address mismatch"
        );
        assert_eq!(
            actual.token.name, expected.token.name,
            "Token name mismatch"
        );
        assert_eq!(
            actual.token.symbol, expected.token.symbol,
            "Token symbol mismatch"
        );
        assert_eq!(
            actual.token.decimals, expected.token.decimals,
            "Token decimals mismatch"
        );
        assert_eq!(
            actual.raindex.id, expected.raindex.id,
            "Raindex ID mismatch"
        );
        assert_eq!(
            actual.orders_as_output.len(),
            expected.orders_as_output.len(),
            "Orders as output length mismatch"
        );
        for (act_o, exp_o) in actual
            .orders_as_output
            .iter()
            .zip(expected.orders_as_output.iter())
        {
            assert_eq!(act_o.id, exp_o.id);
            assert_eq!(act_o.order_hash, exp_o.order_hash);
            assert_eq!(act_o.active, exp_o.active);
        }
        assert_eq!(
            actual.orders_as_input.len(),
            expected.orders_as_input.len(),
            "Orders as input length mismatch"
        );
        for (act_i, exp_i) in actual
            .orders_as_input
            .iter()
            .zip(expected.orders_as_input.iter())
        {
            assert_eq!(act_i.id, exp_i.id);
            assert_eq!(act_i.order_hash, exp_i.order_hash);
            assert_eq!(act_i.active, exp_i.active);
        }
    }

    fn default_balance_change_json() -> serde_json::Value {
        json!({
            "__typename": "Deposit",
            "id": "0xDepositId",
            "amount": F0_5.as_hex(),
            "newVaultBalance": F1_5.as_hex(),
            "oldVaultBalance": F1.as_hex(),
            "vault": {
                "id": "0xVaultIdForBalanceChange",
                "vaultId": "12345",
                "token": {
                    "id": "0xTokenId",
                    "address": "0xTokenAddress",
                    "name": "Test Token",
                    "symbol": "TTK",
                    "decimals": "18"
                }
            },
            "timestamp": "1700000100",
            "transaction": {
                "id": "0xTransactionId",
                "from": "0xSenderAddress",
                "blockNumber": "100",
                "timestamp": "1700000000"
            },
            "raindex": {
                "id": "0xRaindexId"
            }
        })
    }

    #[tokio::test]
    async fn test_vault_detail_found() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let vault_id_str = "0xExistingVaultId";
        let vault_id = Id::new(vault_id_str);
        let expected_vault = default_sg_vault();

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"vault": expected_vault}}));
        });

        let result = client.vault_detail(vault_id).await;
        assert!(result.is_ok());
        let vault = result.unwrap();
        assert_sg_vault_eq(&vault, &expected_vault);
    }

    #[tokio::test]
    async fn test_vault_detail_not_found() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let vault_id_str = "0xNonExistentVaultId";
        let vault_id = Id::new(vault_id_str);

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(json!({"data": {"vault": null}}));
        });

        let result = client.vault_detail(vault_id).await;
        assert!(matches!(result, Err(RaindexSubgraphClientError::Empty)));
    }

    #[tokio::test]
    async fn test_vault_detail_network_error() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let vault_id = Id::new("0xAnyVaultId");

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let result = client.vault_detail(vault_id).await;
        assert!(matches!(
            result,
            Err(RaindexSubgraphClientError::CynicClientError(_))
        ));
    }

    #[tokio::test]
    async fn test_vaults_list_no_filters() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: false,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };
        let expected_vaults = vec![default_sg_vault(), default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains("\"skip\":0")
                .body_contains("\"first\":10");
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(result.is_ok());
        let vaults = result.unwrap();
        assert_eq!(vaults.len(), expected_vaults.len());
        for (actual, expected) in vaults.iter().zip(expected_vaults.iter()) {
            assert_sg_vault_eq(actual, expected);
        }
    }

    #[tokio::test]
    async fn test_vaults_count_fetches_all_pages() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: false,
        };
        let full_page: Vec<SgVault> = (0..ALL_PAGES_QUERY_PAGE_SIZE)
            .map(|_| default_sg_vault())
            .collect();
        let final_page = vec![default_sg_vault(), default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains("\"skip\":0")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200)
                .json_body(json!({"data": {"vaults": full_page}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200)
                .json_body(json!({"data": {"vaults": final_page}}));
        });

        let count = client.vaults_count(filter_args).await.unwrap();
        assert_eq!(count, ALL_PAGES_QUERY_PAGE_SIZE as u32 + 2);
    }

    #[tokio::test]
    async fn test_vaults_list_with_owner_filter() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let owner_address = SgBytes("0xSpecificOwner".to_string());
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![owner_address.clone()],
            hide_zero_balance: false,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: false,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 5,
        };
        let expected_vaults = vec![default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains("\"owner_in\":[\"0xSpecificOwner\"]");
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(result.is_ok());
        let vaults = result.unwrap();
        assert_eq!(vaults.len(), expected_vaults.len());
    }

    #[tokio::test]
    async fn test_vaults_list_with_hide_zero_balance_true() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: true,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: false,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 5,
        };
        let expected_vaults = vec![default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains("\"balance_not\":\"0x0000000000000000000000000000000000000000000000000000000000000000\"");
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        let vaults = result.unwrap();
        assert_eq!(vaults.len(), expected_vaults.len());
    }

    #[tokio::test]
    async fn test_vaults_list_pagination() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: false,
        };
        let pagination_args = SgPaginationArgs {
            page: 2,
            page_size: 3,
        };
        let expected_vaults = vec![default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains("\"skip\":3")
                .body_contains("\"first\":3");
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_vaults_list_empty_result() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: false,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(json!({"data": {"vaults": []}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_vaults_list_network_error() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: false,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(matches!(
            result,
            Err(RaindexSubgraphClientError::CynicClientError(_))
        ));
    }

    #[tokio::test]
    async fn test_vaults_list_all_multiple_pages() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let vaults_page1: Vec<SgVault> = (0..ALL_PAGES_QUERY_PAGE_SIZE)
            .map(|_| default_sg_vault())
            .collect();
        let vaults_page2: Vec<SgVault> = (0..50).map(|_| default_sg_vault()).collect();

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0")
                .body_contains("\"balance_not\":\"0x0000000000000000000000000000000000000000000000000000000000000000\"");
            then.status(200)
                .json_body(json!({"data": {"vaults": vaults_page1}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"balance_not\":\"0x0000000000000000000000000000000000000000000000000000000000000000\"");
            then.status(200)
                .json_body(json!({"data": {"vaults": vaults_page2}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE * 2))
                .body_contains("\"balance_not\":\"0x0000000000000000000000000000000000000000000000000000000000000000\"");
            then.status(200).json_body(json!({"data": {"vaults": []}}));
        });

        let result = client.vaults_list_all().await;
        let vaults = result.unwrap();
        assert_eq!(vaults.len(), ALL_PAGES_QUERY_PAGE_SIZE as usize + 50);
    }

    #[tokio::test]
    async fn test_vaults_list_all_no_vaults() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0")
                .body_contains("\"balance_not\":\"0x0000000000000000000000000000000000000000000000000000000000000000\"");
            then.status(200).json_body(json!({"data": {"vaults": []}}));
        });
        let result = client.vaults_list_all().await;
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_vaults_list_all_network_error_on_page() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let vaults_page1: Vec<SgVault> = (0..ALL_PAGES_QUERY_PAGE_SIZE)
            .map(|_| default_sg_vault())
            .collect();

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"vaults": vaults_page1}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(500);
        });

        let result = client.vaults_list_all().await;
        assert!(matches!(
            result,
            Err(RaindexSubgraphClientError::CynicClientError(_))
        ));
    }

    #[tokio::test]
    async fn test_vault_balance_changes_list_found() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let vault_id_str = "0xVaultForBalanceChanges";
        let vault_id = Id::new(vault_id_str);
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };
        let expected_json = vec![default_balance_change_json()];

        sg_server.mock(|when, then| {
            when.method(POST).path("/").body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"vaultBalanceChanges": expected_json}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST).path("/").body_contains("\"skip\":200");
            then.status(200)
                .json_body(json!({"data": {"vaultBalanceChanges": []}}));
        });

        let result = client
            .vault_balance_changes_list(vault_id.clone(), pagination_args, None)
            .await;
        assert!(result.is_ok(), "Result was: {:?}", result.err());
        let changes = result.unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].typename(), "Deposit");
    }

    #[tokio::test]
    async fn test_vault_balance_changes_list_empty_result() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let vault_id = Id::new("0xVaultWithNoChanges");
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"vaultBalanceChanges": []}}));
        });

        let result = client
            .vault_balance_changes_list(vault_id, pagination_args, None)
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_vault_balance_changes_list_network_error() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let vault_id = Id::new("0xAnyVaultId");
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let result = client
            .vault_balance_changes_list(vault_id, pagination_args, None)
            .await;
        assert!(matches!(
            result,
            Err(RaindexSubgraphClientError::PaginationClientError(_))
        ));
    }

    #[tokio::test]
    async fn test_vault_balance_changes_list_with_type_filter() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let vault_id_str = "0xVaultForBalanceChanges";
        let vault_id = Id::new(vault_id_str);
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };

        let mut deposit_json = default_balance_change_json();
        deposit_json["__typename"] = json!("Deposit");
        let mut withdrawal_json = default_balance_change_json();
        withdrawal_json["__typename"] = json!("Withdrawal");
        withdrawal_json["id"] = json!("0xWithdrawalId");
        let all_changes_json = vec![deposit_json, withdrawal_json];

        sg_server.mock(|when, then| {
            when.method(POST).path("/").body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"vaultBalanceChanges": all_changes_json}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST).path("/").body_contains("\"skip\":200");
            then.status(200)
                .json_body(json!({"data": {"vaultBalanceChanges": []}}));
        });

        let filter_typenames: &[&str] = &["Deposit"];
        let result = client
            .vault_balance_changes_list(vault_id.clone(), pagination_args, Some(filter_typenames))
            .await;
        assert!(result.is_ok(), "Result was: {:?}", result.err());
        let changes = result.unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].typename(), "Deposit");
    }

    #[tokio::test]
    async fn test_vault_balance_changes_list_all_multiple_pages() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let vault_id_str = "0xVaultForBalanceChangesAll";
        let vault_id = Id::new(vault_id_str);

        let changes_page1: Vec<serde_json::Value> = (0..ALL_PAGES_QUERY_PAGE_SIZE)
            .map(|_| default_balance_change_json())
            .collect();
        let changes_page2: Vec<serde_json::Value> =
            (0..30).map(|_| default_balance_change_json()).collect();

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"id\":\"{}\"", vault_id_str))
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"vaultBalanceChanges": changes_page1}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"id\":\"{}\"", vault_id_str))
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200)
                .json_body(json!({"data": {"vaultBalanceChanges": changes_page2}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"id\":\"{}\"", vault_id_str))
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE * 2));
            then.status(200)
                .json_body(json!({"data": {"vaultBalanceChanges": []}}));
        });

        let result = client
            .vault_balance_changes_list_all(vault_id.clone(), None)
            .await;
        assert!(result.is_ok(), "Result was: {:?}", result.err());
        let changes = result.unwrap();
        assert_eq!(changes.len(), ALL_PAGES_QUERY_PAGE_SIZE as usize + 30);
    }

    #[tokio::test]
    async fn test_vault_balance_changes_list_all_no_changes() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let vault_id = Id::new("0xVaultWithNoChangesAll");

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"vaultBalanceChanges": []}}));
        });

        let result = client.vault_balance_changes_list_all(vault_id, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_vaults_list_with_token_filter() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let token_address = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913".to_string();
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![token_address.clone()],
            raindexes: vec![],
            only_active_orders: false,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };
        let expected_vaults = vec![default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"token_in\":[\"{}\"]", token_address));
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(result.is_ok());
        let vaults = result.unwrap();
        assert_eq!(vaults.len(), expected_vaults.len());
    }

    #[tokio::test]
    async fn test_vaults_list_with_multiple_token_filters() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let token1 = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913".to_string();
        let token2 = "0xa0b86a33e6441f8c5e1e2a9f8c5e1e2a9f8c5e1e".to_string();
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![token1.clone(), token2.clone()],
            raindexes: vec![],
            only_active_orders: false,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };
        let expected_vaults = vec![default_sg_vault(), default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"token_in\":[\"{}\",\"{}\"]", token1, token2));
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(result.is_ok());
        let vaults = result.unwrap();
        assert_eq!(vaults.len(), expected_vaults.len());
    }

    #[tokio::test]
    async fn test_vaults_list_with_token_and_other_filters() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let token_address = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913".to_string();
        let owner_address = SgBytes("0xowner123".to_string());
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![owner_address.clone()],
            hide_zero_balance: true,
            tokens: vec![token_address.clone()],
            raindexes: vec![],
            only_active_orders: false,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 5,
        };
        let expected_vaults = vec![default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains("\"owner_in\":[\"0xowner123\"]")
                .body_contains("\"balance_not\":\"0x0000000000000000000000000000000000000000000000000000000000000000\"")
                .body_contains(format!("\"token_in\":[\"{}\"]", token_address));
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        let vaults = result.unwrap();
        assert_eq!(vaults.len(), expected_vaults.len());
    }

    #[tokio::test]
    async fn test_vaults_list_with_raindex_filter() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let raindex_address = "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string();
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![],
            raindexes: vec![raindex_address.clone()],
            only_active_orders: false,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };
        let expected_vaults = vec![default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"raindex_in\":[\"{}\"]", raindex_address));
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(result.is_ok());
        let vaults = result.unwrap();
        assert_eq!(vaults.len(), expected_vaults.len());
    }

    #[tokio::test]
    async fn test_vaults_list_with_multiple_raindex_filters() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let ob1 = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        let ob2 = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![],
            raindexes: vec![ob1.clone(), ob2.clone()],
            only_active_orders: false,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };
        let expected_vaults = vec![default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"raindex_in\":[\"{}\",\"{}\"]", ob1, ob2));
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(result.is_ok());
        let vaults = result.unwrap();
        assert_eq!(vaults.len(), expected_vaults.len());
    }

    #[tokio::test]
    async fn test_vaults_list_with_only_active_orders_filter() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: true,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };
        let expected_vaults = vec![default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains("\"or\":")
                .body_contains("\"ordersAsInput_\":")
                .body_contains("\"ordersAsOutput_\":")
                .body_contains("\"active\":true");
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(result.is_ok());
        let vaults = result.unwrap();
        assert_eq!(vaults.len(), expected_vaults.len());
    }

    #[tokio::test]
    async fn test_vaults_list_with_only_active_orders_and_other_filters() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let owner_address = SgBytes("0xOwnerWithActiveOrders".to_string());
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![owner_address.clone()],
            hide_zero_balance: true,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: true,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };
        let expected_vaults = vec![default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains("\"owner_in\":[\"0xOwnerWithActiveOrders\"]")
                .body_contains("\"balance_not\":\"0x0000000000000000000000000000000000000000000000000000000000000000\"")
                .body_contains("\"or\":")
                .body_contains("\"ordersAsInput_\":")
                .body_contains("\"ordersAsOutput_\":")
                .body_contains("\"active\":true");
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(result.is_ok());
        let vaults = result.unwrap();
        assert_eq!(vaults.len(), expected_vaults.len());
    }

    #[tokio::test]
    async fn test_vaults_list_filters_some_when_only_active_orders_true() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let filter_args = SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: true,
        };
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };
        let expected_vaults = vec![default_sg_vault()];

        sg_server.mock(|when, then| {
            when.method(POST).path("/").body_contains("\"filters\":");
            then.status(200)
                .json_body(json!({"data": {"vaults": expected_vaults}}));
        });

        let result = client.vaults_list(filter_args, pagination_args).await;
        assert!(result.is_ok());
    }
}
