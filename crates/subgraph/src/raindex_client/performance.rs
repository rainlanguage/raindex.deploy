use super::*;
use crate::performance::vol::{get_vaults_vol, VaultVolume};

impl RaindexSubgraphClient {
    pub async fn order_vaults_volume(
        &self,
        order_id: cynic::Id,
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
    ) -> Result<Vec<VaultVolume>, RaindexSubgraphClientError> {
        let trades = self
            .order_trades_list_all(order_id, start_timestamp, end_timestamp)
            .await?;
        Ok(get_vaults_vol(&trades)?)
    }

    // TODO: APY related logic
    // /// Fetches order data and measures an order's detailed performance (apy and vol)
    // pub async fn order_performance(
    //     &self,
    //     order_id: cynic::Id,
    //     start_timestamp: Option<u64>,
    //     end_timestamp: Option<u64>,
    // ) -> Result<OrderPerformance, RaindexSubgraphClientError> {
    //     let order = self.order_detail(&order_id).await?;
    //     let trades = self
    //         .order_trades_list_all(order_id, start_timestamp, end_timestamp)
    //         .await?;
    //     Ok(OrderPerformance::measure(
    //         &order,
    //         &trades,
    //         start_timestamp,
    //         end_timestamp,
    //     )?)
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raindex_client::RaindexSubgraphClientError;
    use crate::types::common::{
        SgBigInt, SgBytes, SgErc20, SgRaindex, SgTrade, SgTradeEvent, SgTradeEventTypename,
        SgTradeRef, SgTradeStructPartialOrder, SgTradeVaultBalanceChange, SgTransaction,
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

    fn default_sg_erc20(id_suffix: &str) -> SgErc20 {
        SgErc20 {
            id: SgBytes(format!("0xtoken_id_{}", id_suffix)),
            address: SgBytes(format!("0xtoken_address_{}", id_suffix)),
            name: Some(format!("Token {}", id_suffix.to_uppercase())),
            symbol: Some(format!("TKN{}", id_suffix.to_uppercase())),
            decimals: Some(SgBigInt("18".to_string())),
        }
    }

    fn default_sg_trade(trade_id_str: &str, order_id_str: &str, timestamp: u64) -> SgTrade {
        let input_token = default_sg_erc20("input");
        let output_token = default_sg_erc20("output");
        SgTrade {
            id: SgBytes(trade_id_str.to_string()),
            timestamp: SgBigInt(timestamp.to_string()),
            order: SgTradeStructPartialOrder {
                id: SgBytes(order_id_str.to_string()),
                order_hash: SgBytes(format!("0xhash_{}", order_id_str)),
                owner: SgBytes("0xowner_default".to_string()),
            },
            raindex: SgRaindex {
                id: SgBytes("0xraindex_default".to_string()),
            },
            trade_event: SgTradeEvent {
                id: SgBytes("0xevent_default".to_string()),
                __typename: "TakeOrder".to_string(),
                transaction: SgTransaction {
                    id: SgBytes("0xtx_default".to_string()),
                    from: SgBytes("0xfrom_default".to_string()),
                    block_number: SgBigInt("1".to_string()),
                    timestamp: SgBigInt(timestamp.to_string()),
                },
                sender: SgBytes("0xsender_default".to_string()),
            },
            input_vault_balance_change: SgTradeVaultBalanceChange {
                id: SgBytes("ivbc_default".to_string()),
                __typename: "TradeVaultBalanceChange".to_string(),
                amount: SgBytes(F100.as_hex()),
                new_vault_balance: SgBytes("".to_string()),
                old_vault_balance: SgBytes("".to_string()),
                vault: SgVaultBalanceChangeVault {
                    id: SgBytes("input_vault_id".to_string()),
                    vault_id: SgBytes("input_vault_sg_id".to_string()),
                    token: input_token.clone(),
                },
                timestamp: SgBigInt(timestamp.to_string()),
                transaction: SgTransaction {
                    id: SgBytes("0xtx_default".to_string()),
                    from: SgBytes("0xfrom_default".to_string()),
                    block_number: SgBigInt("1".to_string()),
                    timestamp: SgBigInt(timestamp.to_string()),
                },
                raindex: SgRaindex {
                    id: SgBytes("0xraindex_default".to_string()),
                },
                trade: SgTradeRef {
                    trade_event: SgTradeEventTypename {
                        __typename: "TradeEvent".to_string(),
                    },
                },
            },
            output_vault_balance_change: SgTradeVaultBalanceChange {
                id: SgBytes("ovbc_default".to_string()),
                __typename: "TradeVaultBalanceChange".to_string(),
                amount: SgBytes(NEG5.as_hex()),
                new_vault_balance: SgBytes("".to_string()),
                old_vault_balance: SgBytes("".to_string()),
                vault: SgVaultBalanceChangeVault {
                    id: SgBytes("output_vault_id".to_string()),
                    vault_id: SgBytes("output_vault_sg_id".to_string()),
                    token: output_token.clone(),
                },
                timestamp: SgBigInt(timestamp.to_string()),
                transaction: SgTransaction {
                    id: SgBytes("0xtx_default".to_string()),
                    from: SgBytes("0xfrom_default".to_string()),
                    block_number: SgBigInt("1".to_string()),
                    timestamp: SgBigInt(timestamp.to_string()),
                },
                raindex: SgRaindex {
                    id: SgBytes("0xraindex_default".to_string()),
                },
                trade: SgTradeRef {
                    trade_event: SgTradeEventTypename {
                        __typename: "TradeEvent".to_string(),
                    },
                },
            },
        }
    }

    #[tokio::test]
    async fn test_order_vaults_volume_success() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let order_id_str = "0xvol_order_1";
        let order_id = Id::new(order_id_str);
        let trades = vec![default_sg_trade("trade1", order_id_str, 1600000100)];

        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"skip\":{}", 0));
            then.status(200)
                .json_body(json!({"data": {"trades": trades}}));
        });
        sg_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client.order_vaults_volume(order_id, None, None).await;
        assert!(result.is_ok(), "Result was: {:?}", result);
        let vault_volumes = result.unwrap();
        assert_eq!(vault_volumes[0].id, "input_vault_sg_id");
        assert_eq!(vault_volumes[0].token, default_sg_erc20("input"));
        assert!(vault_volumes[0].vol_details.total_in.eq(F100).unwrap());
        assert!(vault_volumes[0].vol_details.total_out.eq(F0).unwrap());
        assert!(vault_volumes[0].vol_details.total_vol.eq(F100).unwrap());
        assert!(vault_volumes[0].vol_details.net_vol.eq(F100).unwrap());
        assert_eq!(vault_volumes[1].id, "output_vault_sg_id");
        assert_eq!(vault_volumes[1].token, default_sg_erc20("output"));
        assert!(vault_volumes[1].vol_details.total_in.eq(F0).unwrap());
        assert!(vault_volumes[1].vol_details.total_out.eq(F5).unwrap());
        assert!(vault_volumes[1].vol_details.total_vol.eq(F5).unwrap());
        assert!(vault_volumes[1].vol_details.net_vol.eq(NEG5).unwrap());
    }

    #[tokio::test]
    async fn test_order_vaults_volume_success_no_trades() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let order_id_str = "0xvol_order_notrades";
        let order_id = Id::new(order_id_str);

        sg_server.mock(|when, then| {
            when.method(POST).path("/").body_contains(order_id_str);
            then.status(200).json_body(json!({"data": {"trades": []}}));
        });

        let result = client.order_vaults_volume(order_id, None, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_order_vaults_volume_error_from_trades_list_all() {
        let sg_server = MockServer::start_async().await;
        let client = setup_client(&sg_server);
        let order_id = Id::new("0xvol_order_err_trades");

        sg_server.mock(|when, then| {
            when.method(POST).path("/");
            then.status(500);
        });

        let result = client.order_vaults_volume(order_id, None, None).await;
        assert!(matches!(
            result,
            Err(RaindexSubgraphClientError::CynicClientError(_))
        ));
    }

    // TODO: APY related logic
    // use crate::performance::PerformanceError;
    // use crate::types::common::{SgOrder, SgVault};
    // use alloy::primitives::U256;
    //
    // fn default_sg_order(order_id_str: &str) -> SgOrder {
    //     SgOrder {
    //         id: SgBytes(order_id_str.to_string()),
    //         order_hash: SgBytes(format!("0xhash_{}", order_id_str)),
    //         owner: SgBytes("0xowner_default".to_string()),
    //         order_bytes: SgBytes("0xorderbytes_default".to_string()),
    //         timestamp_added: SgBigInt("1600000000".to_string()),
    //         active: true,
    //         raindex: SgRaindex {
    //             id: SgBytes("0xraindex_default".to_string()),
    //         },
    //         inputs: vec![SgVault {
    //             id: SgBytes("input_vault_id".to_string()),
    //             owner: SgBytes("0xowner_default".to_string()),
    //             vault_id: SgBigInt("input_vault_sg_id".to_string()),
    //             balance: SgBigInt("1000".to_string()),
    //             token: default_sg_erc20("input"),
    //             raindex: SgRaindex {
    //                 id: SgBytes("0xraindex_default".to_string()),
    //             },
    //             orders_as_output: vec![],
    //             orders_as_input: vec![],
    //             balance_changes: vec![],
    //         }],
    //         outputs: vec![SgVault {
    //             id: SgBytes("output_vault_id".to_string()),
    //             owner: SgBytes("0xowner_default".to_string()),
    //             vault_id: SgBigInt("output_vault_sg_id".to_string()),
    //             balance: SgBigInt("0".to_string()),
    //             token: default_sg_erc20("output"),
    //             raindex: SgRaindex {
    //                 id: SgBytes("0xraindex_default".to_string()),
    //             },
    //             orders_as_output: vec![],
    //             orders_as_input: vec![],
    //             balance_changes: vec![],
    //         }],
    //         meta: None,
    //         add_events: vec![],
    //         trades: vec![],
    //         remove_events: vec![],
    //     }
    // }
    //
    // #[tokio::test]
    // async fn test_order_performance_success() {
    //     let sg_server = MockServer::start_async().await;
    //     let client = setup_client(&sg_server);
    //     let order_id_str = "0xperf_order_1";
    //     let order_id = Id::new(order_id_str);
    //     let order_data = default_sg_order(order_id_str);
    //     let trades_data = vec![
    //         default_sg_trade("trade_perf_1", order_id_str, 1600000100),
    //         default_sg_trade("trade_perf_2", order_id_str, 1600000200),
    //     ];
    //
    //     sg_server.mock(|when, then| {
    //         when.method(POST)
    //             .path("/")
    //             .body_contains(format!("\"id\":\"{}\"", order_id_str))
    //             .body_contains("SgOrderDetailByIdQuery");
    //         then.status(200)
    //             .json_body(json!({"data": {"order": order_data}}));
    //     });
    //
    //     sg_server.mock(|when, then| {
    //         when.method(POST)
    //             .path("/")
    //             .body_contains(order_id_str)
    //             .body_contains("SgOrderTradesListQuery")
    //             .body_contains("\"skip\":0");
    //         then.status(200)
    //             .json_body(json!({"data": {"trades": trades_data}}));
    //     });
    //
    //     sg_server.mock(|when, then| {
    //         when.method(POST)
    //             .path("/")
    //             .body_contains(order_id_str)
    //             .body_contains("SgOrderTradesListQuery")
    //             .body_contains(format!("\"skip\":{}", ALL_PAGES_QUERY_PAGE_SIZE));
    //         then.status(200).json_body(json!({"data": {"trades": []}}));
    //     });
    //
    //     let result = client.order_performance(order_id, None, None).await;
    //     assert!(result.is_ok(), "Result was: {:?}", result);
    //     let performance_report = result.unwrap();
    //     assert_eq!(performance_report.order_id, order_id_str);
    //     assert_eq!(performance_report.inputs_vaults.len(), 1);
    //     assert_eq!(performance_report.outputs_vaults.len(), 1);
    //     assert_eq!(performance_report.inputs_vaults[0].id, "input_vault_sg_id");
    //     assert_eq!(
    //         performance_report.inputs_vaults[0].token,
    //         default_sg_erc20("input")
    //     );
    //     assert_eq!(
    //         performance_report.outputs_vaults[0].id,
    //         "output_vault_sg_id"
    //     );
    //     assert_eq!(
    //         performance_report.outputs_vaults[0].token,
    //         default_sg_erc20("output")
    //     );
    //     assert_eq!(
    //         performance_report.inputs_vaults[0].vol_details.net_vol,
    //         U256::from(200)
    //     );
    //     assert_eq!(
    //         performance_report.inputs_vaults[0].vol_details.total_in,
    //         U256::from(200)
    //     );
    //     assert_eq!(
    //         performance_report.inputs_vaults[0].vol_details.total_out,
    //         U256::from(0)
    //     );
    //     assert_eq!(
    //         performance_report.inputs_vaults[0].vol_details.total_vol,
    //         U256::from(200)
    //     );
    //     assert_eq!(
    //         performance_report.outputs_vaults[0].vol_details.net_vol,
    //         U256::from(100)
    //     );
    //     assert_eq!(
    //         performance_report.outputs_vaults[0].vol_details.total_in,
    //         U256::from(0)
    //     );
    //     assert_eq!(
    //         performance_report.outputs_vaults[0].vol_details.total_out,
    //         U256::from(100)
    //     );
    //     assert_eq!(
    //         performance_report.outputs_vaults[0].vol_details.total_vol,
    //         U256::from(100)
    //     );
    // }
    //
    // #[tokio::test]
    // async fn test_order_performance_error_no_trades() {
    //     let sg_server = MockServer::start_async().await;
    //     let client = setup_client(&sg_server);
    //     let order_id_str = "0xperf_order_notrades";
    //     let order_id = Id::new(order_id_str);
    //     let order_data = default_sg_order(order_id_str);
    //
    //     sg_server.mock(|when, then| {
    //         when.method(POST)
    //             .path("/")
    //             .body_contains(format!("\"id\":\"{}\"", order_id_str))
    //             .body_contains("SgOrderDetailByIdQuery");
    //         then.status(200)
    //             .json_body(json!({"data": {"order": order_data}}));
    //     });
    //
    //     sg_server.mock(|when, then| {
    //         when.method(POST)
    //             .path("/")
    //             .body_contains(order_id_str)
    //             .body_contains("SgOrderTradesListQuery");
    //         then.status(200).json_body(json!({"data": {"trades": []}}));
    //     });
    //
    //     let result = client.order_performance(order_id, None, None).await;
    //     assert!(result.is_err());
    //     match result {
    //         Err(RaindexSubgraphClientError::PerformanceError(PerformanceError::NoTrades)) => (),
    //         _ => panic!("Expected PerformanceError::NoTrades, got {:?}", result),
    //     }
    // }
    //
    // #[tokio::test]
    // async fn test_order_performance_error_from_order_detail() {
    //     let sg_server = MockServer::start_async().await;
    //     let client = setup_client(&sg_server);
    //     let order_id = Id::new("0xperf_order_err_detail");
    //
    //     sg_server.mock(|when, then| {
    //         when.method(POST)
    //             .path("/")
    //             .body_contains("SgOrderDetailByIdQuery");
    //         then.status(200).json_body(json!({"data": {"order": null}}));
    //     });
    //
    //     let result = client.order_performance(order_id, None, None).await;
    //     assert!(matches!(result, Err(RaindexSubgraphClientError::Empty)));
    // }
    //
    // #[tokio::test]
    // async fn test_order_performance_error_from_trades_list_all() {
    //     let sg_server = MockServer::start_async().await;
    //     let client = setup_client(&sg_server);
    //     let order_id_str = "0xperf_order_err_trades";
    //     let order_id = Id::new(order_id_str);
    //     let order_data = default_sg_order(order_id_str);
    //
    //     sg_server.mock(|when, then| {
    //         when.method(POST)
    //             .path("/")
    //             .body_contains(format!("\"id\":\"{}\"", order_id_str))
    //             .body_contains("SgOrderDetailByIdQuery");
    //         then.status(200)
    //             .json_body(json!({"data": {"order": order_data}}));
    //     });
    //
    //     sg_server.mock(|when, then| {
    //         when.method(POST)
    //             .path("/")
    //             .body_contains(order_id_str)
    //             .body_contains("SgOrderTradesListQuery");
    //         then.status(500);
    //     });
    //
    //     let result = client.order_performance(order_id, None, None).await;
    //     assert!(matches!(
    //         result,
    //         Err(RaindexSubgraphClientError::CynicClientError(_))
    //     ));
    // }
}
