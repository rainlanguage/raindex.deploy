use super::*;

impl RaindexSubgraphClient {
    /// Fetch single order take
    pub async fn order_trade_detail(&self, id: Id) -> Result<SgTrade, RaindexSubgraphClientError> {
        let data = self
            .query::<SgOrderTradeDetailQuery, SgIdQueryVariables>(SgIdQueryVariables { id: &id })
            .await?;
        let order_take = data.trade.ok_or(RaindexSubgraphClientError::Empty)?;

        Ok(order_take)
    }

    /// Fetch all order takes paginated for a single order
    pub async fn order_trades_list(
        &self,
        order_id: cynic::Id,
        pagination_args: SgPaginationArgs,
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
    ) -> Result<Vec<SgTrade>, RaindexSubgraphClientError> {
        let pagination_variables = Self::parse_pagination_args(pagination_args);
        let data = self
            .query::<SgOrderTradesListQuery, SgPaginationWithTimestampQueryVariables>(
                SgPaginationWithTimestampQueryVariables {
                    id: SgBytes(order_id.inner().to_string()),
                    first: pagination_variables.first,
                    skip: pagination_variables.skip,
                    timestamp_gte: Some(
                        start_timestamp
                            .map_or(SgBigInt("0".to_string()), |v| SgBigInt(v.to_string())),
                    ),
                    timestamp_lte: Some(
                        end_timestamp
                            .map_or(SgBigInt(u64::MAX.to_string()), |v| SgBigInt(v.to_string())),
                    ),
                },
            )
            .await?;

        Ok(data.trades)
    }

    pub async fn trades_by_transaction(
        &self,
        tx_id: String,
        raindex_in: Option<Vec<String>>,
    ) -> Result<Vec<SgTrade>, RaindexSubgraphClientError> {
        let mut all_trades = vec![];
        let mut page = 1;

        loop {
            let pagination_variables = Self::parse_pagination_args(SgPaginationArgs {
                page,
                page_size: ALL_PAGES_QUERY_PAGE_SIZE,
            });
            let data = self
                .query::<SgTransactionTradesQuery, SgPaginationWithTxIdQueryVariables>(
                    SgPaginationWithTxIdQueryVariables {
                        tx_id: tx_id.clone(),
                        first: pagination_variables.first,
                        skip: pagination_variables.skip,
                        raindex_in: raindex_in.clone(),
                    },
                )
                .await?;

            let page_len = data.trades.len();
            all_trades.extend(data.trades);
            if page_len < ALL_PAGES_QUERY_PAGE_SIZE as usize {
                break;
            }
            page += 1;
        }
        all_trades.sort_by(|a, b| {
            let a_timestamp = a.timestamp.0.parse::<u64>().unwrap_or(0);
            let b_timestamp = b.timestamp.0.parse::<u64>().unwrap_or(0);
            b_timestamp
                .cmp(&a_timestamp)
                .then_with(|| a.id.0.cmp(&b.id.0))
        });
        Ok(all_trades)
    }

    pub async fn trades_by_owner(
        &self,
        owner: String,
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
        raindex_in: Option<Vec<String>>,
        pagination_args: SgPaginationArgs,
    ) -> Result<Vec<SgTrade>, RaindexSubgraphClientError> {
        let pagination_variables = Self::parse_pagination_args(pagination_args);
        let data = self
            .query::<SgOwnerTradesListQuery, SgOwnerTradesQueryVariables>(
                SgOwnerTradesQueryVariables {
                    owner: SgBytes(owner),
                    first: pagination_variables.first,
                    skip: pagination_variables.skip,
                    timestamp_gte: Some(
                        start_timestamp
                            .map_or(SgBigInt("0".to_string()), |v| SgBigInt(v.to_string())),
                    ),
                    timestamp_lte: Some(
                        end_timestamp
                            .map_or(SgBigInt(u64::MAX.to_string()), |v| SgBigInt(v.to_string())),
                    ),
                    raindex_in,
                },
            )
            .await?;

        Ok(data.trades)
    }

    /// Fetch a single page of trades matching the provided filters.
    ///
    /// The filters are passed directly to the subgraph `trades` query and
    /// pagination is converted with `parse_pagination_args`. Results are ordered
    /// by timestamp descending by the GraphQL query. Subgraph query errors are
    /// returned as `RaindexSubgraphClientError`.
    pub async fn trades_list(
        &self,
        filters: SgTradesListQueryFilters,
        pagination_args: SgPaginationArgs,
    ) -> Result<Vec<SgTrade>, RaindexSubgraphClientError> {
        let pagination_variables = Self::parse_pagination_args(pagination_args);
        let data = self
            .query::<SgTradesListQuery, SgTradesListQueryVariables>(SgTradesListQueryVariables {
                first: pagination_variables.first,
                skip: pagination_variables.skip,
                filters: Some(filters),
            })
            .await?;

        Ok(data.trades)
    }

    async fn fetch_all_trades_pages(
        &self,
        filters: SgTradesListQueryFilters,
    ) -> Result<Vec<SgTrade>, RaindexSubgraphClientError> {
        let mut all_pages_merged = vec![];
        let mut page: u16 = 1;

        loop {
            let page_data = self
                .trades_list(
                    filters.clone(),
                    SgPaginationArgs {
                        page,
                        page_size: ALL_PAGES_QUERY_PAGE_SIZE,
                    },
                )
                .await?;
            let batch_len = page_data.len();
            all_pages_merged.extend(page_data);
            if batch_len < ALL_PAGES_QUERY_PAGE_SIZE as usize {
                break;
            }
            page += 1;
        }

        Ok(all_pages_merged)
    }

    /// Fetch all trades matching the provided filters across every page.
    ///
    /// This repeatedly calls `trades_list` using the standard all-pages query page
    /// size until a partial page is returned. Any subgraph query error encountered
    /// while fetching a page is propagated.
    pub async fn trades_list_all(
        &self,
        filters: SgTradesListQueryFilters,
    ) -> Result<Vec<SgTrade>, RaindexSubgraphClientError> {
        self.fetch_all_trades_pages(filters).await
    }

    /// Count all trades matching the provided filters.
    ///
    /// This walks every page with `fetch_all_trades_pages` and returns the number
    /// of matching trades. Any subgraph query error encountered while fetching
    /// pages is propagated.
    pub async fn trades_count(
        &self,
        filters: SgTradesListQueryFilters,
    ) -> Result<u32, RaindexSubgraphClientError> {
        Ok(self.fetch_all_trades_pages(filters).await?.len() as u32)
    }

    pub async fn trades_by_owner_all(
        &self,
        owner: String,
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
        raindex_in: Option<Vec<String>>,
    ) -> Result<Vec<SgTrade>, RaindexSubgraphClientError> {
        let mut all_trades = vec![];
        let mut page = 1;

        loop {
            let page_data = self
                .trades_by_owner(
                    owner.clone(),
                    start_timestamp,
                    end_timestamp,
                    raindex_in.clone(),
                    SgPaginationArgs {
                        page,
                        page_size: ALL_PAGES_QUERY_PAGE_SIZE,
                    },
                )
                .await?;
            let page_len = page_data.len();
            all_trades.extend(page_data);
            if page_len < ALL_PAGES_QUERY_PAGE_SIZE as usize {
                break;
            }
            page += 1;
        }
        Ok(all_trades)
    }

    /// Fetch all pages of order_takes_list query
    pub async fn order_trades_list_all(
        &self,
        order_id: cynic::Id,
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
    ) -> Result<Vec<SgTrade>, RaindexSubgraphClientError> {
        let mut all_pages_merged = vec![];
        let mut page = 1;

        loop {
            let page_data = self
                .order_trades_list(
                    order_id.clone(),
                    SgPaginationArgs {
                        page,
                        page_size: ALL_PAGES_QUERY_PAGE_SIZE,
                    },
                    start_timestamp,
                    end_timestamp,
                )
                .await?;
            let page_len = page_data.len();
            all_pages_merged.extend(page_data);
            if page_len < ALL_PAGES_QUERY_PAGE_SIZE as usize {
                break;
            }
            page += 1
        }
        Ok(all_pages_merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::{
        SgBigInt, SgBytes, SgErc20, SgRaindex, SgTradeEvent, SgTradeEventTypename, SgTradeRef,
        SgTradeStructPartialOrder, SgTradeVaultBalanceChange, SgTransaction,
        SgVaultBalanceChangeVault,
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

    fn default_sg_transaction() -> SgTransaction {
        SgTransaction {
            id: SgBytes("0xtransaction_id_default".to_string()),
            from: SgBytes("0xfrom_address_default".to_string()),
            block_number: SgBigInt("100".to_string()),
            timestamp: SgBigInt("1600000000".to_string()),
        }
    }

    fn default_sg_raindex() -> SgRaindex {
        SgRaindex {
            id: SgBytes("0xraindex_id_default".to_string()),
        }
    }

    fn default_sg_erc20() -> SgErc20 {
        SgErc20 {
            id: SgBytes("0xtoken_id_default".to_string()),
            address: SgBytes("0xtoken_address_default".to_string()),
            name: Some("Default Token".to_string()),
            symbol: Some("DTK".to_string()),
            decimals: Some(SgBigInt("18".to_string())),
        }
    }

    fn default_sg_vault_balance_change_vault() -> SgVaultBalanceChangeVault {
        SgVaultBalanceChangeVault {
            id: SgBytes("0xvault_id_default".to_string()),
            vault_id: SgBytes("12345".to_string()),
            token: default_sg_erc20(),
        }
    }

    fn default_sg_trade_event_typename() -> SgTradeEventTypename {
        SgTradeEventTypename {
            __typename: "TakeOrder".to_string(),
        }
    }

    fn default_sg_trade_ref() -> SgTradeRef {
        SgTradeRef {
            trade_event: default_sg_trade_event_typename(),
        }
    }

    fn default_sg_trade_vault_balance_change(type_name: &str) -> SgTradeVaultBalanceChange {
        SgTradeVaultBalanceChange {
            id: SgBytes(format!("0xtrade_vbc_{}_id_default", type_name)),
            __typename: "TradeVaultBalanceChange".to_string(),
            amount: SgBytes(F1.as_hex()),
            new_vault_balance: SgBytes(F5.as_hex()),
            old_vault_balance: SgBytes(F4.as_hex()),
            vault: default_sg_vault_balance_change_vault(),
            timestamp: SgBigInt("1600000100".to_string()),
            transaction: default_sg_transaction(),
            raindex: default_sg_raindex(),
            trade: default_sg_trade_ref(),
        }
    }

    fn default_sg_trade_event() -> SgTradeEvent {
        SgTradeEvent {
            id: SgBytes("0xevent_id_default".to_string()),
            __typename: "TakeOrder".to_string(),
            transaction: default_sg_transaction(),
            sender: SgBytes("0xsender_address_default".to_string()),
        }
    }

    fn default_sg_trade_struct_partial_order() -> SgTradeStructPartialOrder {
        SgTradeStructPartialOrder {
            id: SgBytes("0xorder_id_for_trade_default".to_string()),
            order_hash: SgBytes("0xorder_hash_for_trade_default".to_string()),
            owner: SgBytes("0xowner_address_default".to_string()),
        }
    }

    fn default_sg_trade() -> SgTrade {
        SgTrade {
            id: SgBytes("0xtrade_id_default".to_string()),
            trade_event: default_sg_trade_event(),
            output_vault_balance_change: default_sg_trade_vault_balance_change("output"),
            order: default_sg_trade_struct_partial_order(),
            input_vault_balance_change: default_sg_trade_vault_balance_change("input"),
            timestamp: SgBigInt("1600000200".to_string()),
            raindex: default_sg_raindex(),
        }
    }

    fn sg_trade_with_id_timestamp(id: &str, timestamp: &str) -> SgTrade {
        let mut trade = default_sg_trade();
        trade.id = SgBytes(id.to_string());
        trade.timestamp = SgBigInt(timestamp.to_string());
        trade
    }

    fn assert_sg_trade_eq(actual: &SgTrade, expected: &SgTrade) {
        assert_eq!(actual.id, expected.id, "Trade ID mismatch");
        assert_eq!(
            actual.timestamp, expected.timestamp,
            "Trade timestamp mismatch"
        );
        assert_eq!(
            actual.raindex.id, expected.raindex.id,
            "Trade raindex ID mismatch"
        );

        // Assert TradeEvent
        assert_eq!(actual.trade_event.sender, expected.trade_event.sender);
        assert_eq!(
            actual.trade_event.transaction.id,
            expected.trade_event.transaction.id
        );

        // Assert SgTradeStructPartialOrder
        assert_eq!(actual.order.id, expected.order.id);
        assert_eq!(actual.order.order_hash, expected.order.order_hash);

        // Assert OutputVaultBalanceChange
        let actual_ovbc = &actual.output_vault_balance_change;
        let expected_ovbc = &expected.output_vault_balance_change;
        assert_eq!(actual_ovbc.id, expected_ovbc.id);
        assert_eq!(actual_ovbc.amount, expected_ovbc.amount);
        assert_eq!(actual_ovbc.vault.id, expected_ovbc.vault.id);
        assert_eq!(actual_ovbc.transaction.id, expected_ovbc.transaction.id);

        // Assert InputVaultBalanceChange
        let actual_ivbc = &actual.input_vault_balance_change;
        let expected_ivbc = &expected.input_vault_balance_change;
        assert_eq!(actual_ivbc.id, expected_ivbc.id);
        assert_eq!(actual_ivbc.amount, expected_ivbc.amount);
        assert_eq!(actual_ivbc.vault.id, expected_ivbc.vault.id);
        assert_eq!(actual_ivbc.transaction.id, expected_ivbc.transaction.id);
    }

    #[tokio::test]
    async fn test_order_trade_detail_found() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let trade_id_str = "0xtrade123";
        let trade_id = Id::new(trade_id_str);
        let expected_trade = default_sg_trade();

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"trade": expected_trade}}));
        });

        let result = client.order_trade_detail(trade_id).await;
        assert!(result.is_ok(), "Result should be Ok");
        let trade = result.unwrap();
        assert_sg_trade_eq(&trade, &expected_trade);
    }

    #[tokio::test]
    async fn test_order_trade_detail_not_found() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let trade_id = Id::new("0xnotfoundtrade");

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(json!({"data": {"trade": null}}));
        });

        let result = client.order_trade_detail(trade_id).await;
        assert!(matches!(result, Err(RaindexSubgraphClientError::Empty)));
    }

    #[tokio::test]
    async fn test_order_trade_detail_network_error() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let trade_id = Id::new("0xtrade123error");

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let result = client.order_trade_detail(trade_id).await;
        assert!(matches!(
            result,
            Err(RaindexSubgraphClientError::CynicClientError(_))
        ));
    }

    #[tokio::test]
    async fn test_order_trades_list_found_no_timestamp() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let order_id = Id::new("0xorder1");
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };
        let expected_trades = vec![default_sg_trade(), default_sg_trade()];

        sg_server.mock(|when, then| {
            when.method(POST).path("/").body_contains(order_id.inner());
            then.status(200)
                .json_body(json!({"data": {"trades": expected_trades}}));
        });

        let result = client
            .order_trades_list(order_id, pagination_args, None, None)
            .await;
        assert!(result.is_ok());
        let trades = result.unwrap();
        assert_eq!(trades.len(), expected_trades.len());
        for (actual, expected) in trades.iter().zip(expected_trades.iter()) {
            assert_sg_trade_eq(actual, expected);
        }
    }

    #[tokio::test]
    async fn test_order_trades_list_empty_result() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let order_id = Id::new("0xorder_empty");
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };

        sg_server.mock(|when, then| {
            when.method(POST).path("/").body_contains(order_id.inner());
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client
            .order_trades_list(order_id, pagination_args, None, None)
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_order_trades_list_pagination_second_page() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let order_id = Id::new("0xorder_page2");
        let page_size = 10;
        let pagination_args = SgPaginationArgs { page: 2, page_size };
        let expected_skip = page_size;
        let expected_trades = vec![default_sg_trade()];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(order_id.inner())
                .body_contains(format!("\"skip\":{}", expected_skip))
                .body_contains(format!("\"first\":{}", page_size));
            then.status(200)
                .json_body(json!({"data": {"trades": expected_trades}}));
        });

        let result = client
            .order_trades_list(order_id, pagination_args, None, None)
            .await;
        assert!(result.is_ok());
        let trades = result.unwrap();
        assert_eq!(trades.len(), expected_trades.len());
    }

    #[tokio::test]
    async fn test_order_trades_list_network_error() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let order_id = Id::new("0xorder_network_error");
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };

        sg_server.mock(|when, then| {
            when.method(POST).path("/").body_contains(order_id.inner());
            then.status(500);
        });

        let result = client
            .order_trades_list(order_id, pagination_args, None, None)
            .await;
        assert!(matches!(
            result,
            Err(RaindexSubgraphClientError::CynicClientError(_))
        ));
    }

    #[tokio::test]
    async fn test_order_trades_list_all_multiple_pages() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let order_id = Id::new("0xorder_multi_page");
        let trades_page1: Vec<SgTrade> = (0..ALL_PAGES_QUERY_PAGE_SIZE)
            .map(|_| default_sg_trade())
            .collect();
        let trades_page2: Vec<SgTrade> = (0..50).map(|_| default_sg_trade()).collect();

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(order_id.inner())
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": trades_page1}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(order_id.inner())
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200)
                .json_body(json!({"data": {"trades": trades_page2}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(order_id.inner())
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE * 2));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client.order_trades_list_all(order_id, None, None).await;
        assert!(result.is_ok());
        let trades = result.unwrap();
        assert_eq!(
            trades.len(),
            ALL_PAGES_QUERY_PAGE_SIZE as usize + trades_page2.len()
        );
    }

    #[tokio::test]
    async fn test_order_trades_list_all_single_page() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let order_id = Id::new("0xorder_single_page");
        let trades_page1: Vec<SgTrade> = (0..50).map(|_| default_sg_trade()).collect();

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(order_id.inner())
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": trades_page1}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(order_id.inner())
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client.order_trades_list_all(order_id, None, None).await;
        assert!(result.is_ok());
        let trades = result.unwrap();
        assert_eq!(trades.len(), trades_page1.len());
    }

    #[tokio::test]
    async fn test_order_trades_list_all_no_trades() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let order_id = Id::new("0xorder_all_no_trades");

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(order_id.inner())
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client.order_trades_list_all(order_id, None, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_trades_by_transaction_found() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let tx_id = "0xtx_abc123".to_string();
        let expected_trades = vec![default_sg_trade(), default_sg_trade()];

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"trades": expected_trades}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client.trades_by_transaction(tx_id, None).await;
        assert!(result.is_ok());
        let trades = result.unwrap();
        assert_eq!(trades.len(), expected_trades.len());
        for (actual, expected) in trades.iter().zip(expected_trades.iter()) {
            assert_sg_trade_eq(actual, expected);
        }
    }

    #[tokio::test]
    async fn test_trades_by_transaction_sorts_by_timestamp_then_id() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let tx_id = "0xtx_sorted".to_string();
        let older = sg_trade_with_id_timestamp("0xtrade_c", "100");
        let same_timestamp_second = sg_trade_with_id_timestamp("0xtrade_b", "200");
        let same_timestamp_first = sg_trade_with_id_timestamp("0xtrade_a", "200");

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(json!({
                "data": {
                    "trades": [
                        older,
                        same_timestamp_second,
                        same_timestamp_first
                    ]
                }
            }));
        });
        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let trades = client.trades_by_transaction(tx_id, None).await.unwrap();
        let ids: Vec<_> = trades.into_iter().map(|trade| trade.id.0).collect();
        assert_eq!(ids, vec!["0xtrade_a", "0xtrade_b", "0xtrade_c"]);
    }

    #[tokio::test]
    async fn test_trades_by_transaction_empty() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let tx_id = "0xtx_empty".to_string();

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client.trades_by_transaction(tx_id, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_trades_by_transaction_with_raindex_filter() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let tx_id = "0xtx_raindex_filtered".to_string();
        let raindex = "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string();

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"raindexIn\":[\"{}\"]", raindex));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client
            .trades_by_transaction(tx_id, Some(vec![raindex]))
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_trades_by_transaction_multiple_pages() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let tx_id = "0xtx_multi_page".to_string();
        let trades_page1: Vec<SgTrade> = (0..ALL_PAGES_QUERY_PAGE_SIZE)
            .map(|_| default_sg_trade())
            .collect();
        let trades_page2: Vec<SgTrade> = (0..50).map(|_| default_sg_trade()).collect();

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": trades_page1}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200)
                .json_body(json!({"data": {"trades": trades_page2}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE * 2));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client.trades_by_transaction(tx_id, None).await;
        assert!(result.is_ok());
        let trades = result.unwrap();
        assert_eq!(
            trades.len(),
            ALL_PAGES_QUERY_PAGE_SIZE as usize + trades_page2.len()
        );
    }

    #[tokio::test]
    async fn test_trades_by_transaction_network_error() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let tx_id = "0xtx_error".to_string();

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let result = client.trades_by_transaction(tx_id, None).await;
        assert!(matches!(
            result,
            Err(RaindexSubgraphClientError::CynicClientError(_))
        ));
    }

    #[tokio::test]
    async fn test_order_trades_list_all_network_error_on_page() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let order_id = Id::new("0xorder_all_network_error");
        let trades_page1: Vec<SgTrade> = (0..ALL_PAGES_QUERY_PAGE_SIZE)
            .map(|_| default_sg_trade())
            .collect();

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(order_id.inner())
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": trades_page1}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(order_id.inner())
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(500);
        });

        let result = client.order_trades_list_all(order_id, None, None).await;
        assert!(matches!(
            result,
            Err(RaindexSubgraphClientError::CynicClientError(_))
        ));
    }

    #[tokio::test]
    async fn test_trades_by_owner_found() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let owner = "0xowner_abc123".to_string();
        let expected_trades = vec![default_sg_trade(), default_sg_trade()];
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"trades": expected_trades}}));
        });

        let result = client
            .trades_by_owner(owner, None, None, None, pagination_args)
            .await;
        assert!(result.is_ok());
        let trades = result.unwrap();
        assert_eq!(trades.len(), expected_trades.len());
        for (actual, expected) in trades.iter().zip(expected_trades.iter()) {
            assert_sg_trade_eq(actual, expected);
        }
    }

    #[tokio::test]
    async fn test_trades_by_owner_empty() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let owner = "0xowner_empty".to_string();
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client
            .trades_by_owner(owner, None, None, None, pagination_args)
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_trades_by_owner_with_raindex_filter() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let owner = "0xowner_filtered".to_string();
        let raindex = "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string();
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"raindexIn\":[\"{}\"]", raindex));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client
            .trades_by_owner(owner, None, None, Some(vec![raindex]), pagination_args)
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_trades_by_owner_all_multiple_pages() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let owner = "0xowner_multi_page".to_string();
        let trades_page1: Vec<SgTrade> = (0..ALL_PAGES_QUERY_PAGE_SIZE)
            .map(|_| default_sg_trade())
            .collect();
        let trades_page2: Vec<SgTrade> = (0..50).map(|_| default_sg_trade()).collect();

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": trades_page1}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200)
                .json_body(json!({"data": {"trades": trades_page2}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE * 2));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client.trades_by_owner_all(owner, None, None, None).await;
        assert!(result.is_ok());
        let trades = result.unwrap();
        assert_eq!(
            trades.len(),
            ALL_PAGES_QUERY_PAGE_SIZE as usize + trades_page2.len()
        );
    }

    #[tokio::test]
    async fn test_trades_by_owner_network_error() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let owner = "0xowner_error".to_string();
        let pagination_args = SgPaginationArgs {
            page: 1,
            page_size: 10,
        };

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let result = client
            .trades_by_owner(owner, None, None, None, pagination_args)
            .await;
        assert!(matches!(
            result,
            Err(RaindexSubgraphClientError::CynicClientError(_))
        ));
    }
}
