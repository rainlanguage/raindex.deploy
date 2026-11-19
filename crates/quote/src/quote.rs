use crate::{
    error::{Error, FailedQuote},
    rpc::batch_quote,
};
use alloy::primitives::{
    hex::{decode, encode_prefixed},
    keccak256, Address, B256, U256,
};
use alloy::sol_types::SolValue;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, str::FromStr};
use url::Url;
use wasm_bindgen_utils::{add_ts_content, impl_wasm_traits, prelude::*};

use rain_math_float::Float;
use raindex_bindings::IRaindexV7::{quote3Return, OrderV4, QuoteV3, SignedContextV2};
use raindex_subgraph_client::{
    types::{common::SgBytes, Id},
    utils::make_order_id,
    RaindexSubgraphClient,
};

pub type QuoteResult = Result<OrderQuoteValue, FailedQuote>;
add_ts_content!("export type QuoteResult = OrderQuoteValue | string");

/// Holds quoted order max output and ratio
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub struct OrderQuoteValue {
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub max_output: Float,
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub ratio: Float,
}
#[cfg(target_family = "wasm")]
impl_wasm_traits!(OrderQuoteValue);

impl From<quote3Return> for OrderQuoteValue {
    fn from(v: quote3Return) -> Self {
        Self {
            max_output: Float::from_raw(v.outputMax),
            ratio: Float::from_raw(v.ioRatio),
        }
    }
}

/// A quote target
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub struct QuoteTarget {
    pub quote_config: QuoteV3,
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub raindex: Address,
}
#[cfg(target_family = "wasm")]
impl_wasm_traits!(QuoteTarget);

impl QuoteTarget {
    /// Get the order hash of self
    pub fn get_order_hash(&self) -> B256 {
        keccak256(self.quote_config.order.abi_encode())
    }

    /// Get subgraph represented "order_id" of self
    /// which is keccak256 of raindex address concated with order hash
    pub fn get_id(&self) -> B256 {
        make_order_id(self.raindex, self.get_order_hash().into())
    }

    /// Quotes the target on the given rpc urls
    pub async fn do_quote(
        &self,
        rpcs: Vec<String>,
        block_number: Option<u64>,
        counterparty: Address,
        chunk_size: Option<usize>,
    ) -> Result<QuoteResult, Error> {
        Ok(batch_quote(
            std::slice::from_ref(self),
            rpcs,
            block_number,
            counterparty,
            None,
            chunk_size,
        )
        .await?
        .into_iter()
        .next()
        .unwrap())
    }

    /// Validate the quote target
    /// Checks if the requested input and output indexes are valid
    pub fn validate(&self) -> Result<(), Error> {
        if self.quote_config.inputIOIndex >= U256::from(self.quote_config.order.validInputs.len()) {
            return Err(Error::InvalidQuoteTarget(self.quote_config.inputIOIndex));
        }
        if self.quote_config.outputIOIndex >= U256::from(self.quote_config.order.validOutputs.len())
        {
            return Err(Error::InvalidQuoteTarget(self.quote_config.outputIOIndex));
        }
        Ok(())
    }
}

/// Specifies a batch of [QuoteTarget]s
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Tsify)]
#[serde(transparent)]
#[serde(rename_all = "camelCase")]
pub struct BatchQuoteTarget(pub Vec<QuoteTarget>);
impl_wasm_traits!(BatchQuoteTarget);

impl BatchQuoteTarget {
    /// Quotes the targets in batch on the given rpc url
    pub async fn do_quote(
        &self,
        rpcs: Vec<String>,
        block_number: Option<u64>,
        counterparty: Address,
        chunk_size: Option<usize>,
    ) -> Result<Vec<QuoteResult>, Error> {
        batch_quote(&self.0, rpcs, block_number, counterparty, None, chunk_size).await
    }
}

/// A quote target specifier, where the order details need to be fetched from a
/// source (such as subgraph) to build a [QuoteTarget] out of it
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub struct QuoteSpec {
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub order_hash: U256,
    #[serde(rename = "inputIOIndex")]
    pub input_io_index: u8,
    #[serde(rename = "outputIOIndex")]
    pub output_io_index: u8,
    pub signed_context: Vec<SignedContextV2>,
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub raindex: Address,
}
#[cfg(target_family = "wasm")]
impl_wasm_traits!(QuoteSpec);

impl QuoteSpec {
    /// Get subgraph represented "order_id" of self
    /// which is keccak256 of raindex address concated with order hash
    pub fn get_id(&self) -> B256 {
        make_order_id(self.raindex, self.order_hash)
    }

    /// Given a subgraph will fetch the order details and returns the
    /// respective quote target
    pub async fn get_quote_target_from_subgraph(
        &self,
        subgraph_url: &str,
    ) -> Result<QuoteTarget, Error> {
        let url = Url::from_str(subgraph_url)?;
        let sg_client = RaindexSubgraphClient::new(url);
        let order_detail = sg_client
            .order_detail(&Id::new(encode_prefixed(self.get_id())))
            .await?;

        Ok(QuoteTarget {
            raindex: self.raindex,
            quote_config: QuoteV3 {
                inputIOIndex: U256::from(self.input_io_index),
                outputIOIndex: U256::from(self.output_io_index),
                signedContext: self.signed_context.clone(),
                order: OrderV4::abi_decode(
                    decode(order_detail.order_bytes.0.as_str())?.as_slice(),
                )?,
            },
        })
    }

    /// Given a subgraph url, will fetch the order details from the subgraph and
    /// then quotes it using the given rpc urls.
    pub async fn do_quote(
        &self,
        subgraph_url: &str,
        rpcs: Vec<String>,
        block_number: Option<u64>,
        counterparty: Address,
        chunk_size: Option<usize>,
    ) -> Result<QuoteResult, Error> {
        let quote_target = self.get_quote_target_from_subgraph(subgraph_url).await?;
        let quote_result = batch_quote(
            &[quote_target],
            rpcs,
            block_number,
            counterparty,
            None,
            chunk_size,
        )
        .await?;

        Ok(quote_result.into_iter().next().unwrap())
    }
}

/// specifies a batch of [QuoteSpec]s
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, Tsify)]
#[serde(transparent)]
#[serde(rename_all = "camelCase")]
pub struct BatchQuoteSpec(pub Vec<QuoteSpec>);
impl_wasm_traits!(BatchQuoteSpec);

impl BatchQuoteSpec {
    /// Given a subgraph url, will fetch orders details and returns their
    /// respective quote targets.
    /// Those specifiers that were not in the subgraph are returned as None
    /// in the resturning array
    pub async fn get_batch_quote_target_from_subgraph(
        &self,
        subgraph_url: &str,
    ) -> Result<Vec<Option<QuoteTarget>>, Error> {
        let url = Url::from_str(subgraph_url)?;
        let sg_client = RaindexSubgraphClient::new(url);
        let orders_details = sg_client
            .batch_order_detail(
                self.0
                    .iter()
                    .map(|v| SgBytes(encode_prefixed(v.get_id())))
                    .collect(),
            )
            .await?;

        Ok(self
            .0
            .iter()
            .map(|target| {
                orders_details
                    .iter()
                    .find(|order_detail| order_detail.id.0 == encode_prefixed(target.get_id()))
                    .and_then(|order_detail| {
                        Some(QuoteTarget {
                            raindex: target.raindex,
                            quote_config: QuoteV3 {
                                inputIOIndex: U256::from(target.input_io_index),
                                outputIOIndex: U256::from(target.output_io_index),
                                signedContext: target.signed_context.clone(),
                                order: OrderV4::abi_decode(
                                    decode(order_detail.order_bytes.0.as_str()).ok()?.as_slice(),
                                )
                                .ok()?,
                            },
                        })
                    })
            })
            .collect())
    }

    /// Given a subgraph url, will fetch the order details from the subgraph and
    /// then quotes them using the given rpc url.
    /// Those orders that are not found from subgraph are excluded from quoting,
    /// and final result also leaves their place in the array as None
    pub async fn do_quote(
        &self,
        subgraph_url: &str,
        rpcs: Vec<String>,
        block_number: Option<u64>,
        counterparty: Address,
        chunk_size: Option<usize>,
    ) -> Result<Vec<QuoteResult>, Error> {
        let opts_quote_targets = self
            .get_batch_quote_target_from_subgraph(subgraph_url)
            .await?;

        // quote the valid quote targets
        let quote_targets: Vec<QuoteTarget> = opts_quote_targets
            .iter()
            .filter_map(|v| v.clone())
            .collect();
        let mut quote_results = VecDeque::from(
            batch_quote(
                &quote_targets,
                rpcs,
                block_number,
                counterparty,
                None,
                chunk_size,
            )
            .await?,
        );

        // fill the array with quote results and invalid quote targets following
        // their original order
        let mut result = vec![];
        opts_quote_targets.iter().for_each(|v| {
            if v.is_some() {
                result.push(
                    quote_results
                        .pop_front()
                        .unwrap_or(Err(FailedQuote::NonExistent)),
                );
            } else {
                result.push(Err(FailedQuote::NonExistent))
            }
        });

        Ok(result)
    }
}

#[cfg(not(target_family = "wasm"))]
#[cfg(test)]
mod tests {
    use super::*;
    use alloy::hex::ToHexExt;
    use alloy::primitives::keccak256;
    use alloy::primitives::Bytes;
    use alloy::primitives::{hex::encode_prefixed, U256};
    use alloy::sol_types::{SolCall, SolValue};
    use httpmock::{Method::POST, MockServer};
    use rain_error_decoding::AbiDecodedErrorType;
    use raindex_bindings::IRaindexV7::{quote3Call, QuoteV3, IOV2};
    use raindex_subgraph_client::RaindexSubgraphClientError;
    use serde_json::{json, Value};

    // helper fn to build some test data
    fn get_test_data(batch: bool) -> (Address, OrderV4, U256, Value) {
        let raindex = Address::random();
        let order = OrderV4 {
            validInputs: vec![IOV2::default()],
            validOutputs: vec![IOV2::default()],
            ..Default::default()
        };
        let order_hash_bytes = keccak256(order.abi_encode()).0;
        let order_hash_u256 = U256::from_be_bytes(order_hash_bytes);
        let order_hash = encode_prefixed(order_hash_bytes);
        let mut id = vec![];
        id.extend_from_slice(raindex.as_ref());
        id.extend_from_slice(&order_hash_bytes);
        let order_id = encode_prefixed(keccak256(id));
        let order_json = json!({
            "id": order_id,
            "orderBytes": encode_prefixed(order.abi_encode()),
            "orderHash": order_hash,
            "owner": encode_prefixed(order.owner),
            "outputs": [{
                "id": encode_prefixed(Address::random().0.0),
                "owner": encode_prefixed(order.owner),
                "token": {
                    "id": encode_prefixed(order.validOutputs[0].token.0.0),
                    "address": encode_prefixed(order.validOutputs[0].token.0.0),
                    "name": "T1",
                    "symbol": "T1",
                    "decimals": "0"
                },
                "balance": "0",
                "vaultId": order.validOutputs[0].vaultId.to_string(),
                "raindex": { "id": encode_prefixed(B256::random()) },
                "ordersAsOutput": [{
                    "id": encode_prefixed(B256::random()),
                    "orderHash": encode_prefixed(B256::random()),
                    "active": true
                }],
                "ordersAsInput": [{
                    "id": encode_prefixed(B256::random()),
                    "orderHash": encode_prefixed(B256::random()),
                    "active": true
                }],
                "balanceChanges": [{
                    "__typename": "Withdrawal",
                    "id": encode_prefixed(B256::random()),
                    "amount": "0",
                    "newVaultBalance": "0",
                    "oldVaultBalance": "0",
                    "vault": {
                        "id": encode_prefixed(B256::random()),
                        "vaultId": encode_prefixed(B256::random()),
                        "token": {
                            "id": encode_prefixed(order.validOutputs[0].token.0.0),
                            "address": encode_prefixed(order.validOutputs[0].token.0.0),
                            "name": "T1",
                            "symbol": "T1",
                            "decimals": "0"
                        },
                    },
                    "timestamp": "0",
                    "transaction": {
                        "id": encode_prefixed(B256::random()),
                        "blockNumber": "0",
                        "timestamp": "0",
                        "from": encode_prefixed(Address::random())
                    },
                    "raindex": { "id": encode_prefixed(B256::random()) }
                }],
            }],
            "inputs": [{
                "id": encode_prefixed(Address::random().0.0),
                "owner": encode_prefixed(order.owner),
                "token": {
                    "id": encode_prefixed(order.validInputs[0].token.0.0),
                    "address": encode_prefixed(order.validInputs[0].token.0.0),
                    "name": "T2",
                    "symbol": "T2",
                    "decimals": "0"
                },
                "balance": "0",
                "vaultId": order.validInputs[0].vaultId.to_string(),
                "raindex": { "id": encode_prefixed(B256::random()) },
                "ordersAsOutput": [{
                    "id": encode_prefixed(B256::random()),
                    "orderHash": encode_prefixed(B256::random()),
                    "active": true
                }],
                "ordersAsInput": [{
                    "id": encode_prefixed(B256::random()),
                    "orderHash": encode_prefixed(B256::random()),
                    "active": true
                }],
                "balanceChanges": [{
                    "__typename": "Withdrawal",
                    "id": encode_prefixed(B256::random()),
                    "amount": "0",
                    "newVaultBalance": "0",
                    "oldVaultBalance": "0",
                    "vault": {
                        "id": encode_prefixed(B256::random()),
                        "vaultId": encode_prefixed(B256::random()),
                        "token": {
                            "id": encode_prefixed(order.validOutputs[0].token.0.0),
                            "address": encode_prefixed(order.validOutputs[0].token.0.0),
                            "name": "T1",
                            "symbol": "T1",
                            "decimals": "0"
                        },
                    },
                    "timestamp": "0",
                    "transaction": {
                        "id": encode_prefixed(B256::random()),
                        "blockNumber": "0",
                        "timestamp": "0",
                        "from": encode_prefixed(Address::random())
                    },
                    "raindex": { "id": encode_prefixed(B256::random()) }
                }],
            }],
            "raindex": { "id": encode_prefixed(B256::random()) },
            "active": true,
            "addEvents": [{
                "transaction": {
                    "id": encode_prefixed(B256::random()),
                    "blockNumber": "0",
                    "timestamp": "0",
                    "from": encode_prefixed(Address::random())
                }
            }],
            "meta": null,
            "timestampAdded": "0",
            "trades": [],
            "removeEvents": []
        });
        let retrun_sg_data = if batch {
            json!({
                "data": {
                    "orders": [order_json]
                }
            })
        } else {
            json!({
                "data": {
                    "order": order_json
                }
            })
        };
        (raindex, order, order_hash_u256, retrun_sg_data)
    }

    #[test]
    fn test_quote_target_get_order_hash() {
        let (raindex, order, _, _) = get_test_data(false);
        let quote_target = QuoteTarget {
            quote_config: QuoteV3 {
                order,
                ..Default::default()
            },
            raindex,
        };
        let actual = quote_target.get_order_hash().encode_hex();
        let expected =
            "89a108449cd7a8de4e7061645c1dc7ffe8ff2163eb6eff847efd9b1ad1f39934".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_quote_target_get_id() {
        let quote_target = QuoteTarget {
            quote_config: Default::default(),
            raindex: Address::ZERO,
        };
        let actual = quote_target.get_id().encode_hex();
        let expected =
            "3c220b0ff68b48f69ef802b5d39e6942218a1b843a1845ade53d1e2412135b63".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_quote_spec_get_id() {
        let quote_spec = QuoteSpec {
            order_hash: U256::from(0_u16),
            input_io_index: 0,
            output_io_index: 0,
            signed_context: Vec::new(),
            raindex: Address::ZERO,
        };
        let actual = quote_spec.get_id().encode_hex();
        let expected =
            "a86d54e9aab41ae5e520ff0062ff1b4cbd0b2192bb01080a058bb170d84e6457".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_validate_ok() {
        let (raindex, order, _, _) = get_test_data(false);
        let quote_target = QuoteTarget {
            quote_config: QuoteV3 {
                order,
                ..Default::default()
            },
            raindex,
        };
        assert!(quote_target.validate().is_ok());
    }

    #[test]
    fn test_validate_err() {
        let quote_target = QuoteTarget {
            quote_config: QuoteV3 {
                order: OrderV4::default(),
                outputIOIndex: U256::from(1_u16),
                ..Default::default()
            },
            raindex: Address::ZERO,
        };
        assert!(quote_target.validate().is_err());

        let quote_target = QuoteTarget {
            quote_config: QuoteV3 {
                order: OrderV4::default(),
                inputIOIndex: U256::from(1_u16),
                ..Default::default()
            },
            raindex: Address::ZERO,
        };
        assert!(quote_target.validate().is_err());
    }

    #[tokio::test]
    async fn test_get_quote_spec_from_subgraph_ok() {
        let rpc_server = MockServer::start_async().await;

        let (raindex, order, order_id_u256, retrun_sg_data) = get_test_data(false);

        // mock subgraph
        rpc_server.mock(|when, then| {
            when.method(POST).path("/");
            then.json_body_obj(&retrun_sg_data);
        });

        let quote_target_specifier = QuoteSpec {
            order_hash: order_id_u256,
            input_io_index: 0,
            output_io_index: 0,
            signed_context: vec![],
            raindex,
        };
        let result = quote_target_specifier
            .get_quote_target_from_subgraph(rpc_server.url("/").as_str())
            .await
            .unwrap();

        let expected = QuoteTarget {
            raindex,
            quote_config: QuoteV3 {
                order,
                inputIOIndex: U256::from(quote_target_specifier.input_io_index),
                outputIOIndex: U256::from(quote_target_specifier.output_io_index),
                signedContext: quote_target_specifier.signed_context,
            },
        };

        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_get_quote_spec_from_subgraph_err() {
        let (raindex, _, order_id_u256, _) = get_test_data(false);

        let quote_target_specifier = QuoteSpec {
            order_hash: order_id_u256,
            input_io_index: 0,
            output_io_index: 0,
            signed_context: vec![],
            raindex,
        };

        let err = quote_target_specifier
            .get_quote_target_from_subgraph("this will break")
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            Error::UrlParseError(url::ParseError::RelativeUrlWithoutBase)
        ));

        let rpc_server = MockServer::start_async().await;

        rpc_server.mock(|when, then| {
            when.method(POST).path("/404");
            then.status(404);
        });

        let err = quote_target_specifier
            .get_quote_target_from_subgraph(rpc_server.url("/404").as_str())
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            Error::SubgraphClientError(RaindexSubgraphClientError::CynicClientError(_))
        ));
    }

    #[tokio::test]
    async fn test_get_batch_quote_spec_from_subgraph_ok() {
        let rpc_server = MockServer::start_async().await;

        let (raindex, order, order_id_u256, retrun_sg_data) = get_test_data(true);

        // mock subgraph
        rpc_server.mock(|when, then| {
            when.method(POST).path("/");
            then.json_body_obj(&retrun_sg_data);
        });

        let batch_quote_targets_specifiers = BatchQuoteSpec(vec![QuoteSpec {
            order_hash: order_id_u256,
            input_io_index: 0,
            output_io_index: 0,
            signed_context: vec![],
            raindex,
        }]);
        let result = batch_quote_targets_specifiers
            .get_batch_quote_target_from_subgraph(rpc_server.url("/").as_str())
            .await
            .unwrap();

        let expected = vec![Some(QuoteTarget {
            raindex,
            quote_config: QuoteV3 {
                order,
                inputIOIndex: U256::from(batch_quote_targets_specifiers.0[0].input_io_index),
                outputIOIndex: U256::from(batch_quote_targets_specifiers.0[0].output_io_index),
                signedContext: batch_quote_targets_specifiers.0[0].signed_context.clone(),
            },
        })];

        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_get_batch_quote_spec_from_subgraph_err() {
        let rpc_server = MockServer::start_async().await;

        let (raindex, order, order_id_u256, _) = get_test_data(true);

        rpc_server.mock(|when, then| {
            when.method(POST).path("/sg");

            let invalid_order_json = json!({
                "id": encode_prefixed(B256::random()),
                "orderBytes": encode_prefixed(order.abi_encode()),
                "orderHash": encode_prefixed(B256::random()),
                "owner": encode_prefixed(order.owner),
                "raindex": { "id": encode_prefixed(B256::random()) },
                "active": true,
                "addEvents": [],
                "meta": null,
                "timestampAdded": "0",
                "trades": [],
                "removeEvents": []
            });

            then.json_body_obj(&json!({
                "data": {
                    "orders": [invalid_order_json]
                }
            }));
        });

        let batch_quote_targets_specifiers = BatchQuoteSpec(vec![QuoteSpec {
            order_hash: order_id_u256,
            input_io_index: 0,
            output_io_index: 0,
            signed_context: vec![],
            raindex,
        }]);

        let err = batch_quote_targets_specifiers
            .get_batch_quote_target_from_subgraph(rpc_server.url("/sg").as_str())
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            Error::SubgraphClientError(RaindexSubgraphClientError::CynicClientError(cynic_err))
            if cynic_err.to_string().contains("error decoding response body")
        ));
    }

    #[tokio::test]
    async fn test_quote_spec_do_quote_ok() {
        let rpc_server = MockServer::start_async().await;

        let (raindex, _, order_id_u256, retrun_sg_data) = get_test_data(false);

        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();

        // build response data
        let inner_return: Bytes = quote3Call::abi_encode_returns(&quote3Return {
            exists: true,
            outputMax: one.get_inner(),
            ioRatio: two.get_inner(),
        })
        .into();
        let response_data = <Vec<Bytes> as SolValue>::abi_encode(&vec![inner_return]);

        // mock rpc with call data and response data
        rpc_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body_obj(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_prefixed(response_data).as_str(),
            }));
        });

        // mock subgraph
        rpc_server.mock(|when, then| {
            when.method(POST).path("/sg");
            then.json_body_obj(&retrun_sg_data);
        });

        let quote_target_specifier = QuoteSpec {
            order_hash: order_id_u256,
            input_io_index: 0,
            output_io_index: 0,
            signed_context: vec![],
            raindex,
        };

        let result = quote_target_specifier
            .do_quote(
                rpc_server.url("/sg").as_str(),
                vec![rpc_server.url("/rpc").to_string()],
                None,
                Address::ZERO,
                None,
            )
            .await
            .unwrap();

        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();

        let quote = result.unwrap();

        assert!(quote.max_output.eq(one).unwrap());
        assert!(quote.ratio.eq(two).unwrap());
    }

    #[tokio::test]
    async fn test_quote_spec_do_quote_err() {
        let server = MockServer::start_async().await;

        let (raindex, _, order_id_u256, retrun_sg_data) = get_test_data(false);

        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();

        let inner_return: Bytes = quote3Call::abi_encode_returns(&quote3Return {
            exists: true,
            outputMax: one.get_inner(),
            ioRatio: two.get_inner(),
        })
        .into();
        let response_data = <Vec<Bytes> as SolValue>::abi_encode(&vec![inner_return]);

        server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body_obj(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_prefixed(response_data).as_str(),
            }));
        });

        server.mock(|when, then| {
            when.method(POST).path("/bad-rpc");
            then.json_body_obj(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": "not good data",
            }));
        });

        server.mock(|when, then| {
            when.method(POST).path("/sg");
            then.json_body_obj(&retrun_sg_data);
        });

        server.mock(|when, then| {
            when.method(POST).path("/bad-sg");
            then.json_body_obj(&json!({ "data": null }));
        });

        let quote_target_specifier = QuoteSpec {
            order_hash: order_id_u256,
            input_io_index: 0,
            output_io_index: 0,
            signed_context: vec![],
            raindex,
        };

        // A malformed RPC response remains aligned with the requested quote.
        let result = quote_target_specifier
            .do_quote(
                server.url("/sg").as_str(),
                vec![server.url("/bad-rpc").to_string()],
                None,
                Address::ZERO,
                None,
            )
            .await
            .unwrap();
        assert!(matches!(
            result,
            Err(FailedQuote::CorruptReturnData(message))
                if message.contains("all quote RPCs failed")
        ));

        let err = quote_target_specifier
            .do_quote(
                server.url("/bad-sg").as_str(),
                vec![server.url("/rpc").to_string()],
                None,
                Address::ZERO,
                None,
            )
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            Error::SubgraphClientError(RaindexSubgraphClientError::CynicClientError(
                cynic_err,
            )) if cynic_err.to_string().contains("error decoding response body")
        ));
    }

    #[tokio::test]
    async fn test_quote_batch_spec_do_quote_err() {
        let rpc_server = MockServer::start_async().await;

        let (raindex, _, order_id_u256, retrun_sg_data) = get_test_data(true);

        // build response data
        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();

        let inner_return: Bytes = quote3Call::abi_encode_returns(&quote3Return {
            exists: true,
            outputMax: one.get_inner(),
            ioRatio: two.get_inner(),
        })
        .into();
        let response_data = <Vec<Bytes> as SolValue>::abi_encode(&vec![inner_return]);

        // mock rpc with call data and response data
        rpc_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body_obj(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_prefixed(response_data).as_str(),
            }));
        });

        // mock subgraph
        rpc_server.mock(|when, then| {
            when.method(POST).path("/sg");
            then.json_body_obj(&retrun_sg_data);
        });

        let batch_quote_targets_specifiers = BatchQuoteSpec(vec![
            QuoteSpec {
                order_hash: order_id_u256,
                input_io_index: 0,
                output_io_index: 0,
                signed_context: vec![],
                raindex,
            },
            // should be Err in final result
            QuoteSpec::default(),
            QuoteSpec::default(),
        ]);

        // Endpoint failures stay aligned with the quote targets that reached RPC.
        let bad_rpc_url = rpc_server.url("/bad-rpc").to_string();
        let results = batch_quote_targets_specifiers
            .do_quote(
                rpc_server.url("/sg").as_str(),
                vec![bad_rpc_url.clone()],
                None,
                Address::ZERO,
                None,
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 3);
        assert!(matches!(
            &results[0],
            Err(FailedQuote::CorruptReturnData(message))
                if message.contains("all quote RPCs failed")
        ));
        assert!(matches!(&results[1], Err(FailedQuote::NonExistent)));
        assert!(matches!(&results[2], Err(FailedQuote::NonExistent)));

        let result = batch_quote_targets_specifiers
            .do_quote(
                rpc_server.url("/sg").as_str(),
                vec![rpc_server.url("/rpc").to_string()],
                None,
                Address::ZERO,
                None,
            )
            .await
            .unwrap();

        assert_eq!(result.len(), 3);

        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();

        let quote = result[0].as_ref().unwrap();
        assert!(quote.max_output.eq(one).unwrap());
        assert!(quote.ratio.eq(two).unwrap());

        // specifiers that were not present on subgraph were not quoted and are Err
        assert!(result[1].is_err());
        assert!(result[2].is_err());
    }

    #[tokio::test]
    async fn test_quote_target_do_quote_ok() {
        let rpc_server = MockServer::start_async().await;

        let (raindex, order, _, _) = get_test_data(false);
        let quote_target = QuoteTarget {
            quote_config: QuoteV3 {
                order,
                ..Default::default()
            },
            raindex,
        };

        // OZ Multicall returns `bytes[]`; each element is the ABI-encoded
        // return of one inner `quote3` call.
        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();

        let inner_return: Bytes = quote3Call::abi_encode_returns(&quote3Return {
            exists: true,
            outputMax: one.get_inner(),
            ioRatio: two.get_inner(),
        })
        .into();
        let response_data = <Vec<Bytes> as SolValue>::abi_encode(&vec![inner_return]);

        // mock rpc with call data and response data
        rpc_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body_obj(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_prefixed(response_data).as_str(),
            }));
        });

        let result = quote_target
            .do_quote(
                vec![rpc_server.url("/rpc").to_string()],
                None,
                Address::ZERO,
                None,
            )
            .await
            .unwrap()
            .unwrap();

        assert!(result.max_output.eq(one).unwrap());
        assert!(result.ratio.eq(two).unwrap());
    }

    #[tokio::test]
    async fn test_quote_target_do_quote_err() {
        let rpc_server = MockServer::start_async().await;

        let (raindex, order, _, _) = get_test_data(false);
        let quote_target = QuoteTarget {
            quote_config: QuoteV3 {
                order,
                ..Default::default()
            },
            raindex,
        };

        // Outer multicall decodes fine (one `bytes` element) but that inner
        // bytes is not a valid `quote3Return`, so the per-target decode fails
        // and we get `FailedQuote::CorruptReturnData` — not a top-level Error.
        let corrupt_inner: Bytes = Bytes::from_static(b"corrupt data");
        let response_data = <Vec<Bytes> as SolValue>::abi_encode(&vec![corrupt_inner]);

        rpc_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body_obj(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_prefixed(response_data).as_str(),
            }));
        });

        let result = quote_target
            .do_quote(
                vec![rpc_server.url("/rpc").to_string()],
                None,
                Address::ZERO,
                None,
            )
            .await
            .unwrap();

        assert!(
            matches!(result, Err(FailedQuote::CorruptReturnData(_))),
            "unexpected result: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_batch_quote_target_do_quote_ok() {
        let rpc_server = MockServer::start_async().await;

        let (raindex, order, _, _) = get_test_data(true);
        let quote_targets = BatchQuoteTarget(vec![QuoteTarget {
            quote_config: QuoteV3 {
                order,
                ..Default::default()
            },
            raindex,
        }]);

        // OZ Multicall returns `bytes[]`; one element per inner `quote3`.
        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();

        let inner_return: Bytes = quote3Call::abi_encode_returns(&quote3Return {
            exists: true,
            outputMax: one.get_inner(),
            ioRatio: two.get_inner(),
        })
        .into();
        let response_data = <Vec<Bytes> as SolValue>::abi_encode(&vec![inner_return]);

        rpc_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body_obj(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_prefixed(response_data).as_str(),
            }));
        });

        let result = quote_targets
            .do_quote(
                vec![rpc_server.url("/rpc").to_string()],
                None,
                Address::ZERO,
                None,
            )
            .await
            .unwrap();

        assert_eq!(result.len(), 1);

        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();

        let quote = result[0].as_ref().unwrap();

        assert!(quote.max_output.eq(one).unwrap());
        assert!(quote.ratio.eq(two).unwrap());
    }

    #[tokio::test]
    async fn test_batch_quote_target_do_quote_err() {
        let rpc_server = MockServer::start_async().await;

        let (raindex, order, _, _) = get_test_data(true);
        let quote_targets = BatchQuoteTarget(vec![QuoteTarget {
            quote_config: QuoteV3 {
                order,
                ..Default::default()
            },
            raindex,
        }]);

        // The raindex's OZ Multicall bubbles the first inner revert up as
        // the outer eth_call revert data. We simulate `TokenSelfTrade()`
        // (selector 0x734bc71c) being bubbled up directly.
        rpc_server.mock(|when, then| {
            when.method(POST).path("/reverted-rpc");
            then.json_body_obj(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "error": {
                    "code": 3,
                    "message": "execution reverted",
                    "data": "0x734bc71c",
                }
            }));
        });

        let results = quote_targets
            .do_quote(
                vec![rpc_server.url("/reverted-rpc").to_string()],
                None,
                Address::ZERO,
                None,
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);

        let err = results.into_iter().next().unwrap().unwrap_err();

        assert!(matches!(
            err,
            FailedQuote::RevertError(AbiDecodedErrorType::Known { name, .. })
            if name == "TokenSelfTrade"
        ));
    }
}
