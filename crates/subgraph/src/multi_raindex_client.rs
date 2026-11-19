use crate::{
    types::common::{
        SgErc20WithSubgraphName, SgOrderWithSubgraphName, SgOrdersListFilterArgs,
        SgTradeWithSubgraphName, SgTradesListQueryFilters, SgVaultWithSubgraphName,
        SgVaultsListFilterArgs,
    },
    RaindexSubgraphClient, RaindexSubgraphClientError, SgPaginationArgs,
};
use futures::future::join_all;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use wasm_bindgen_utils::{impl_wasm_traits, prelude::*};

#[derive(Debug, Clone, Serialize, Deserialize, Tsify)]
pub struct MultiSubgraphArgs {
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub url: Url,
    pub name: String,
}
impl_wasm_traits!(MultiSubgraphArgs);

pub struct MultiRaindexSubgraphClient {
    subgraphs: Vec<MultiSubgraphArgs>,
}

fn sort_trades(trades: &mut [SgTradeWithSubgraphName]) {
    trades.sort_by(|a, b| {
        let a_timestamp = a.trade.timestamp.0.parse::<i64>().unwrap_or(0);
        let b_timestamp = b.trade.timestamp.0.parse::<i64>().unwrap_or(0);
        b_timestamp
            .cmp(&a_timestamp)
            .then_with(|| a.trade.id.0.cmp(&b.trade.id.0))
    });
}

impl MultiRaindexSubgraphClient {
    pub fn new(subgraphs: Vec<MultiSubgraphArgs>) -> Self {
        Self { subgraphs }
    }

    fn get_raindex_subgraph_client(&self, url: Url) -> RaindexSubgraphClient {
        RaindexSubgraphClient::new(url)
    }

    pub async fn orders_list(
        &self,
        filter_args: SgOrdersListFilterArgs,
        pagination_args: SgPaginationArgs,
    ) -> Result<Vec<SgOrderWithSubgraphName>, RaindexSubgraphClientError> {
        let futures = self.subgraphs.iter().map(|subgraph| {
            let url = subgraph.url.clone();
            let filter_args = filter_args.clone();
            let pagination_args = pagination_args.clone();
            async move {
                let client = self.get_raindex_subgraph_client(url);
                let orders = client.orders_list(filter_args, pagination_args).await?;
                let wrapped_orders: Vec<SgOrderWithSubgraphName> = orders
                    .into_iter()
                    .map(|order| SgOrderWithSubgraphName {
                        order,
                        subgraph_name: subgraph.name.clone(),
                    })
                    .collect();
                Ok::<_, RaindexSubgraphClientError>(wrapped_orders)
            }
        });

        let results = join_all(futures).await;

        let mut all_orders = Vec::new();
        let mut last_error = None;
        for result in results {
            match result {
                Ok(items) => all_orders.extend(items),
                Err(e) => last_error = Some(e),
            }
        }
        if all_orders.is_empty() {
            if let Some(e) = last_error {
                return Err(e);
            }
        }

        all_orders.sort_by(|a, b| {
            let a_timestamp = a.order.timestamp_added.0.parse::<i64>().unwrap_or(0);
            let b_timestamp = b.order.timestamp_added.0.parse::<i64>().unwrap_or(0);
            b_timestamp.cmp(&a_timestamp)
        });

        Ok(all_orders)
    }

    pub async fn orders_count(
        &self,
        filter_args: SgOrdersListFilterArgs,
    ) -> Result<u32, RaindexSubgraphClientError> {
        let futures = self.subgraphs.iter().map(|subgraph| {
            let url = subgraph.url.clone();
            let filter_args = filter_args.clone();
            async move {
                let client = self.get_raindex_subgraph_client(url);
                client.orders_count(filter_args).await
            }
        });

        let results = join_all(futures).await;
        let mut total: u32 = 0;
        for result in results {
            total += result?;
        }
        Ok(total)
    }

    async fn vaults_list_with_policy(
        &self,
        filter_args: SgVaultsListFilterArgs,
        pagination_args: SgPaginationArgs,
        allow_partial: bool,
    ) -> Result<Vec<SgVaultWithSubgraphName>, RaindexSubgraphClientError> {
        let futures = self.subgraphs.iter().map(|subgraph| {
            let url = subgraph.url.clone();
            let filter_args = filter_args.clone();
            let pagination_args = pagination_args.clone();
            async move {
                let client = self.get_raindex_subgraph_client(url);
                let vaults = client.vaults_list(filter_args, pagination_args).await?;
                let wrapped_vaults: Vec<SgVaultWithSubgraphName> = vaults
                    .into_iter()
                    .map(|vault| SgVaultWithSubgraphName {
                        vault,
                        subgraph_name: subgraph.name.clone(),
                    })
                    .collect();
                Ok::<_, RaindexSubgraphClientError>(wrapped_vaults)
            }
        });

        let results = join_all(futures).await;

        let mut all_vaults = Vec::new();
        let mut last_error = None;
        for result in results {
            match result {
                Ok(items) => all_vaults.extend(items),
                Err(e) => last_error = Some(e),
            }
        }
        if (all_vaults.is_empty() || !allow_partial) && last_error.is_some() {
            if let Some(e) = last_error {
                return Err(e);
            }
        }

        Ok(all_vaults)
    }

    pub async fn vaults_list(
        &self,
        filter_args: SgVaultsListFilterArgs,
        pagination_args: SgPaginationArgs,
    ) -> Result<Vec<SgVaultWithSubgraphName>, RaindexSubgraphClientError> {
        self.vaults_list_with_policy(filter_args, pagination_args, true)
            .await
    }

    pub async fn vaults_list_strict(
        &self,
        filter_args: SgVaultsListFilterArgs,
        pagination_args: SgPaginationArgs,
    ) -> Result<Vec<SgVaultWithSubgraphName>, RaindexSubgraphClientError> {
        self.vaults_list_with_policy(filter_args, pagination_args, false)
            .await
    }

    pub async fn vaults_count(
        &self,
        filter_args: SgVaultsListFilterArgs,
    ) -> Result<u32, RaindexSubgraphClientError> {
        let futures = self.subgraphs.iter().map(|subgraph| {
            let url = subgraph.url.clone();
            let filter_args = filter_args.clone();
            async move {
                let client = self.get_raindex_subgraph_client(url);
                client.vaults_count(filter_args).await
            }
        });

        let results = join_all(futures).await;
        let mut total: u32 = 0;
        for result in results {
            total += result?;
        }
        Ok(total)
    }

    pub async fn trades_by_transaction(
        &self,
        tx_id: String,
        raindex_in: Option<Vec<String>>,
    ) -> Result<Vec<SgTradeWithSubgraphName>, RaindexSubgraphClientError> {
        let futures = self.subgraphs.iter().map(|subgraph| {
            let url = subgraph.url.clone();
            let subgraph_name = subgraph.name.clone();
            let tx_id = tx_id.clone();
            let raindex_in = raindex_in.clone();
            async move {
                let client = self.get_raindex_subgraph_client(url.clone());
                let result = client
                    .trades_by_transaction(tx_id, raindex_in)
                    .await
                    .map(|trades| {
                        trades
                            .into_iter()
                            .map(|trade| SgTradeWithSubgraphName {
                                trade,
                                subgraph_name: subgraph_name.clone(),
                            })
                            .collect::<Vec<_>>()
                    });
                (subgraph_name, url, result)
            }
        });

        let results = join_all(futures).await;

        let mut all_trades = Vec::new();
        let mut last_error = None;
        let mut any_success = false;
        for (subgraph_name, url, result) in results {
            match result {
                Ok(items) => {
                    any_success = true;
                    all_trades.extend(items);
                }
                Err(e) => {
                    tracing::warn!(
                        subgraph = %subgraph_name,
                        url = %url,
                        error = %e,
                        "failed to fetch transaction trades from subgraph"
                    );
                    last_error = Some(e);
                }
            }
        }
        if !any_success {
            if let Some(e) = last_error {
                return Err(e);
            }
        }

        Ok(all_trades)
    }

    pub async fn trades_by_owner(
        &self,
        owner: String,
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
        raindex_in: Option<Vec<String>>,
    ) -> Vec<SgTradeWithSubgraphName> {
        let futures = self.subgraphs.iter().map(|subgraph| {
            let url = subgraph.url.clone();
            let owner = owner.clone();
            let raindex_in = raindex_in.clone();
            async move {
                let client = self.get_raindex_subgraph_client(url);
                let trades = client
                    .trades_by_owner_all(owner, start_timestamp, end_timestamp, raindex_in)
                    .await?;
                let wrapped_trades: Vec<SgTradeWithSubgraphName> = trades
                    .into_iter()
                    .map(|trade| SgTradeWithSubgraphName {
                        trade,
                        subgraph_name: subgraph.name.clone(),
                    })
                    .collect();
                Ok::<_, RaindexSubgraphClientError>(wrapped_trades)
            }
        });

        let results = join_all(futures).await;

        results
            .into_iter()
            .filter_map(Result::ok)
            .flatten()
            .collect()
    }

    /// Fetches the general filtered trades list across every configured subgraph.
    ///
    /// This backs the SDK-level `RaindexClient.getTrades` API. Order-specific trade
    /// history still uses `RaindexSubgraphClient::order_trades_list`.
    pub async fn trades_list(
        &self,
        filters: SgTradesListQueryFilters,
        pagination_args: SgPaginationArgs,
    ) -> Result<Vec<SgTradeWithSubgraphName>, RaindexSubgraphClientError> {
        let futures = self.subgraphs.iter().map(|subgraph| {
            let url = subgraph.url.clone();
            let subgraph_name = subgraph.name.clone();
            let filters = filters.clone();
            let pagination_args = pagination_args.clone();
            async move {
                let client = self.get_raindex_subgraph_client(url.clone());
                let result = client
                    .trades_list(filters, pagination_args)
                    .await
                    .map(|trades| {
                        trades
                            .into_iter()
                            .map(|trade| SgTradeWithSubgraphName {
                                trade,
                                subgraph_name: subgraph_name.clone(),
                            })
                            .collect::<Vec<_>>()
                    });
                (subgraph_name, url, result)
            }
        });

        let results = join_all(futures).await;
        let mut all_trades = Vec::new();
        let mut last_error = None;
        let mut any_success = false;
        for (subgraph_name, url, result) in results {
            match result {
                Ok(items) => {
                    any_success = true;
                    all_trades.extend(items);
                }
                Err(e) => {
                    tracing::warn!(
                        subgraph = %subgraph_name,
                        url = %url,
                        error = %e,
                        "failed to fetch trades from subgraph"
                    );
                    last_error = Some(e);
                }
            }
        }
        if !any_success {
            if let Some(e) = last_error {
                return Err(e);
            }
        }

        sort_trades(&mut all_trades);
        Ok(all_trades)
    }

    /// Fetches all filtered trades across every configured subgraph.
    ///
    /// This is used when callers need to merge and paginate across multiple data
    /// sources after fetching. Order-specific trade history still uses
    /// `RaindexSubgraphClient::order_trades_list`.
    pub async fn trades_list_all(
        &self,
        filters: SgTradesListQueryFilters,
    ) -> Result<Vec<SgTradeWithSubgraphName>, RaindexSubgraphClientError> {
        let futures = self.subgraphs.iter().map(|subgraph| {
            let url = subgraph.url.clone();
            let subgraph_name = subgraph.name.clone();
            let filters = filters.clone();
            async move {
                let client = self.get_raindex_subgraph_client(url.clone());
                let result = client.trades_list_all(filters).await.map(|trades| {
                    trades
                        .into_iter()
                        .map(|trade| SgTradeWithSubgraphName {
                            trade,
                            subgraph_name: subgraph_name.clone(),
                        })
                        .collect::<Vec<_>>()
                });
                (subgraph_name, url, result)
            }
        });

        let results = join_all(futures).await;
        let mut all_trades = Vec::new();
        let mut last_error = None;
        let mut any_success = false;
        for (subgraph_name, url, result) in results {
            match result {
                Ok(items) => {
                    any_success = true;
                    all_trades.extend(items);
                }
                Err(e) => {
                    tracing::warn!(
                        subgraph = %subgraph_name,
                        url = %url,
                        error = %e,
                        "failed to fetch all trades from subgraph"
                    );
                    last_error = Some(e);
                }
            }
        }
        if !any_success {
            if let Some(e) = last_error {
                return Err(e);
            }
        }

        sort_trades(&mut all_trades);
        Ok(all_trades)
    }

    pub async fn trades_count(
        &self,
        filters: SgTradesListQueryFilters,
    ) -> Result<u32, RaindexSubgraphClientError> {
        let futures = self.subgraphs.iter().map(|subgraph| {
            let url = subgraph.url.clone();
            let subgraph_name = subgraph.name.clone();
            let filters = filters.clone();
            async move {
                let client = self.get_raindex_subgraph_client(url.clone());
                (subgraph_name, url, client.trades_count(filters).await)
            }
        });

        let results = join_all(futures).await;
        let mut total: u32 = 0;
        let mut last_error = None;
        let mut any_success = false;
        for (subgraph_name, url, result) in results {
            match result {
                Ok(count) => {
                    any_success = true;
                    total += count;
                }
                Err(e) => {
                    tracing::warn!(
                        subgraph = %subgraph_name,
                        url = %url,
                        error = %e,
                        "failed to count trades from subgraph"
                    );
                    last_error = Some(e);
                }
            }
        }
        if !any_success {
            if let Some(e) = last_error {
                return Err(e);
            }
        }
        Ok(total)
    }

    pub async fn tokens_list(
        &self,
    ) -> Result<Vec<SgErc20WithSubgraphName>, RaindexSubgraphClientError> {
        let futures = self.subgraphs.iter().map(|subgraph| {
            let url = subgraph.url.clone();
            async move {
                let client = self.get_raindex_subgraph_client(url);
                let tokens = client.tokens_list_all().await?;
                let wrapped_tokens: Vec<SgErc20WithSubgraphName> = tokens
                    .into_iter()
                    .map(|token| SgErc20WithSubgraphName {
                        token,
                        subgraph_name: subgraph.name.clone(),
                    })
                    .collect();
                Ok::<_, RaindexSubgraphClientError>(wrapped_tokens)
            }
        });

        let results = join_all(futures).await;

        let mut all_tokens = Vec::new();
        let mut last_error = None;
        for result in results {
            match result {
                Ok(items) => all_tokens.extend(items),
                Err(e) => last_error = Some(e),
            }
        }
        if all_tokens.is_empty() {
            if let Some(e) = last_error {
                return Err(e);
            }
        }

        Ok(all_tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cynic_client::CynicClientError;
    use crate::raindex_client::ALL_PAGES_QUERY_PAGE_SIZE;
    use crate::types::common::{
        SgBigInt, SgBytes, SgErc20, SgOrder, SgOrdersListFilterArgs, SgRaindex, SgTrade,
        SgTradeEvent, SgTradeEventTypename, SgTradeRef, SgTradeStructPartialOrder,
        SgTradeVaultBalanceChange, SgTradesListQueryFilters, SgTransaction, SgVault,
        SgVaultBalanceChangeVault,
    };
    use crate::utils::float::*;
    use httpmock::prelude::*;
    use reqwest::Url;
    use serde_json::json;

    fn sample_sg_order(id_suffix: &str, timestamp: &str) -> SgOrder {
        SgOrder {
            id: SgBytes(format!("0xorder_id_{}", id_suffix)),
            order_bytes: SgBytes("0x00".to_string()),
            order_hash: SgBytes(format!("0xhash_{}", id_suffix)),
            owner: SgBytes("0xdefault_owner".to_string()),
            outputs: vec![],
            inputs: vec![],
            raindex: SgRaindex {
                id: SgBytes("0xdefault_raindex_id".to_string()),
            },
            active: true,
            timestamp_added: SgBigInt(timestamp.to_string()),
            meta: None,
            add_events: vec![],
            trades: vec![],
            remove_events: vec![],
        }
    }

    fn default_filter_args() -> SgOrdersListFilterArgs {
        SgOrdersListFilterArgs {
            owners: vec![],
            active: None,
            order_hash: None,
            tokens: None,
            raindexes: vec![],
            has_positive_output_vault_balance: None,
        }
    }

    fn default_pagination_args() -> SgPaginationArgs {
        SgPaginationArgs {
            page: 1,
            page_size: 10,
        }
    }

    #[tokio::test]
    async fn test_orders_list_no_subgraphs() {
        let client = MultiRaindexSubgraphClient::new(vec![]);
        let result = client
            .orders_list(default_filter_args(), default_pagination_args())
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_orders_list_one_subgraph_returns_orders() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "subgraph_alpha";

        let order1_s1 = sample_sg_order("s1_1", "100");
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"orders": [order1_s1]}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg1_url,
            name: sg1_name.to_string(),
        }]);

        let orders = client
            .orders_list(default_filter_args(), default_pagination_args())
            .await
            .unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].order.id, order1_s1.id);
        assert_eq!(orders[0].subgraph_name, sg1_name);
    }

    #[tokio::test]
    async fn test_orders_list_multiple_subgraphs_merge_and_sort() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_one";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_two";

        let order_a_s1 = sample_sg_order("s1_A", "100");
        let order_b_s2 = sample_sg_order("s2_B", "200");
        let order_c_s2 = sample_sg_order("s2_C", "50");

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"orders": [order_a_s1]}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"orders": [order_b_s2, order_c_s2]}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);

        let orders = client
            .orders_list(default_filter_args(), default_pagination_args())
            .await
            .unwrap();

        assert_eq!(orders.len(), 3);
        assert_eq!(orders[0].order.id, order_b_s2.id);
        assert_eq!(orders[0].subgraph_name, sg2_name);
        assert_eq!(orders[1].order.id, order_a_s1.id);
        assert_eq!(orders[1].subgraph_name, sg1_name);
        assert_eq!(orders[2].order.id, order_c_s2.id);
        assert_eq!(orders[2].subgraph_name, sg2_name);
    }

    #[tokio::test]
    async fn test_orders_list_multiple_subgraphs_some_empty() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_one";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_two_empty";

        let order_a_s1 = sample_sg_order("s1_A", "100");
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"orders": [order_a_s1]}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(json!({"data": {"orders": []}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);
        let orders = client
            .orders_list(default_filter_args(), default_pagination_args())
            .await
            .unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].order.id, order_a_s1.id);
        assert_eq!(orders[0].subgraph_name, sg1_name);
    }

    #[tokio::test]
    async fn test_orders_list_one_subgraph_errors_others_succeed() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_one_ok";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_two_error";

        let order_a_s1 = sample_sg_order("s1_A", "100");
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"orders": [order_a_s1]}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);
        let orders = client
            .orders_list(default_filter_args(), default_pagination_args())
            .await
            .unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].order.id, order_a_s1.id);
        assert_eq!(orders[0].subgraph_name, sg1_name);
    }

    #[tokio::test]
    async fn test_orders_list_all_subgraphs_error() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_one_err";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_two_err";

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);
        let result = client
            .orders_list(default_filter_args(), default_pagination_args())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_orders_list_rate_limited_returns_http_429_error() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_rate_limited";

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(429).body("rate limit exceeded");
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg1_url,
            name: sg1_name.to_string(),
        }]);
        let result = client
            .orders_list(default_filter_args(), default_pagination_args())
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            RaindexSubgraphClientError::CynicClientError(CynicClientError::HttpError {
                status,
                body,
            }) => {
                assert_eq!(status, 429);
                assert_eq!(body, "rate limit exceeded");
            }
            other => panic!("Expected HttpError with status 429, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_orders_list_rate_limited_with_one_successful_subgraph() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_ok";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_rate_limited";

        let order_a = sample_sg_order("s1_A", "100");
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"orders": [order_a]}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(429).body("rate limit exceeded");
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);
        let orders = client
            .orders_list(default_filter_args(), default_pagination_args())
            .await
            .unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].order.id, order_a.id);
        assert_eq!(orders[0].subgraph_name, sg1_name);
    }

    #[tokio::test]
    async fn test_orders_list_invalid_timestamp_string_sorts_as_zero() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_one";

        let order_a = sample_sg_order("A", "100");
        let order_b = sample_sg_order("B", "invalid_timestamp");
        let order_c = sample_sg_order("C", "50");

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"orders": [order_a, order_b, order_c]}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg1_url,
            name: sg1_name.to_string(),
        }]);
        let orders = client
            .orders_list(default_filter_args(), default_pagination_args())
            .await
            .unwrap();
        assert_eq!(orders.len(), 3);
        assert_eq!(orders[0].order.id, order_a.id);
        assert_eq!(orders[1].order.id, order_c.id);
        assert_eq!(orders[2].order.id, order_b.id);
    }

    #[tokio::test]
    async fn test_orders_list_sorts_various_timestamps_correctly() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_one";

        let order_a = sample_sg_order("A", "0");
        let order_b = sample_sg_order("B", "9999999999999");
        let order_c = sample_sg_order("C", "1");
        let order_d = sample_sg_order("D", "-10");
        let order_e = sample_sg_order("E", "another_invalid");

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(
                json!({"data": {"orders": [order_a, order_b, order_c, order_d, order_e]}}),
            );
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg1_url,
            name: sg1_name.to_string(),
        }]);
        let orders = client
            .orders_list(default_filter_args(), default_pagination_args())
            .await
            .unwrap();
        assert_eq!(orders.len(), 5);

        assert_eq!(orders[0].order.id, order_b.id);
        assert_eq!(orders[1].order.id, order_c.id);

        let ids_for_ts_zero: Vec<&SgBytes> = orders
            .iter()
            .filter(|o| o.order.timestamp_added.0.parse::<i64>().unwrap_or(0) == 0)
            .map(|o| &o.order.id)
            .collect();
        assert!(ids_for_ts_zero.contains(&&order_a.id));
        assert!(ids_for_ts_zero.contains(&&order_e.id));
        assert_eq!(orders[4].order.id, order_d.id);

        let order_ids_sorted: Vec<SgBytes> = orders.into_iter().map(|o| o.order.id).collect();
        assert_eq!(order_ids_sorted[0], order_b.id);
        assert_eq!(order_ids_sorted[1], order_c.id);

        assert!(
            (order_ids_sorted[2] == order_a.id && order_ids_sorted[3] == order_e.id)
                || (order_ids_sorted[2] == order_e.id && order_ids_sorted[3] == order_a.id)
        );
        assert_eq!(order_ids_sorted[4], order_d.id);
    }

    #[tokio::test]
    async fn test_orders_count_no_subgraphs() {
        let client = MultiRaindexSubgraphClient::new(vec![]);
        let count = client.orders_count(default_filter_args()).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_orders_count_one_subgraph() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();

        let orders: Vec<_> = (0..3)
            .map(|i| sample_sg_order(&format!("c_{}", i), "100"))
            .collect();
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"orders": orders}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg1_url,
            name: "sg_one".to_string(),
        }]);

        let count = client.orders_count(default_filter_args()).await.unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_orders_count_multiple_subgraphs_sums() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        let orders_s1: Vec<_> = (0..2)
            .map(|i| sample_sg_order(&format!("s1_{}", i), "100"))
            .collect();
        let orders_s2: Vec<_> = (0..5)
            .map(|i| sample_sg_order(&format!("s2_{}", i), "200"))
            .collect();

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"orders": orders_s1}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"orders": orders_s2}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two".to_string(),
            },
        ]);

        let count = client.orders_count(default_filter_args()).await.unwrap();
        assert_eq!(count, 7);
    }

    #[tokio::test]
    async fn test_orders_count_one_subgraph_errors_propagates() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        let orders_s1: Vec<_> = (0..4)
            .map(|i| sample_sg_order(&format!("s1_{}", i), "100"))
            .collect();
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"orders": orders_s1}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two_err".to_string(),
            },
        ]);

        let result = client.orders_count(default_filter_args()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_orders_count_all_subgraphs_error() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one_err".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two_err".to_string(),
            },
        ]);

        let result = client.orders_count(default_filter_args()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_orders_count_pagination_boundary() {
        use crate::raindex_client::ALL_PAGES_QUERY_PAGE_SIZE;

        let server = MockServer::start_async().await;
        let sg_url = Url::parse(&server.url("")).unwrap();

        let page1_orders: Vec<_> = (0..ALL_PAGES_QUERY_PAGE_SIZE)
            .map(|i| sample_sg_order(&format!("p1_{}", i), "100"))
            .collect();
        let page2_orders: Vec<_> = (0..10)
            .map(|i| sample_sg_order(&format!("p2_{}", i), "100"))
            .collect();

        server.mock(|when, then| {
            when.method(POST).path("/").body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"orders": page1_orders}}));
        });
        server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200)
                .json_body(json!({"data": {"orders": page2_orders}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg_url,
            name: "sg_one".to_string(),
        }]);

        let count = client.orders_count(default_filter_args()).await.unwrap();
        assert_eq!(count, ALL_PAGES_QUERY_PAGE_SIZE as u32 + 10);
    }

    fn default_sg_transaction() -> SgTransaction {
        SgTransaction {
            id: SgBytes("0xtransaction_id_default".to_string()),
            from: SgBytes("0xfrom_address_default".to_string()),
            block_number: SgBigInt("100".to_string()),
            timestamp: SgBigInt("1600000000".to_string()),
        }
    }

    fn default_sg_trade_erc20() -> SgErc20 {
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
            token: default_sg_trade_erc20(),
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
            raindex: SgRaindex {
                id: SgBytes("0xraindex_id_default".to_string()),
            },
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
            raindex: SgRaindex {
                id: SgBytes("0xraindex_id_default".to_string()),
            },
        }
    }

    fn sample_sg_trade(id: &str, timestamp: &str) -> SgTrade {
        SgTrade {
            id: SgBytes(id.to_string()),
            timestamp: SgBigInt(timestamp.to_string()),
            ..default_sg_trade()
        }
    }

    fn default_trade_filters() -> SgTradesListQueryFilters {
        SgTradesListQueryFilters::default()
    }

    #[tokio::test]
    async fn test_trades_list_no_subgraphs() {
        let client = MultiRaindexSubgraphClient::new(vec![]);
        let trades = client
            .trades_list(
                default_trade_filters(),
                SgPaginationArgs {
                    page: 1,
                    page_size: 10,
                },
            )
            .await
            .unwrap();
        assert!(trades.is_empty());
    }

    #[tokio::test]
    async fn test_trades_list_multiple_subgraphs_merge() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        let trade_s1 = sample_sg_trade("0xtrade_old", "100");
        let trade_s2 = sample_sg_trade("0xtrade_new", "200");
        server1.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains("\"first\":10")
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s1]}}));
        });
        server2.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains("\"first\":10")
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s2]}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two".to_string(),
            },
        ]);

        let trades = client
            .trades_list(
                default_trade_filters(),
                SgPaginationArgs {
                    page: 1,
                    page_size: 10,
                },
            )
            .await
            .unwrap();
        assert_eq!(trades.len(), 2);
        let names: std::collections::HashSet<_> = trades
            .iter()
            .map(|trade| trade.subgraph_name.as_str())
            .collect();
        assert!(names.contains("sg_one"));
        assert!(names.contains("sg_two"));
        assert_eq!(trades[0].trade.id.0, "0xtrade_new");
        assert_eq!(trades[1].trade.id.0, "0xtrade_old");
    }

    #[tokio::test]
    async fn test_trades_list_one_subgraph_errors_others_succeed() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        let trade_s1 = default_sg_trade();
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s1]}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two_err".to_string(),
            },
        ]);

        let trades = client
            .trades_list(
                default_trade_filters(),
                SgPaginationArgs {
                    page: 1,
                    page_size: 10,
                },
            )
            .await
            .unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].subgraph_name, "sg_one");
    }

    #[tokio::test]
    async fn test_trades_list_every_subgraph_errors() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one_err".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two_err".to_string(),
            },
        ]);

        let result = client
            .trades_list(
                default_trade_filters(),
                SgPaginationArgs {
                    page: 1,
                    page_size: 10,
                },
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_trades_list_all_multiple_subgraphs_merge() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        let trade_s1 = sample_sg_trade("0xtrade_old", "100");
        let trade_s2 = sample_sg_trade("0xtrade_new", "200");
        server1.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s1]}}));
        });
        server2.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s2]}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two".to_string(),
            },
        ]);

        let trades = client
            .trades_list_all(default_trade_filters())
            .await
            .unwrap();
        assert_eq!(trades.len(), 2);
        let names: std::collections::HashSet<_> = trades
            .iter()
            .map(|trade| trade.subgraph_name.as_str())
            .collect();
        assert!(names.contains("sg_one"));
        assert!(names.contains("sg_two"));
        assert_eq!(trades[0].trade.id.0, "0xtrade_new");
        assert_eq!(trades[1].trade.id.0, "0xtrade_old");
    }

    #[tokio::test]
    async fn test_trades_list_all_one_subgraph_errors_others_succeed() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        let trade_s1 = default_sg_trade();
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s1]}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two_err".to_string(),
            },
        ]);

        let trades = client
            .trades_list_all(default_trade_filters())
            .await
            .unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].subgraph_name, "sg_one");
    }

    #[tokio::test]
    async fn test_trades_list_all_every_subgraph_errors() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one_err".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two_err".to_string(),
            },
        ]);

        let result = client.trades_list_all(default_trade_filters()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_trades_count_no_subgraphs() {
        let client = MultiRaindexSubgraphClient::new(vec![]);
        let count = client.trades_count(default_trade_filters()).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_trades_count_multiple_subgraphs_sum() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        let trades_s1 = vec![default_sg_trade(), default_sg_trade()];
        let trades_s2 = vec![default_sg_trade(), default_sg_trade(), default_sg_trade()];
        server1.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": trades_s1}}));
        });
        server2.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"first\":{}", ALL_PAGES_QUERY_PAGE_SIZE))
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": trades_s2}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two".to_string(),
            },
        ]);

        let count = client.trades_count(default_trade_filters()).await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_trades_count_one_subgraph_errors_others_succeed() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        let trades_s1 = vec![default_sg_trade(), default_sg_trade()];
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"trades": trades_s1}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two_err".to_string(),
            },
        ]);

        let count = client.trades_count(default_trade_filters()).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_trades_count_all_subgraphs_error() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one_err".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two_err".to_string(),
            },
        ]);

        let result = client.trades_count(default_trade_filters()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_trades_by_transaction_no_subgraphs() {
        let client = MultiRaindexSubgraphClient::new(vec![]);
        let result = client
            .trades_by_transaction("0xtx123".to_string(), None)
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_trades_by_transaction_one_subgraph_returns_trades() {
        use crate::raindex_client::ALL_PAGES_QUERY_PAGE_SIZE;

        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "subgraph_alpha";
        let tx_id = "0xtx_abc";

        let trade1 = default_sg_trade();
        server1.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(tx_id)
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade1]}}));
        });
        server1.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(tx_id)
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg1_url,
            name: sg1_name.to_string(),
        }]);

        let trades = client
            .trades_by_transaction(tx_id.to_string(), None)
            .await
            .unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].trade.id, trade1.id);
        assert_eq!(trades[0].subgraph_name, sg1_name);
    }

    #[tokio::test]
    async fn test_trades_by_transaction_with_raindex_filter() {
        use crate::raindex_client::ALL_PAGES_QUERY_PAGE_SIZE;

        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "subgraph_ob_filter";
        let tx_id = "0xtx_ob_filter";
        let raindex_addr = "0x1234567890abcdef1234567890abcdef12345678";

        let trade1 = default_sg_trade();
        server1.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(tx_id)
                .body_contains(raindex_addr)
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade1]}}));
        });
        server1.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(tx_id)
                .body_contains(raindex_addr)
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg1_url,
            name: sg1_name.to_string(),
        }]);

        let trades = client
            .trades_by_transaction(tx_id.to_string(), Some(vec![raindex_addr.to_string()]))
            .await
            .unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].trade.id, trade1.id);
        assert_eq!(trades[0].subgraph_name, sg1_name);
    }

    #[tokio::test]
    async fn test_trades_by_transaction_multiple_subgraphs_merge() {
        use crate::raindex_client::ALL_PAGES_QUERY_PAGE_SIZE;

        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_one";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_two";
        let tx_id = "0xtx_multi";

        let trade_s1 = default_sg_trade();
        let trade_s2 = default_sg_trade();

        server1.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(tx_id)
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s1]}}));
        });
        server1.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(tx_id)
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });
        server2.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(tx_id)
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s2]}}));
        });
        server2.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(tx_id)
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);

        let trades = client
            .trades_by_transaction(tx_id.to_string(), None)
            .await
            .unwrap();
        assert_eq!(trades.len(), 2);

        let names: std::collections::HashSet<_> =
            trades.iter().map(|t| t.subgraph_name.clone()).collect();
        assert!(names.contains(sg1_name));
        assert!(names.contains(sg2_name));
    }

    #[tokio::test]
    async fn test_trades_by_transaction_one_subgraph_errors_others_succeed() {
        use crate::raindex_client::ALL_PAGES_QUERY_PAGE_SIZE;

        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_one_ok";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_two_error";
        let tx_id = "0xtx_partial";

        let trade_s1 = default_sg_trade();
        server1.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(tx_id)
                .body_contains("\"skip\":0");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s1]}}));
        });
        server1.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(tx_id)
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/").body_contains(tx_id);
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);
        let trades = client
            .trades_by_transaction(tx_id.to_string(), None)
            .await
            .unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].trade.id, trade_s1.id);
        assert_eq!(trades[0].subgraph_name, sg1_name);
    }

    #[tokio::test]
    async fn test_trades_by_transaction_all_subgraphs_error() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_one_err";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_two_err";
        let tx_id = "0xtx_all_err";

        server1.mock(|when, then| {
            when.method(POST).path("/").body_contains(tx_id);
            then.status(500);
        });
        server2.mock(|when, then| {
            when.method(POST).path("/").body_contains(tx_id);
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);
        let result = client.trades_by_transaction(tx_id.to_string(), None).await;
        assert!(result.is_err());
    }

    fn sample_sg_erc20(id_suffix: &str) -> SgErc20 {
        SgErc20 {
            id: SgBytes(format!("0xtoken_id_{}", id_suffix)),
            address: SgBytes(format!("0xtoken_address_{}", id_suffix)),
            name: Some(format!("Token {}", id_suffix)),
            symbol: Some(format!("TKN{}", id_suffix)),
            decimals: Some(SgBigInt("18".to_string())),
        }
    }

    fn sample_sg_raindex(id_suffix: &str) -> SgRaindex {
        SgRaindex {
            id: SgBytes(format!("0xraindex_id_{}", id_suffix)),
        }
    }

    fn sample_sg_vault(id_suffix: &str) -> SgVault {
        SgVault {
            id: SgBytes(format!("0xvault_id_{}", id_suffix)),
            owner: SgBytes(format!("0xowner_vault_{}", id_suffix)),
            vault_id: SgBytes(format!(
                "{}",
                id_suffix
                    .chars()
                    .filter_map(|c| c.to_digit(10))
                    .fold(0, |acc, digit| acc * 10 + digit)
                    + 1000
            )),
            balance: SgBytes(F1.as_hex()),
            token: sample_sg_erc20(id_suffix),
            raindex: sample_sg_raindex(id_suffix),
            orders_as_output: vec![],
            orders_as_input: vec![],
            balance_changes: vec![],
        }
    }

    fn default_vault_filter_args() -> SgVaultsListFilterArgs {
        SgVaultsListFilterArgs {
            owners: vec![],
            hide_zero_balance: false,
            tokens: vec![],
            raindexes: vec![],
            only_active_orders: false,
        }
    }

    #[tokio::test]
    async fn test_vaults_list_no_subgraphs() {
        let client = MultiRaindexSubgraphClient::new(vec![]);
        let result = client
            .vaults_list(default_vault_filter_args(), default_pagination_args())
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_vaults_list_one_subgraph_returns_vaults() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "subgraph_gamma";

        let vault1_s1 = sample_sg_vault("s1_v1");
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"vaults": [vault1_s1]}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg1_url,
            name: sg1_name.to_string(),
        }]);

        let vaults = client
            .vaults_list(default_vault_filter_args(), default_pagination_args())
            .await
            .unwrap();
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].vault.id, vault1_s1.id);
        assert_eq!(vaults[0].subgraph_name, sg1_name);
    }

    #[tokio::test]
    async fn test_vaults_list_multiple_subgraphs_merge() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_v_one";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_v_two";

        let vault_a_s1 = sample_sg_vault("s1_VA");
        let vault_b_s2 = sample_sg_vault("s2_VB");
        let vault_c_s2 = sample_sg_vault("s2_VC");

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"vaults": [vault_a_s1]}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"vaults": [vault_b_s2, vault_c_s2]}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);

        let vaults_with_names = client
            .vaults_list(default_vault_filter_args(), default_pagination_args())
            .await
            .unwrap();

        assert_eq!(vaults_with_names.len(), 3);

        let mut expected_vault_ids_with_names = std::collections::HashSet::new();
        expected_vault_ids_with_names.insert((vault_a_s1.id.clone(), sg1_name.to_string()));
        expected_vault_ids_with_names.insert((vault_b_s2.id.clone(), sg2_name.to_string()));
        expected_vault_ids_with_names.insert((vault_c_s2.id.clone(), sg2_name.to_string()));

        let actual_vault_ids_with_names: std::collections::HashSet<_> = vaults_with_names
            .into_iter()
            .map(|v| (v.vault.id, v.subgraph_name))
            .collect();

        assert_eq!(actual_vault_ids_with_names, expected_vault_ids_with_names);
    }

    #[tokio::test]
    async fn test_vaults_list_multiple_subgraphs_some_empty() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_v_one";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_v_two_empty";

        let vault_a_s1 = sample_sg_vault("s1_VA");
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"vaults": [vault_a_s1]}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(json!({"data": {"vaults": []}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);
        let vaults = client
            .vaults_list(default_vault_filter_args(), default_pagination_args())
            .await
            .unwrap();
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].vault.id, vault_a_s1.id);
        assert_eq!(vaults[0].subgraph_name, sg1_name);
    }

    #[tokio::test]
    async fn test_vaults_list_one_subgraph_errors_others_succeed() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_v_one_ok";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_v_two_error";

        let vault_a_s1 = sample_sg_vault("s1_VA");
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"vaults": [vault_a_s1]}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);
        let vaults = client
            .vaults_list(default_vault_filter_args(), default_pagination_args())
            .await
            .unwrap();
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].vault.id, vault_a_s1.id);
        assert_eq!(vaults[0].subgraph_name, sg1_name);
    }

    #[tokio::test]
    async fn test_vaults_list_strict_errors_when_one_subgraph_errors() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        let vault_a_s1 = sample_sg_vault("s1_VA");
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"vaults": [vault_a_s1]}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_v_one_ok".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_v_two_error".to_string(),
            },
        ]);
        let result = client
            .vaults_list_strict(default_vault_filter_args(), default_pagination_args())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_vaults_list_all_subgraphs_error() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_v_one_err";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_v_two_err";

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);
        let result = client
            .vaults_list(default_vault_filter_args(), default_pagination_args())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_trades_by_owner_no_subgraphs() {
        let client = MultiRaindexSubgraphClient::new(vec![]);
        let result = client
            .trades_by_owner("0xowner".to_string(), None, None, None)
            .await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_trades_by_owner_one_subgraph_returns_trades() {
        let server = MockServer::start_async().await;
        let sg_url = Url::parse(&server.url("")).unwrap();
        let sg_name = "sg_owner";
        let owner = "0xowner_abc";

        let trade1 = default_sg_trade();
        server.mock(|when, then| {
            when.method(POST).path("/").body_contains(owner);
            then.status(200)
                .json_body(json!({"data": {"trades": [trade1]}}));
        });
        server.mock(|when, then| {
            when.method(POST).path("/").body_contains(owner);
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg_url,
            name: sg_name.to_string(),
        }]);

        let trades = client
            .trades_by_owner(owner.to_string(), None, None, None)
            .await;
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].trade.id, trade1.id);
        assert_eq!(trades[0].subgraph_name, sg_name);
    }

    #[tokio::test]
    async fn test_trades_by_owner_multiple_subgraphs_merge() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_one";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_two";

        let owner = "0xowner_multi";
        let trade_s1 = default_sg_trade();
        let trade_s2 = default_sg_trade();

        server1.mock(|when, then| {
            when.method(POST).path("/").body_contains(owner);
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s1]}}));
        });
        server1.mock(|when, then| {
            when.method(POST).path("/").body_contains(owner);
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/").body_contains(owner);
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s2]}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/").body_contains(owner);
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);

        let trades = client
            .trades_by_owner(owner.to_string(), None, None, None)
            .await;
        assert_eq!(trades.len(), 2);

        let names: std::collections::HashSet<_> =
            trades.iter().map(|t| t.subgraph_name.clone()).collect();
        assert!(names.contains(sg1_name));
        assert!(names.contains(sg2_name));
    }

    #[tokio::test]
    async fn test_trades_by_owner_with_time_filters() {
        let server = MockServer::start_async().await;
        let sg_url = Url::parse(&server.url("")).unwrap();
        let sg_name = "sg_time";
        let owner = "0xowner_time";

        let trade1 = default_sg_trade();
        server.mock(|when, then| {
            when.method(POST).path("/").body_contains(owner);
            then.status(200)
                .json_body(json!({"data": {"trades": [trade1]}}));
        });
        server.mock(|when, then| {
            when.method(POST).path("/").body_contains(owner);
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg_url,
            name: sg_name.to_string(),
        }]);

        let trades = client
            .trades_by_owner(owner.to_string(), Some(1000), Some(2000), None)
            .await;
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].trade.id, trade1.id);
    }

    #[tokio::test]
    async fn test_trades_by_owner_with_raindex_filter() {
        let server = MockServer::start_async().await;
        let sg_url = Url::parse(&server.url("")).unwrap();
        let sg_name = "sg_ob_filter";
        let owner = "0xowner_ob";
        let raindex_addr = "0x1234567890abcdef1234567890abcdef12345678";

        let trade1 = default_sg_trade();
        server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(owner)
                .body_contains(raindex_addr);
            then.status(200)
                .json_body(json!({"data": {"trades": [trade1]}}));
        });
        server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(owner)
                .body_contains(raindex_addr);
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let client = MultiRaindexSubgraphClient::new(vec![MultiSubgraphArgs {
            url: sg_url,
            name: sg_name.to_string(),
        }]);

        let trades = client
            .trades_by_owner(
                owner.to_string(),
                None,
                None,
                Some(vec![raindex_addr.to_string()]),
            )
            .await;
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].trade.id, trade1.id);
    }

    #[tokio::test]
    async fn test_trades_by_owner_one_subgraph_errors_others_succeed() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();
        let sg1_name = "sg_one_ok";

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();
        let sg2_name = "sg_two_error";

        let owner = "0xowner_partial";
        let trade_s1 = default_sg_trade();
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200)
                .json_body(json!({"data": {"trades": [trade_s1]}}));
        });
        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: sg1_name.to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: sg2_name.to_string(),
            },
        ]);
        let trades = client
            .trades_by_owner(owner.to_string(), None, None, None)
            .await;
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].trade.id, trade_s1.id);
        assert_eq!(trades[0].subgraph_name, sg1_name);
    }

    #[tokio::test]
    async fn test_trades_by_owner_all_subgraphs_error() {
        let server1 = MockServer::start_async().await;
        let sg1_url = Url::parse(&server1.url("")).unwrap();

        let server2 = MockServer::start_async().await;
        let sg2_url = Url::parse(&server2.url("")).unwrap();

        server1.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });
        server2.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let client = MultiRaindexSubgraphClient::new(vec![
            MultiSubgraphArgs {
                url: sg1_url,
                name: "sg_one_err".to_string(),
            },
            MultiSubgraphArgs {
                url: sg2_url,
                name: "sg_two_err".to_string(),
            },
        ]);
        let trades = client
            .trades_by_owner("0xowner_all_err".to_string(), None, None, None)
            .await;
        assert!(trades.is_empty());
    }
}
