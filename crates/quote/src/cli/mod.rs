use crate::{OrderQuoteValue, QuoteResult};
use alloy::primitives::Address;
use clap::{ArgAction, Parser};
use serde::{Deserialize, Serialize};
use std::{fs::write, io::Write, path::PathBuf};
use url::Url;

mod input;
pub use input::*;

/// Rain raindex Quoter CLI app entrypoint sruct
#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "Rain Raindex Quote CLI", long_about = None)]
pub struct Quoter {
    // input group, only one of which can be specified at a time
    #[command(flatten)]
    pub input: Input,

    /// RPC URL of the evm chain to quote
    #[arg(short, long, env, value_name = "URL", hide_env_values = true)]
    pub rpc: Url,

    /// Subgraph URL to read orders details from, presence of this
    /// arg determines what type input's undelying content should be in
    #[arg(
        short,
        long,
        env,
        value_name = "URL",
        visible_alias = "sg",
        hide_env_values = true
    )]
    pub subgraph: Option<Url>,

    /// Optional block number to quote at
    #[arg(short, long, env, value_name = "INTEGER")]
    pub block_number: Option<u64>,

    /// Counterparty address used as `from` of each `eth_call` when quoting.
    /// This is what `msg.sender` / `order-counterparty()` will see inside
    /// `calculate-io`. Defaults to the zero address.
    #[arg(short, long, env, value_name = "ADDRESS", default_value_t = Address::ZERO)]
    pub counterparty: Address,

    /// Optional file path to write the output results into
    #[arg(short, long, env, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Do NOT send the results to stdout
    #[arg(long, action = ArgAction::SetTrue)]
    pub no_stdout: bool,

    /// Pretty format the result
    #[arg(short, long, action = ArgAction::SetTrue)]
    pub pretty: bool,
}

/// A serializable/deserializable struct that bridges [QuoteResult] for cli
/// output by implementing `From` trait for it.
/// This is is needed since [crate::error::FailedQuote] does not impl ser/deser.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "status", content = "message")]
pub enum QuoterResultInner {
    Error(String),
    #[serde(untagged)]
    Ok(OrderQuoteValue),
}

impl From<QuoteResult> for QuoterResultInner {
    fn from(value: QuoteResult) -> Self {
        match value {
            Ok(v) => Self::Ok(v),
            Err(e) => Self::Error(e.to_string()),
        }
    }
}

/// Wrapper struct for array of [QuoterResultInner]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QuoterResult(pub Vec<QuoterResultInner>);

impl From<Vec<QuoteResult>> for QuoterResult {
    fn from(value: Vec<QuoteResult>) -> Self {
        Self(value.into_iter().map(|v| v.into()).collect())
    }
}

impl Quoter {
    /// Executes the CLI call based on the given options of self
    pub async fn run(&self) -> anyhow::Result<QuoterResult> {
        let result: QuoterResult = match self.input.read_content()? {
            InputContentType::Target(v) => v
                .do_quote(
                    vec![self.rpc.to_string()],
                    self.block_number,
                    self.counterparty,
                    None,
                )
                .await?
                .into(),
            InputContentType::Spec(v) => {
                if let Some(sg) = &self.subgraph {
                    v.do_quote(
                        sg.as_str(),
                        vec![self.rpc.to_string()],
                        self.block_number,
                        self.counterparty,
                        None,
                    )
                    .await?
                    .into()
                } else {
                    return Err(anyhow::anyhow!(
                        "requires '--subgraph' url to read orders details from"
                    ));
                }
            }
        };

        if !self.no_stdout || self.output.is_some() {
            let stringified_result = if self.pretty {
                serde_json::to_string_pretty::<QuoterResult>(&result)?
            } else {
                serde_json::to_string::<QuoterResult>(&result)?
            };
            if !self.no_stdout {
                let mut stdout = std::io::stdout().lock();
                stdout.write_all(stringified_result.as_bytes())?;
            }
            if let Some(v) = &self.output {
                write(v, stringified_result)?;
            }
        }

        Ok(result)
    }
}

/// The main entrypoint for this crate's cli
pub async fn main() -> anyhow::Result<()> {
    tracing::subscriber::set_global_default(tracing_subscriber::fmt::Subscriber::new())?;
    let cli = Quoter::parse();
    cli.run().await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::FailedQuote, BatchQuoteSpec, QuoteSpec};
    use alloy::primitives::{hex::encode_prefixed, keccak256, B256, U256};
    use alloy::sol_types::{SolCall, SolValue};
    use clap::CommandFactory;
    use httpmock::{Method::POST, MockServer};
    use rain_math_float::Float;
    use raindex_bindings::IRaindexV7::{quote3Call, quote3Return, OrderV4, IOV2};
    use serde_json::json;
    use std::{fs::read_to_string, str::FromStr};
    use tempfile::NamedTempFile;

    #[test]
    fn verify_cli() {
        Quoter::command().debug_assert();
    }

    #[test]
    fn test_cli_args() {
        let rpc = Url::parse("https://rpc.com").unwrap();
        let sg = Url::parse("https://sg.com").unwrap();
        let output = PathBuf::from_str("./a/b").unwrap();

        let batch_quote_specs = BatchQuoteSpec(vec![QuoteSpec {
            raindex: Address::random(),
            input_io_index: 0,
            output_io_index: 0,
            order_hash: U256::from(1),
            signed_context: vec![],
        }]);
        let mut bytes = vec![];
        bytes.extend(batch_quote_specs.0[0].raindex.0);
        bytes.push(batch_quote_specs.0[0].input_io_index);
        bytes.push(batch_quote_specs.0[0].output_io_index);
        bytes.extend(batch_quote_specs.0[0].order_hash.to_be_bytes_vec());
        let hex_bytes = encode_prefixed(&bytes);
        let cmd = Quoter::command();
        let result = cmd
            .try_get_matches_from(vec![
                "cmd",
                "--output",
                output.clone().to_str().unwrap(),
                "--rpc",
                rpc.as_str(),
                "-i",
                &hex_bytes,
                "--sg",
                sg.as_str(),
            ])
            .unwrap();
        assert_eq!(result.get_one::<PathBuf>("output"), Some(&output));
        assert_eq!(result.get_one::<Url>("subgraph"), Some(&sg));
        assert_eq!(result.get_one::<Url>("rpc"), Some(&rpc));
        assert_eq!(
            result.get_one::<BatchQuoteSpec>("input"),
            Some(&batch_quote_specs)
        );

        let raindex1 = encode_prefixed(Address::random().0);
        let raindex2 = encode_prefixed(Address::random().0);
        let order_bytes1 = encode_prefixed(OrderV4::default().abi_encode());
        let order_bytes2 = encode_prefixed(OrderV4::default().abi_encode());
        let input_index = U256::from(8).to_string();
        let output_index = U256::from(9).to_string();
        let cmd = Quoter::command();
        let result = cmd.get_matches_from(vec![
            "cmd",
            "--output",
            output.clone().to_str().unwrap(),
            "--rpc",
            rpc.as_str(),
            "--target",
            &raindex1,
            &input_index,
            &output_index,
            &order_bytes1,
            "--target",
            &raindex2,
            &input_index,
            &output_index,
            &order_bytes2,
        ]);
        assert_eq!(result.get_one::<PathBuf>("output"), Some(&output));
        assert_eq!(result.get_one::<Url>("rpc"), Some(&rpc));
        assert_eq!(
            result
                .get_occurrences("target")
                .unwrap()
                .map(Iterator::collect)
                .collect::<Vec<Vec<&String>>>(),
            vec![
                vec![&raindex1, &input_index, &output_index, &order_bytes1],
                vec![&raindex2, &input_index, &output_index, &order_bytes2]
            ]
        );

        let cmd = Quoter::command();
        assert!(cmd
            .try_get_matches_from(vec![
                "cmd",
                "--output",
                output.clone().to_str().unwrap(),
                "--target",
                &raindex1,
                &input_index,
                &output_index,
                &order_bytes1,
                "--sg",
                sg.as_str(),
            ])
            .is_err());
    }

    #[tokio::test]
    async fn test_run_err() {
        let cli = Quoter {
            output: Some(PathBuf::new()),
            rpc: Url::parse("http://a.com").unwrap(),
            subgraph: None,
            block_number: None,
            counterparty: Address::ZERO,
            no_stdout: true,
            pretty: true,
            input: Input {
                target: None,
                spec: None,
                input: Some(BatchQuoteSpec(vec![
                    QuoteSpec::default(),
                    QuoteSpec::default(),
                ])),
            },
        };
        let result = cli.run().await.expect_err("expected error").to_string();
        assert_eq!(
            result,
            "requires '--subgraph' url to read orders details from"
        );
    }

    #[tokio::test]
    async fn test_run_ok_spec_inputs() {
        let rpc_server = MockServer::start_async().await;
        let rpc_url = rpc_server.url("/rpc");
        let sg_url = rpc_server.url("/sg");

        // Only one of the two specs resolves to a QuoteTarget (the default
        // spec is missing from the subgraph → yields `NonExistent` before
        // any RPC call), so the raindex-native multicall response carries
        // a single element.
        let exists_true: alloy::primitives::Bytes = quote3Call::abi_encode_returns(&quote3Return {
            exists: true,
            outputMax: Float::default().get_inner(),
            ioRatio: Float::default().get_inner(),
        })
        .into();
        let rpc_response_data =
            <Vec<alloy::primitives::Bytes> as SolValue>::abi_encode(&vec![exists_true]);

        rpc_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body_obj(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_prefixed(rpc_response_data).as_str(),
            }));
        });

        // mock subgraph
        let raindex = Address::random();
        let order = OrderV4 {
            validInputs: vec![IOV2::default()],
            validOutputs: vec![IOV2::default()],
            ..Default::default()
        };
        let order_hash_bytes = keccak256(order.abi_encode()).0;
        let order_hash_u256 = U256::from_be_bytes(order_hash_bytes);
        let order_hash = encode_prefixed(order_hash_bytes);
        let mut order_id = vec![];
        order_id.extend_from_slice(raindex.as_ref());
        order_id.extend_from_slice(&order_hash_bytes);
        let order_id = encode_prefixed(keccak256(order_id));
        let retrun_sg_data = serde_json::json!({
            "data": {
                "orders": [{
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
                            "decimals": "6"
                        },
                        "balance": "0",
                        "vaultId": order.validOutputs[0].vaultId.to_string(),
                        "raindex": { "id": encode_prefixed(B256::random()) },
                        "ordersAsOutput": [],
                        "ordersAsInput": [],
                        "balanceChanges": []
                    }],
                    "inputs": [{
                        "id": encode_prefixed(Address::random().0.0),
                        "owner": encode_prefixed(order.owner),
                        "token": {
                            "id": encode_prefixed(order.validInputs[0].token.0.0),
                            "address": encode_prefixed(order.validInputs[0].token.0.0),
                            "name": "T2",
                            "symbol": "T2",
                            "decimals": "6"
                        },
                        "balance": "0",
                        "vaultId": order.validInputs[0].vaultId.to_string(),
                        "raindex": { "id": encode_prefixed(B256::random()) },
                        "ordersAsOutput": [],
                        "ordersAsInput": [],
                        "balanceChanges": []
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
                }]
            }
        });
        rpc_server.mock(|when, then| {
            when.method(POST).path("/sg");
            then.json_body_obj(&retrun_sg_data);
        });

        // input bytes
        let batch_quote_specs = BatchQuoteSpec(vec![
            QuoteSpec {
                order_hash: order_hash_u256,
                input_io_index: 0,
                output_io_index: 0,
                signed_context: vec![],
                raindex,
            },
            QuoteSpec::default(),
        ]);
        let cli = Quoter {
            output: None,
            rpc: Url::parse(&rpc_url).unwrap(),
            subgraph: Some(Url::parse(&sg_url).unwrap()),
            block_number: None,
            counterparty: Address::ZERO,
            no_stdout: true,
            pretty: false,
            input: Input {
                target: None,
                spec: None,
                input: Some(batch_quote_specs),
            },
        };

        // run
        let result = cli.run().await.unwrap();
        let expected = QuoterResult(vec![
            QuoterResultInner::Ok(OrderQuoteValue::default()),
            QuoterResultInner::Error(FailedQuote::NonExistent.to_string()),
        ]);
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );

        // specs input
        let specs_str = vec![
            encode_prefixed(raindex.0),
            0.to_string(),
            0.to_string(),
            encode_prefixed(order_hash_bytes),
            encode_prefixed(raindex.0),
            0.to_string(),
            0.to_string(),
            encode_prefixed([0u8; 32]),
        ];
        let cli = Quoter {
            output: None,
            rpc: Url::parse(&rpc_url).unwrap(),
            subgraph: Some(Url::parse(&sg_url).unwrap()),
            block_number: None,
            counterparty: Address::ZERO,
            no_stdout: true,
            pretty: false,
            input: Input {
                target: None,
                input: None,
                spec: Some(specs_str),
            },
        };

        // run
        let result = cli.run().await.unwrap();
        let expected = QuoterResult(vec![
            QuoterResultInner::Ok(OrderQuoteValue::default()),
            QuoterResultInner::Error(FailedQuote::NonExistent.to_string()),
        ]);
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }

    #[tokio::test]
    async fn test_run_ok_target_args() {
        let rpc_server = MockServer::start_async().await;
        let rpc_url = rpc_server.url("/rpc");
        let test_file = NamedTempFile::new().unwrap();
        let test_path = test_file.path().to_path_buf();

        let raindex = Address::random();
        let input_io_index = 0u8;
        let output_io_index = 0u8;
        let targets_str = vec![
            encode_prefixed(raindex.0),
            input_io_index.to_string(),
            output_io_index.to_string(),
            encode_prefixed(OrderV4::default().abi_encode()),
        ];

        let cli = Quoter {
            output: Some(test_path.clone()),
            rpc: Url::parse(&rpc_url).unwrap(),
            subgraph: None,
            block_number: None,
            counterparty: Address::ZERO,
            no_stdout: false,
            pretty: false,
            input: Input {
                input: None,
                spec: None,
                target: Some(targets_str),
            },
        };

        // The CLI `--target` flag here is parsed into a single QuoteTarget
        // (four arg tokens per target), so the raindex's multicall carries
        // a single inner `quote3` return.
        let exists_true: alloy::primitives::Bytes = quote3Call::abi_encode_returns(&quote3Return {
            exists: true,
            outputMax: Float::default().get_inner(),
            ioRatio: Float::default().get_inner(),
        })
        .into();
        let rpc_response_data =
            <Vec<alloy::primitives::Bytes> as SolValue>::abi_encode(&vec![exists_true]);
        rpc_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body_obj(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_prefixed(rpc_response_data).as_str(),
            }));
        });

        // run
        let QuoterResult(result) = cli.run().await.unwrap();
        assert_eq!(result.len(), 1);

        let quote = if let QuoterResultInner::Ok(v) = &result[0] {
            v
        } else {
            panic!("expected ok quote result");
        };
        assert!(quote.max_output.eq(Float::default()).unwrap());
        assert!(quote.ratio.eq(Float::default()).unwrap());

        let expected = QuoterResult(vec![QuoterResultInner::Ok(OrderQuoteValue::default())]);

        // output json format containing array of ok/err quote results:
        // [
        //     { "maxOutput": "0x0", "ratio": "0x0" },
        //     { "status": "error", "message": "Order does not exist" }
        // ]
        let result = read_to_string(test_path.clone()).unwrap();
        let expected = serde_json::to_string(&expected).unwrap();
        assert_eq!(result, expected);
    }
}
