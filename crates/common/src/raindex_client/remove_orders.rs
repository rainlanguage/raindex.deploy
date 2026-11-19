use super::orders::{OrdersDataSource, SubgraphOrders};
use super::ClientRef;
use super::*;
use crate::raindex_client::local_db::orders::LocalDbOrders;
use crate::raindex_client::orders::{fetch_orders_dotrain_sources, RaindexOrder};
use crate::raindex_client::QuerySource;
use crate::retry::{retry_with_constant_interval, RetryError};
use crate::utils::timing::Timing;
use alloy::primitives::{hex::decode, Bytes, B256};
use alloy::sol_types::{SolCall, SolValue};
use raindex_bindings::IRaindexV7::{removeOrder4Call, OrderV4};
use raindex_subgraph_client::types::order_detail_traits::OrderDetailError;
use tracing::{error, info, info_span};

const DEFAULT_REMOVE_ORDER_POLL_ATTEMPTS: usize = 10;
const DEFAULT_REMOVE_ORDER_POLL_INTERVAL_MS: u64 = 1_000;

#[derive(Debug)]
enum PollError {
    Empty,
    Inner(RaindexError),
}

#[wasm_export]
impl RaindexClient {
    /// Fetches orders that were removed in a specific transaction
    ///
    /// Retrieves all orders cancelled or removed within a single blockchain transaction.
    /// Polls either local DB or subgraph based on the query source.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await client.getRemoveOrdersForTransaction(1, "0x1234567890abcdef1234567890abcdef12345678");
    /// if (result.error) {
    ///   console.error("Cannot fetch removed orders:", result.error.readableMsg);
    ///   return;
    /// }
    /// const orders = result.value;
    /// // Do something with orders
    /// ```
    #[wasm_export(
        js_name = "getRemoveOrdersForTransaction",
        return_description = "Array of orders removed in the transaction",
        unchecked_return_type = "RaindexOrder[]",
        preserve_js_class
    )]
    pub async fn get_remove_orders_for_transaction_wasm_binding(
        &self,
        #[wasm_export(js_name = "chainId", param_description = "Chain ID for the network")]
        chain_id: u32,
        #[wasm_export(
            js_name = "raindexAddress",
            param_description = "Raindex contract address",
            unchecked_param_type = "Address"
        )]
        raindex_address: String,
        #[wasm_export(
            js_name = "txHash",
            param_description = "Transaction hash",
            unchecked_param_type = "Hex"
        )]
        tx_hash: String,
        #[wasm_export(
            js_name = "maxAttempts",
            param_description = "Optional maximum polling attempts before timing out"
        )]
        max_attempts: Option<u32>,
        #[wasm_export(
            js_name = "intervalMs",
            param_description = "Optional polling interval in milliseconds"
        )]
        interval_ms: Option<u32>,
    ) -> Result<Vec<RaindexOrder>, RaindexError> {
        let raindex_address = Address::from_str(&raindex_address)?;
        let tx_hash = B256::from_str(&tx_hash)?;
        self.get_remove_orders_for_transaction(
            chain_id,
            raindex_address,
            tx_hash,
            max_attempts.map(|v| v as usize),
            interval_ms.map(|v| v as u64),
        )
        .await
    }
}
impl RaindexClient {
    async fn get_remove_orders_for_transaction(
        &self,
        chain_id: u32,
        raindex_address: Address,
        tx_hash: B256,
        max_attempts: Option<usize>,
        interval_ms: Option<u64>,
    ) -> Result<Vec<RaindexOrder>, RaindexError> {
        let raindex_client = ClientRef::new(self.clone());

        let attempts = max_attempts
            .unwrap_or(DEFAULT_REMOVE_ORDER_POLL_ATTEMPTS)
            .max(1);
        let interval_ms = interval_ms.unwrap_or(DEFAULT_REMOVE_ORDER_POLL_INTERVAL_MS);

        match self.query_source(chain_id) {
            QuerySource::LocalDb(local_db) => {
                let local_source = LocalDbOrders::new(&local_db, raindex_client.clone());
                let local_result = retry_with_constant_interval(
                    || async {
                        let orders = local_source
                            .get_removed_by_tx_hash(chain_id, raindex_address, tx_hash)
                            .await
                            .map_err(PollError::Inner)?;
                        if orders.is_empty() {
                            Err(PollError::Empty)
                        } else {
                            Ok(orders)
                        }
                    },
                    attempts,
                    interval_ms,
                    |e| matches!(e, PollError::Empty),
                )
                .await;

                match local_result {
                    Ok(orders) => {
                        let orders = fetch_orders_dotrain_sources(orders).await?;
                        Ok(orders)
                    }
                    Err(RetryError::Operation(PollError::Inner(e))) => Err(e),
                    Err(RetryError::Operation(PollError::Empty))
                    | Err(RetryError::InvalidMaxAttempts) => {
                        Err(RaindexError::TransactionIndexingTimeout { tx_hash, attempts })
                    }
                }
            }
            QuerySource::Subgraph => {
                let subgraph_source = SubgraphOrders::new(self);
                let subgraph_result = retry_with_constant_interval(
                    || async {
                        let orders = match subgraph_source
                            .get_removed_by_tx_hash(chain_id, raindex_address, tx_hash)
                            .await
                        {
                            Ok(orders) => orders,
                            Err(RaindexError::RaindexSubgraphClientError(
                                raindex_subgraph_client::RaindexSubgraphClientError::Empty,
                            )) => return Err(PollError::Empty),
                            Err(e) => return Err(PollError::Inner(e)),
                        };
                        if orders.is_empty() {
                            Err(PollError::Empty)
                        } else {
                            Ok(orders)
                        }
                    },
                    attempts,
                    interval_ms,
                    |e| matches!(e, PollError::Empty),
                )
                .await;

                match subgraph_result {
                    Ok(orders) => {
                        let orders = fetch_orders_dotrain_sources(orders).await?;
                        Ok(orders)
                    }
                    Err(RetryError::Operation(PollError::Inner(e))) => Err(e),
                    Err(RetryError::Operation(PollError::Empty))
                    | Err(RetryError::InvalidMaxAttempts) => {
                        Err(RaindexError::TransactionIndexingTimeout { tx_hash, attempts })
                    }
                }
            }
        }
    }
}

#[wasm_export]
impl RaindexOrder {
    /// Generates ABI-encoded calldata for the `removeOrder2()` function on the raindex contract
    ///
    /// Takes an existing order from the subgraph and creates the transaction calldata needed
    /// to remove it from the raindex. The order must be active and owned by the caller.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// // Generate calldata for removing an order
    /// const result = await order.getRemoveCalldata();
    /// if (result.error) {
    ///   console.error('Failed:', result.error.readableMsg);
    /// } else {
    ///   const calldata = result.value;
    ///   // Do something with the calldata
    /// }
    /// ```
    #[wasm_export(
        js_name = "getRemoveCalldata",
        return_description = "ABI-encoded calldata ready for blockchain submission",
        unchecked_return_type = "Hex"
    )]
    pub fn get_remove_calldata(&self) -> Result<Bytes, RaindexError> {
        let started_at = Timing::now();
        let span = info_span!(
            "get_remove_calldata",
            chain_id = self.chain_id(),
            raindex = %self.raindex(),
            order_hash = %self.order_hash(),
        );
        let _entered = span.enter();

        info!("building remove order calldata");
        let order = match self.try_into() {
            Ok(order) => order,
            Err(err) => {
                error!(
                    duration_ms = started_at.elapsed_ms(),
                    error = %err,
                    "failed decoding order for remove calldata"
                );
                return Err(RaindexError::from(err));
            }
        };
        let remove_order_call = removeOrder4Call {
            order,
            tasks: vec![],
        };
        let calldata = Bytes::copy_from_slice(&remove_order_call.abi_encode());
        info!(
            calldata_len = calldata.len(),
            duration_ms = started_at.elapsed_ms(),
            "built remove order calldata"
        );
        Ok(calldata)
    }
}

impl TryInto<OrderV4> for &RaindexOrder {
    type Error = OrderDetailError;
    fn try_into(self) -> Result<OrderV4, Self::Error> {
        let order = OrderV4::abi_decode(&decode(self.order_bytes().to_string())?)?;
        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_family = "wasm"))]
    use super::*;

    #[cfg(not(target_family = "wasm"))]
    mod non_wasm {
        use super::*;
        use crate::{
            local_db::query::{
                fetch_orders::LocalDbOrder, FromDbJson, LocalDbQueryError, LocalDbQueryExecutor,
                SqlStatement, SqlStatementBatch,
            },
            local_db::RaindexIdentifier,
            raindex_client::local_db::LocalDb,
            raindex_client::tests::{get_test_yaml, new_with_local_db, CHAIN_ID_1_RAINDEX_ADDRESS},
        };
        use alloy::primitives::{b256, Address, Bytes, U256};
        use async_trait::async_trait;
        use httpmock::MockServer;
        use raindex_subgraph_client::utils::float::*;
        use serde_json::{json, Value};
        use std::{
            str::FromStr,
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc,
            },
        };
        use tracing_test::traced_test;

        #[derive(Clone)]
        struct CountingJsonExec {
            json: String,
            calls: Arc<AtomicUsize>,
        }

        #[cfg_attr(target_family = "wasm", async_trait(?Send))]
        #[cfg_attr(not(target_family = "wasm"), async_trait)]
        impl LocalDbQueryExecutor for CountingJsonExec {
            async fn execute_batch(
                &self,
                _batch: &SqlStatementBatch,
            ) -> Result<(), LocalDbQueryError> {
                Ok(())
            }

            async fn query_json<T>(&self, _stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
            where
                T: FromDbJson,
            {
                self.calls.fetch_add(1, Ordering::SeqCst);
                serde_json::from_str(&self.json)
                    .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))
            }

            async fn query_text(&self, _stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
                Err(LocalDbQueryError::database(
                    "query_text not supported in CountingJsonExec",
                ))
            }

            async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
                Err(LocalDbQueryError::not_implemented("wipe_and_recreate"))
            }
        }

        fn empty_remove_orders_response() -> Value {
            json!({
                "data": {
                    "removeOrders": []
                }
            })
        }

        fn sample_remove_orders_response() -> Value {
            json!({
                "data": {
                    "removeOrders": [
                        {
                          "transaction": {
                            "id": "0xb5d715bc74b7a7f2aac8cca544c1c95e209ed4113b82269ac3285142324bc6af",
                            "from": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                            "blockNumber": "37432554",
                            "timestamp": "1739448802"
                          },
                          "order": {
                            "id": "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1",
                            "orderBytes": "0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000f08bcbce72f62c95dcb7c07dcb5ed26acfcfbc1100000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000005c00000000000000000000000000000000000000000000000000000000000000640392c489ef67afdc348209452c338ea5ba2b6152b936e152f610d05e1a20621a40000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae60000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000049d000000000000000000000000000000000000000000000000000000000000000f0000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000c7d713b49da0000914d696e20747261646520616d6f756e742e00000000000000000000000000008b616d6f756e742d75736564000000000000000000000000000000000000000000000000000000000000000000000000000000000000000340aad21b3b70000000000000000000000000000000000000000000000000006194049f30f7200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b1a2bc2ec500000000000000000000000000000000000000000000000000000e043da6172500008f6c6173742d74726164652d74696d65000000000000000000000000000000008d6c6173742d74726164652d696f0000000000000000000000000000000000008c696e697469616c2d74696d650000000000000000000000000000000000000000000000000000000000000000000000000000000000000006f05b59d3b200000000000000000000000000000000000000000000000000008ac7230489e80000000000000000000000020000915e36ef882941816356bc3718df868054f868ad000000000000000000000000000000000000000000000000000000000000027d0a00000024007400e0015801b401e001f40218025c080500040b20000200100001001000000b120003001000010b110004001000030b0100051305000201100001011000003d120000011000020010000003100404211200001d02000001100003031000010c1200004911000003100404001000012b12000001100003031000010c1200004a0200001a0b00090b1000060b20000700100000001000011b1200001a10000047120000001000001a1000004712000001100000011000002e12000001100005011000042e120000001000053d12000001100004001000042e1200000010000601100005001000032e120000481200011d0b020a0010000001100000011000062713000001100003031000010c12000049110000001000030010000247120000001000010b110008001000050110000700100001201200001f12000001100000011000004712000000100006001000073d120000011000002b12000000100008001000043b120000160901080b1000070b10000901100008001000013d1200001b12000001100006001000013d1200000b100009001000033a120000001000040010000248120001001000000b110008001000053d12000000100006001000042b1200000a0401011a10000001100009031000010c1200004a020000001000000110000a031000010c1200004a020000040200010110000b031000010c120000491100000803000201100009031000010c120000491100000110000a031000010c12000049110000100c01030110000d001000002e1200000110000c3e1200000010000100100001001000010010000100100001001000010010000100100001001000013d1a0000020100010210000e3611000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33",
                            "orderHash": "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
                            "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                            "outputs": [
                              {
                                "id": "0x49f6b665c395c7b975caa2fc167cb5119981bbb86798bcaf3c4570153d09dfcf",
                                "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                                "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                                "balance": F10,
                                "token": {
                                  "id": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                                  "address": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                                  "name": "Staked FLR",
                                  "symbol": "sFLR",
                                  "decimals": "18"
                                },
                                "raindex": {
                                  "id": "0xcee8cd002f151a536394e564b84076c41bbbcd4d"
                                },
                                "ordersAsOutput": [
                                  {
                                    "id": "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1",
                                    "orderHash": "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
                                    "active": true
                                  }
                                ],
                                "ordersAsInput": [],
                                "balanceChanges": []
                              }
                            ],
                            "inputs": [
                              {
                                "id": "0x538830b4f8cc03840cea5af799dc532be4363a3ee8f4c6123dbff7a0acc86dac",
                                "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                                "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                                "balance": F0_5,
                                "token": {
                                  "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                  "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                  "name": "Wrapped Flare",
                                  "symbol": "WFLR",
                                  "decimals": "18"
                                },
                                "raindex": {
                                  "id": "0xcee8cd002f151a536394e564b84076c41bbbcd4d"
                                },
                                "ordersAsOutput": [],
                                "ordersAsInput": [
                                  {
                                    "id": "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1",
                                    "orderHash": "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
                                    "active": true
                                  }
                                ],
                                "balanceChanges": []
                              }
                            ],
                            "raindex": {
                              "id": "0xcee8cd002f151a536394e564b84076c41bbbcd4d"
                            },
                            "active": true,
                            "timestampAdded": "1739448802",
                            "meta": "0xff0a89c674ee7874a3005902252f2a20302e2063616c63756c6174652d696f202a2f200a7573696e672d776f7264732d66726f6d203078466532343131434461313933443945346538334135633233344337466433323031303138383361430a616d743a203130302c0a696f3a2063616c6c3c323e28293b0a0a2f2a20312e2068616e646c652d696f202a2f200a3a63616c6c3c333e28292c0a3a656e7375726528657175616c2d746f286f75747075742d7661756c742d64656372656173652829203130302920226d7573742074616b652066756c6c20616d6f756e7422293b0a0a2f2a20322e206765742d696f2d726174696f2d6e6f77202a2f200a656c61707365643a2063616c6c3c343e28292c0a696f3a2073617475726174696e672d73756228302e3031373733353620646976286d756c28656c61707365642073756228302e3031373733353620302e30313733383434292920363029293b0a0a2f2a20332e206f6e652d73686f74202a2f200a3a656e737572652869732d7a65726f286765742868617368286f726465722d68617368282920226861732d657865637574656422292929202268617320657865637574656422292c0a3a7365742868617368286f726465722d68617368282920226861732d657865637574656422292031293b0a0a2f2a20342e206765742d656c6170736564202a2f200a5f3a20737562286e6f772829206765742868617368286f726465722d68617368282920226465706c6f792d74696d65222929293b011bff13109e41336ff20278186170706c69636174696f6e2f6f637465742d73747265616d",
                            "addEvents": [
                              {
                                "transaction": {
                                  "id": "0xb5d715bc74b7a7f2aac8cca544c1c95e209ed4113b82269ac3285142324bc6af",
                                  "from": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                                  "blockNumber": "37432554",
                                  "timestamp": "1739448802"
                                }
                              }
                            ],
                            "trades": [],
                            "removeEvents": []
                          }
                        }
                      ]
                }
            })
        }

        #[tokio::test]
        async fn test_get_transaction_remove_orders() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg");
                then.status(200)
                    .json_body_obj(&sample_remove_orders_response());
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg").to_string(),
                    &sg_server.url("/sg").to_string(),
                    "http://localhost:3000",
                    "http://localhost:3000",
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let res = raindex_client
                .get_remove_orders_for_transaction(
                    1,
                    Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000123"),
                    None,
                    None,
                )
                .await
                .unwrap();

            assert_eq!(res.len(), 1);
            let order = &res[0];
            let transaction = order.transaction().unwrap();

            assert_eq!(
                transaction.id(),
                b256!("0xb5d715bc74b7a7f2aac8cca544c1c95e209ed4113b82269ac3285142324bc6af")
            );
            assert_eq!(
                transaction.from(),
                Address::from_str("0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11").unwrap()
            );
            assert_eq!(transaction.block_number(), U256::from(37432554));
            assert_eq!(transaction.timestamp(), U256::from(1739448802));

            assert_eq!(
                order.id(),
                b256!("0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1")
            );
            assert_eq!(order
                .order_bytes(), Bytes::from_str("0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000f08bcbce72f62c95dcb7c07dcb5ed26acfcfbc1100000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000005c00000000000000000000000000000000000000000000000000000000000000640392c489ef67afdc348209452c338ea5ba2b6152b936e152f610d05e1a20621a40000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae60000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000049d000000000000000000000000000000000000000000000000000000000000000f0000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000c7d713b49da0000914d696e20747261646520616d6f756e742e00000000000000000000000000008b616d6f756e742d75736564000000000000000000000000000000000000000000000000000000000000000000000000000000000000000340aad21b3b70000000000000000000000000000000000000000000000000006194049f30f7200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b1a2bc2ec500000000000000000000000000000000000000000000000000000e043da6172500008f6c6173742d74726164652d74696d65000000000000000000000000000000008d6c6173742d74726164652d696f0000000000000000000000000000000000008c696e697469616c2d74696d650000000000000000000000000000000000000000000000000000000000000000000000000000000000000006f05b59d3b200000000000000000000000000000000000000000000000000008ac7230489e80000000000000000000000020000915e36ef882941816356bc3718df868054f868ad000000000000000000000000000000000000000000000000000000000000027d0a00000024007400e0015801b401e001f40218025c080500040b20000200100001001000000b120003001000010b110004001000030b0100051305000201100001011000003d120000011000020010000003100404211200001d02000001100003031000010c1200004911000003100404001000012b12000001100003031000010c1200004a0200001a0b00090b1000060b20000700100000001000011b1200001a10000047120000001000001a1000004712000001100000011000002e12000001100005011000042e120000001000053d12000001100004001000042e1200000010000601100005001000032e120000481200011d0b020a0010000001100000011000062713000001100003031000010c12000049110000001000030010000247120000001000010b110008001000050110000700100001201200001f12000001100000011000004712000000100006001000073d120000011000002b12000000100008001000043b120000160901080b1000070b10000901100008001000013d1200001b12000001100006001000013d1200000b100009001000033a120000001000040010000248120001001000000b110008001000053d12000000100006001000042b1200000a0401011a10000001100009031000010c1200004a020000001000000110000a031000010c1200004a020000040200010110000b031000010c120000491100000803000201100009031000010c120000491100000110000a031000010c12000049110000100c01030110000d001000002e1200000110000c3e1200000010000100100001001000010010000100100001001000010010000100100001001000013d1a0000020100010210000e3611000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33").unwrap());
            assert_eq!(
                order.order_hash(),
                b256!("0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4")
            );
            assert_eq!(
                order.owner(),
                Address::from_str("0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11").unwrap()
            );
            assert_eq!(
                order.timestamp_added(),
                U256::from_str("1739448802").unwrap()
            );
            assert_eq!(order.meta(), Some(Bytes::from_str("0xff0a89c674ee7874a3005902252f2a20302e2063616c63756c6174652d696f202a2f200a7573696e672d776f7264732d66726f6d203078466532343131434461313933443945346538334135633233344337466433323031303138383361430a616d743a203130302c0a696f3a2063616c6c3c323e28293b0a0a2f2a20312e2068616e646c652d696f202a2f200a3a63616c6c3c333e28292c0a3a656e7375726528657175616c2d746f286f75747075742d7661756c742d64656372656173652829203130302920226d7573742074616b652066756c6c20616d6f756e7422293b0a0a2f2a20322e206765742d696f2d726174696f2d6e6f77202a2f200a656c61707365643a2063616c6c3c343e28292c0a696f3a2073617475726174696e672d73756228302e3031373733353620646976286d756c28656c61707365642073756228302e3031373733353620302e30313733383434292920363029293b0a0a2f2a20332e206f6e652d73686f74202a2f200a3a656e737572652869732d7a65726f286765742868617368286f726465722d68617368282920226861732d657865637574656422292929202268617320657865637574656422292c0a3a7365742868617368286f726465722d68617368282920226861732d657865637574656422292031293b0a0a2f2a20342e206765742d656c6170736564202a2f200a5f3a20737562286e6f772829206765742868617368286f726465722d68617368282920226465706c6f792d74696d65222929293b011bff13109e41336ff20278186170706c69636174696f6e2f6f637465742d73747265616d").unwrap()));
            assert!(order.active());
            assert_eq!(
                order.raindex(),
                Address::from_str("0xcee8cd002f151a536394e564b84076c41bbbcd4d").unwrap()
            );

            assert_eq!(order.outputs_list().items().len(), 1);
            let output = &order.outputs_list().items()[0];
            assert_eq!(
                output.id(),
                Bytes::from_str(
                    "0x49f6b665c395c7b975caa2fc167cb5119981bbb86798bcaf3c4570153d09dfcf"
                )
                .unwrap()
            );
            assert_eq!(
                output.owner(),
                Address::from_str("0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11").unwrap()
            );
            assert_eq!(
                output.vault_id(),
                U256::from_str(
                    "75486334982066122983501547829219246999490818941767825330875804445439814023987"
                )
                .unwrap()
            );
            assert!(output.balance().eq(F10).unwrap());
            assert_eq!(
                output.token().id(),
                "0x12e605bc104e93b45e1ad99f9e555f659051c2bb".to_string()
            );
            assert_eq!(
                output.token().address(),
                Address::from_str("0x12e605bc104e93b45e1ad99f9e555f659051c2bb").unwrap()
            );
            assert_eq!(output.token().name(), Some("Staked FLR".to_string()));
            assert_eq!(output.token().symbol(), Some("sFLR".to_string()));
            assert_eq!(output.token().decimals(), 18);
            assert_eq!(
                output.raindex(),
                Address::from_str("0xcee8cd002f151a536394e564b84076c41bbbcd4d").unwrap()
            );
            assert_eq!(output.orders_as_outputs().len(), 1);
            assert_eq!(
                output.orders_as_outputs()[0].id,
                b256!("0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1")
            );
            assert_eq!(
                output.orders_as_outputs()[0].order_hash,
                b256!("0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4")
            );
            assert!(output.orders_as_outputs()[0].active);
            assert!(output.orders_as_inputs().is_empty());

            assert_eq!(order.inputs_list().items().len(), 1);
            let input = &order.inputs_list().items()[0];
            assert_eq!(
                input.id(),
                Bytes::from_str(
                    "0x538830b4f8cc03840cea5af799dc532be4363a3ee8f4c6123dbff7a0acc86dac"
                )
                .unwrap()
            );
            assert_eq!(
                input.owner(),
                Address::from_str("0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11").unwrap()
            );
            assert_eq!(
                input.vault_id(),
                U256::from_str(
                    "75486334982066122983501547829219246999490818941767825330875804445439814023987"
                )
                .unwrap()
            );
            assert!(input.balance().eq(F0_5).unwrap());
            assert_eq!(
                input.token().id(),
                "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d".to_string()
            );
            assert_eq!(
                input.token().address(),
                Address::from_str("0x1d80c49bbbcd1c0911346656b529df9e5c2f783d").unwrap()
            );
            assert_eq!(input.token().name(), Some("Wrapped Flare".to_string()));
            assert_eq!(input.token().symbol(), Some("WFLR".to_string()));
            assert_eq!(input.token().decimals(), 18);
            assert_eq!(
                input.raindex(),
                Address::from_str("0xcee8cd002f151a536394e564b84076c41bbbcd4d").unwrap()
            );
            assert!(input.orders_as_outputs().is_empty());
            assert_eq!(input.orders_as_inputs().len(), 1);
            assert_eq!(
                input.orders_as_inputs()[0].id,
                b256!("0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1")
            );
            assert_eq!(
                input.orders_as_inputs()[0].order_hash,
                b256!("0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4")
            );
            assert!(input.orders_as_inputs()[0].active);

            assert!(order.transaction().is_some());
            let transaction = &order.transaction().unwrap();
            assert_eq!(
                transaction.id(),
                b256!("0xb5d715bc74b7a7f2aac8cca544c1c95e209ed4113b82269ac3285142324bc6af")
            );
            assert_eq!(
                transaction.from(),
                Address::from_str("0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11").unwrap()
            );
            assert_eq!(transaction.block_number(), U256::from(37432554));
            assert_eq!(transaction.timestamp(), U256::from(1739448802));

            assert_eq!(order.trades_count(), 0);
        }

        fn get_order1_json() -> Value {
            json!({
              "id": "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1",
              "orderBytes": "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000006171c21b2e553c59a64d1337211b77c367cefe5d00000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001c000000000000000000000000000000000000000000000000000000000000002400000000000000000000000000000000000000000000000000000000000000001000000000000000000000000379b966dc6b117dd47b5fc5308534256a4ab1bcc0000000000000000000000006e4b01603edbda617002a077420e98c86595748e000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000950000000000000000000000000000000000000000000000000000000000000002ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0000000000000000000000000000000000000000000000000b1a2bc2ec5000000000000000000000000000000000000000000000000000000000000000000015020000000c020200020110000001100001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000050c5725949a6f0c72e6c4a641f24049a917db0cb000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda0291300000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000001",
              "orderHash": "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
              "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
              "outputs": [],
              "inputs": [],
              "raindex": {
                "id": CHAIN_ID_1_RAINDEX_ADDRESS
              },
              "active": true,
              "timestampAdded": "1739448802",
              "meta": null,
              "addEvents": [],
              "trades": [],
              "removeEvents": []
            })
        }

        #[traced_test]
        #[tokio::test]
        async fn test_get_remove_calldata() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [get_order1_json()]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg").to_string(),
                    &sg_server.url("/sg").to_string(),
                    "http://localhost:3000",
                    "http://localhost:3000",
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let order = raindex_client
                .get_order_by_hash(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000123"),
                )
                .await
                .unwrap();
            let calldata = order.get_remove_calldata().unwrap();
            assert_eq!(
                calldata,
                Bytes::from_str("0xdf6db3c2000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000002c00000000000000000000000006171c21b2e553c59a64d1337211b77c367cefe5d00000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001c000000000000000000000000000000000000000000000000000000000000002200000000000000000000000000000000000000000000000000000000000000001000000000000000000000000379b966dc6b117dd47b5fc5308534256a4ab1bcc0000000000000000000000006e4b01603edbda617002a077420e98c86595748e000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000950000000000000000000000000000000000000000000000000000000000000002ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0000000000000000000000000000000000000000000000000b1a2bc2ec5000000000000000000000000000000000000000000000000000000000000000000015020000000c020200020110000001100001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000050c5725949a6f0c72e6c4a641f24049a917db0cb00000000000000000000000000000000000000000000000000000000000000120000000000000000000000000000000000000000000000000000000000000001000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda0291300000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000").unwrap()
            );
            assert!(logs_contain("built remove order calldata"));
        }

        #[tokio::test]
        async fn test_get_transaction_remove_orders_with_polling_success() {
            let sg_server = MockServer::start_async().await;
            let _mock = sg_server.mock(|when, then| {
                when.path("/sg");
                then.status(200)
                    .json_body_obj(&sample_remove_orders_response());
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg").to_string(),
                    &sg_server.url("/sg").to_string(),
                    "http://localhost:3000",
                    "http://localhost:3000",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let res = raindex_client
                .get_remove_orders_for_transaction(
                    1,
                    Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000123"),
                    Some(DEFAULT_REMOVE_ORDER_POLL_ATTEMPTS),
                    Some(10),
                )
                .await
                .unwrap();
            assert_eq!(res.len(), 1);
        }

        #[tokio::test]
        async fn test_get_transaction_remove_orders_timeout() {
            let sg_server = MockServer::start_async().await;
            let _empty = sg_server.mock(|when, then| {
                when.path("/sg");
                then.status(200)
                    .json_body_obj(&empty_remove_orders_response());
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg").to_string(),
                    &sg_server.url("/sg").to_string(),
                    "http://localhost:3000",
                    "http://localhost:3000",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let err = raindex_client
                .get_remove_orders_for_transaction(
                    1,
                    Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    b256!("0x0000000000000000000000000000000000000000000000000000000000000123"),
                    Some(DEFAULT_REMOVE_ORDER_POLL_ATTEMPTS),
                    Some(10),
                )
                .await
                .unwrap_err();

            match err {
                RaindexError::TransactionIndexingTimeout { attempts, .. } => {
                    assert_eq!(attempts, DEFAULT_REMOVE_ORDER_POLL_ATTEMPTS);
                }
                other => panic!("expected timeout error, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn test_get_transaction_remove_orders_prefers_local_db() {
            let tx_hash =
                b256!("0x00000000000000000000000000000000000000000000000000000000deadbeef");
            let raindex_address =
                Address::from_str("0x0987654321098765432109876543210987654321").unwrap();
            let owner = Address::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();

            let local_order = LocalDbOrder {
                chain_id: 137,
                order_hash: b256!(
                    "0x0000000000000000000000000000000000000000000000000000000000000abc"
                ),
                owner,
                block_timestamp: 1_000,
                block_timestamp_removed: Some(1_000),
                block_number: 50,
                raindex_address,
                order_bytes: Bytes::from_str("0x01").unwrap(),
                transaction_hash: tx_hash,
                inputs: None,
                outputs: None,
                trade_count: 0,
                active: false, // removed orders are inactive
                meta: None,
            };

            let local_exec = CountingJsonExec {
                json: serde_json::to_string(&vec![local_order]).unwrap(),
                calls: Arc::new(AtomicUsize::new(0)),
            };
            let local_calls = local_exec.calls.clone();
            let local_db = LocalDb::new(local_exec);

            let sg_server = MockServer::start_async().await;
            let sg_mock = sg_server.mock(|when, then| {
                when.path("/sg2");
                then.status(200)
                    .json_body_obj(&empty_remove_orders_response());
            });

            let client = new_with_local_db(
                vec![get_test_yaml(
                    &sg_server.url("/sg1").to_string(),
                    &sg_server.url("/sg2").to_string(),
                    "http://localhost:3000",
                    "http://localhost:3000",
                )],
                local_db,
                vec![137],
            )
            .await;

            let res = client
                .get_remove_orders_for_transaction(137, raindex_address, tx_hash, Some(3), Some(1))
                .await
                .unwrap();

            assert_eq!(res.len(), 1);
            assert_eq!(sg_mock.hits(), 0, "subgraph should not be queried");
            assert_eq!(local_calls.load(Ordering::SeqCst), 1);

            let tx = res[0]
                .transaction()
                .expect("local db should populate transaction");
            assert_eq!(tx.id(), tx_hash);
            assert_eq!(tx.block_number(), U256::from(50));
            assert_eq!(tx.timestamp(), U256::from(1_000));
        }

        #[tokio::test]
        async fn test_get_transaction_remove_orders_exhausts_local_without_fallback() {
            let tx_hash =
                b256!("0x00000000000000000000000000000000000000000000000000000000cafebabe");
            let raindex_address =
                Address::from_str("0x0987654321098765432109876543210987654321").unwrap();

            let local_exec = CountingJsonExec {
                json: "[]".to_string(),
                calls: Arc::new(AtomicUsize::new(0)),
            };
            let local_calls = local_exec.calls.clone();
            let local_db = LocalDb::new(local_exec);

            let sg_server = MockServer::start_async().await;
            let sg_mock = sg_server.mock(|when, then| {
                when.path("/sg2");
                then.status(200)
                    .json_body_obj(&sample_remove_orders_response());
            });

            let client = new_with_local_db(
                vec![get_test_yaml(
                    &sg_server.url("/sg1").to_string(),
                    &sg_server.url("/sg2").to_string(),
                    "http://localhost:3000",
                    "http://localhost:3000",
                )],
                local_db,
                vec![137],
            )
            .await;

            let err = client
                .get_remove_orders_for_transaction(137, raindex_address, tx_hash, Some(2), Some(1))
                .await
                .unwrap_err();

            assert!(
                matches!(err, RaindexError::TransactionIndexingTimeout { .. }),
                "should timeout without falling back to subgraph"
            );
            assert_eq!(
                local_calls.load(Ordering::SeqCst),
                2,
                "local DB should be tried for the full attempt budget"
            );
            assert_eq!(
                sg_mock.hits(),
                0,
                "subgraph should not be queried when local DB is the source"
            );
        }
    }
}
