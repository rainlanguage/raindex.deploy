use crate::{BatchQuoteSpec, BatchQuoteTarget, QuoteSpec, QuoteTarget};
use alloy::primitives::{
    hex::{decode, FromHex},
    Address, U256,
};
use alloy::sol_types::SolType;
use clap::Args;
use raindex_bindings::IRaindexV7::{OrderV4, QuoteV3};
use std::str::FromStr;

/// Group of valid input formats
/// Only one of them can be passed at a time in cli
#[derive(Args, Clone, Debug, PartialEq)]
#[group(required = true, multiple = false)]
pub struct Input {
    /// Quote specs concated bytes
    #[arg(
        short,
        long,
        env,
        value_name = "HEX_STRING",
        requires = "subgraph",
        value_parser = parse_input
    )]
    pub input: Option<BatchQuoteSpec>,

    /// A Quote Target input that takes exactly 4 values
    #[arg(
        short,
        long,
        num_args = 4,
        value_names = [
            "RAINDEX_ADDRESS",
            "INPUT_IO_INDEX",
            "OUTPUT_IO_INDEX",
            "ORDER_BYTES"
        ],
    )]
    pub target: Option<Vec<String>>,

    /// A Quote Spec input that takes exactly 4 values
    #[arg(
        long,
        num_args = 4,
        value_names = [
            "RAINDEX_ADDRESS",
            "INPUT_IO_INDEX",
            "OUTPUT_IO_INDEX",
            "ORDER_HASH"
        ],
    )]
    pub spec: Option<Vec<String>>,
}

/// Determines the variants of parsed json input
#[derive(Debug, Clone, PartialEq)]
pub enum InputContentType {
    /// quote specs that need to read their order details from a subgraph before a quote call
    Spec(BatchQuoteSpec),
    // ready to quote targets that have all the details for a quote call
    Target(BatchQuoteTarget),
}

impl Input {
    /// Reads the input content from the provided source
    pub fn read_content(&self) -> anyhow::Result<InputContentType> {
        let mut inputs_count = 0;
        if self.input.is_some() {
            inputs_count += 1;
        }
        if self.target.is_some() {
            inputs_count += 1;
        }
        if self.spec.is_some() {
            inputs_count += 1;
        }
        if inputs_count > 1 {
            Err(anyhow::anyhow!("conflicting inputs"))
        } else if let Some(v) = &self.input {
            Ok(InputContentType::Spec(v.clone()))
        } else if let Some(targets) = &self.target {
            Ok(InputContentType::Target(targets.try_into()?))
        } else if let Some(specs) = &self.spec {
            Ok(InputContentType::Spec(specs.try_into()?))
        } else {
            Err(anyhow::anyhow!("expected at least one input"))
        }
    }
}

/// Parse and validates the input hex string bytes into [BatchQuoteSpec]
pub fn parse_input(value: &str) -> anyhow::Result<BatchQuoteSpec> {
    let bytes = alloy::primitives::hex::decode(value)?;
    if bytes.is_empty() || bytes.len() % 54 != 0 {
        return Err(anyhow::anyhow!("bad input length"));
    }
    let mut batch_quote_sepcs = BatchQuoteSpec(vec![]);
    let mut start_index = 0;
    let mut end_index = 54;
    while let Some(bytes_piece) = bytes.get(start_index..end_index) {
        batch_quote_sepcs.0.push(QuoteSpec {
            signed_context: vec![],
            raindex: bytes_piece
                .get(..20)
                .map(Address::from_slice)
                .ok_or(anyhow::anyhow!("missing raindex address"))?,
            input_io_index: bytes_piece
                .get(20..21)
                .map(|v| v[0])
                .ok_or(anyhow::anyhow!("missing input IO index"))?,
            output_io_index: bytes_piece
                .get(21..22)
                .map(|v| v[0])
                .ok_or(anyhow::anyhow!("missing output IO index"))?,
            order_hash: bytes_piece
                .get(22..)
                .map(U256::from_be_slice)
                .ok_or(anyhow::anyhow!("missing order hash"))?,
        });
        start_index += 54;
        end_index += 54;
    }
    Ok(batch_quote_sepcs)
}

// tries to map an array of strings into a BatchQuoteTarget
impl TryFrom<&Vec<String>> for BatchQuoteTarget {
    type Error = anyhow::Error;
    fn try_from(value: &Vec<String>) -> Result<Self, Self::Error> {
        let mut batch_quote_target = BatchQuoteTarget::default();
        let mut iter = value.iter();

        while let Some(raindex_str) = iter.next() {
            let input_io_index_str = match iter.next() {
                Some(s) => s,
                None => return Err(anyhow::anyhow!("missing input IO index")),
            };
            let output_io_index_str = match iter.next() {
                Some(s) => s,
                None => return Err(anyhow::anyhow!("missing output IO index")),
            };
            let order_bytes_str = match iter.next() {
                Some(s) => s,
                None => return Err(anyhow::anyhow!("missing order bytes")),
            };

            batch_quote_target.0.push(QuoteTarget {
                raindex: Address::from_hex(raindex_str)?,
                quote_config: QuoteV3 {
                    signedContext: vec![],
                    inputIOIndex: U256::from_str(input_io_index_str)?,
                    outputIOIndex: U256::from_str(output_io_index_str)?,
                    order: OrderV4::abi_decode(&decode(order_bytes_str)?)?,
                },
            });
        }

        if batch_quote_target.0.is_empty() {
            return Err(anyhow::anyhow!("missing '--target' values"));
        }
        Ok(batch_quote_target)
    }
}

// tries to map an array of strings into a BatchQuoteSpec
impl TryFrom<&Vec<String>> for BatchQuoteSpec {
    type Error = anyhow::Error;
    fn try_from(value: &Vec<String>) -> Result<Self, Self::Error> {
        let mut batch_quote_specs = BatchQuoteSpec::default();
        let mut iter = value.iter();
        while let Some(raindex_str) = iter.next() {
            let input_io_index_str = match iter.next() {
                Some(s) => s,
                None => return Err(anyhow::anyhow!("missing input IO index")),
            };
            let output_io_index_str = match iter.next() {
                Some(s) => s,
                None => return Err(anyhow::anyhow!("missing output IO index")),
            };
            let order_hash_str = match iter.next() {
                Some(s) => s,
                None => return Err(anyhow::anyhow!("missing order hash")),
            };

            batch_quote_specs.0.push(QuoteSpec {
                signed_context: vec![],
                raindex: Address::from_hex(raindex_str)?,
                order_hash: U256::from_str(order_hash_str)?,
                input_io_index: input_io_index_str.parse()?,
                output_io_index: output_io_index_str.parse()?,
            });
        }
        if batch_quote_specs.0.is_empty() {
            return Err(anyhow::anyhow!("missing '--spec' values"));
        }
        Ok(batch_quote_specs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::hex::encode_prefixed;
    use alloy::sol_types::SolValue;
    use raindex_bindings::IRaindexV7::{EvaluableV4, OrderV4};

    #[test]
    fn test_parse_input() {
        let raindex_address1 = Address::random();
        let input_io_index1 = 10u8;
        let output_io_index1 = 8u8;
        let order_hash1 = [5u8; 32];

        let raindex_address2 = Address::random();
        let input_io_index2 = 1u8;
        let output_io_index2 = 2u8;
        let order_hash2 = [2u8; 32];

        let mut bytes = vec![];
        bytes.extend(raindex_address1.0 .0);
        bytes.push(input_io_index1);
        bytes.push(output_io_index1);
        bytes.extend(order_hash1);
        bytes.extend(raindex_address2.0 .0);
        bytes.push(input_io_index2);
        bytes.push(output_io_index2);
        bytes.extend(order_hash2);

        let hex_bytes = encode_prefixed(&bytes);

        let result = parse_input(&hex_bytes).unwrap();
        let expected = BatchQuoteSpec(vec![
            QuoteSpec {
                order_hash: U256::from_be_bytes(order_hash1),
                input_io_index: input_io_index1,
                output_io_index: output_io_index1,
                signed_context: vec![],
                raindex: raindex_address1,
            },
            QuoteSpec {
                order_hash: U256::from_be_bytes(order_hash2),
                input_io_index: input_io_index2,
                output_io_index: output_io_index2,
                signed_context: vec![],
                raindex: raindex_address2,
            },
        ]);
        assert_eq!(result, expected);

        let hex_bytes = encode_prefixed(&bytes[2..]);
        let result = parse_input(&hex_bytes)
            .expect_err("expected to error")
            .to_string();
        assert_eq!(result, "bad input length");

        assert!(parse_input("some non bytes input").is_err());
    }

    #[test]
    fn test_try_from_vec_string_for_batch_quote_target() {
        // valid targets
        let input_index = 8u8;
        let output_index = 9u8;
        let raindex1 = Address::random();
        let raindex2 = Address::random();
        let order1 = OrderV4 {
            evaluable: EvaluableV4 {
                bytecode: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0].into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let order2 = OrderV4 {
            evaluable: EvaluableV4 {
                bytecode: vec![0xa, 0xb, 0xc, 0xd, 0xe, 0xf].into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let order_bytes1 = order1.abi_encode();
        let order_bytes2 = order2.abi_encode();

        let targets_str = vec![
            encode_prefixed(raindex1.0),
            input_index.to_string(),
            output_index.to_string(),
            encode_prefixed(order_bytes1.clone()),
            encode_prefixed(raindex2.0),
            input_index.to_string(),
            output_index.to_string(),
            encode_prefixed(order_bytes2),
        ];

        let result: BatchQuoteTarget = (&targets_str).try_into().unwrap();
        let expected = BatchQuoteTarget(vec![
            QuoteTarget {
                raindex: raindex1,
                quote_config: QuoteV3 {
                    inputIOIndex: U256::from(input_index),
                    outputIOIndex: U256::from(output_index),
                    signedContext: vec![],
                    order: order1,
                },
            },
            QuoteTarget {
                raindex: raindex2,
                quote_config: QuoteV3 {
                    inputIOIndex: U256::from(input_index),
                    outputIOIndex: U256::from(output_index),
                    signedContext: vec![],
                    order: order2,
                },
            },
        ]);
        assert_eq!(result, expected);

        // invalid targets
        let targets_str = vec![
            encode_prefixed(raindex1.0),
            input_index.to_string(),
            output_index.to_string(),
            encode_prefixed(order_bytes1),
            encode_prefixed(raindex2.0),
            input_index.to_string(),
            output_index.to_string(),
        ];
        let result = std::convert::TryInto::<BatchQuoteTarget>::try_into(&targets_str)
            .expect_err("expected error")
            .to_string();
        assert_eq!(result, "missing order bytes");

        let targets_str = vec![encode_prefixed(raindex1.0), input_index.to_string()];
        let result = std::convert::TryInto::<BatchQuoteTarget>::try_into(&targets_str)
            .expect_err("expected error")
            .to_string();
        assert_eq!(result, "missing output IO index");
    }

    #[test]
    fn test_try_from_vec_string_for_batch_quote_spec() {
        // valid targets
        let input_index = 8u8;
        let output_index = 9u8;
        let raindex1 = Address::random();
        let raindex2 = Address::random();
        let order_hash1 = [1u8; 32];
        let order_hash2 = [2u8; 32];

        let specs_str = vec![
            encode_prefixed(raindex1.0),
            input_index.to_string(),
            output_index.to_string(),
            encode_prefixed(order_hash1),
            encode_prefixed(raindex2.0),
            input_index.to_string(),
            output_index.to_string(),
            encode_prefixed(order_hash2),
        ];

        let result: BatchQuoteSpec = (&specs_str).try_into().unwrap();
        let expected = BatchQuoteSpec(vec![
            QuoteSpec {
                raindex: raindex1,
                input_io_index: input_index,
                output_io_index: output_index,
                signed_context: vec![],
                order_hash: U256::from_be_bytes(order_hash1),
            },
            QuoteSpec {
                raindex: raindex2,
                input_io_index: input_index,
                output_io_index: output_index,
                signed_context: vec![],
                order_hash: U256::from_be_bytes(order_hash2),
            },
        ]);
        assert_eq!(result, expected);

        // invalid targets
        let specs_str = vec![
            encode_prefixed(raindex1.0),
            input_index.to_string(),
            output_index.to_string(),
            encode_prefixed([1u8; 32]),
            encode_prefixed(raindex2.0),
            input_index.to_string(),
            output_index.to_string(),
        ];
        let result = std::convert::TryInto::<BatchQuoteSpec>::try_into(&specs_str)
            .expect_err("expected error")
            .to_string();
        assert_eq!(result, "missing order hash");

        let specs_str = vec![encode_prefixed(raindex1.0), input_index.to_string()];
        let result = std::convert::TryInto::<BatchQuoteSpec>::try_into(&specs_str)
            .expect_err("expected error")
            .to_string();
        assert_eq!(result, "missing output IO index");
    }

    #[test]
    fn test_read_content() {
        let raindex = Address::random();
        let input_io_index = 10u8;
        let output_io_index = 8u8;
        let order_hash = [5u8; 32];

        let specs = BatchQuoteSpec(vec![QuoteSpec {
            order_hash: U256::from_be_bytes(order_hash),
            input_io_index,
            output_io_index,
            signed_context: vec![],
            raindex,
        }]);
        let input = Input {
            input: Some(specs.clone()),
            target: None,
            spec: None,
        };
        matches!(input.read_content().unwrap(), InputContentType::Spec(_));

        let targets_str = vec![
            encode_prefixed(raindex.0),
            input_io_index.to_string(),
            output_io_index.to_string(),
            encode_prefixed(OrderV4::default().abi_encode()),
        ];
        let input = Input {
            input: None,
            target: Some(targets_str.clone()),
            spec: None,
        };
        matches!(input.read_content().unwrap(), InputContentType::Target(_));

        let specs_str = vec![
            encode_prefixed(raindex.0),
            input_io_index.to_string(),
            output_io_index.to_string(),
            encode_prefixed([1u8; 32]),
        ];
        let input = Input {
            input: None,
            spec: Some(specs_str.clone()),
            target: None,
        };
        matches!(input.read_content().unwrap(), InputContentType::Spec(_));

        let input = Input {
            input: None,
            target: None,
            spec: None,
        };
        assert_eq!(
            input
                .read_content()
                .expect_err("expected error")
                .to_string(),
            "expected at least one input"
        );

        let input = Input {
            input: Some(specs),
            target: Some(targets_str),
            spec: None,
        };
        assert_eq!(
            input
                .read_content()
                .expect_err("expected error")
                .to_string(),
            "conflicting inputs"
        );
    }
}
