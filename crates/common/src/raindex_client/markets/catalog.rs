use super::*;
use crate::raindex_client::vaults::RaindexVaultToken;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexedToken {
    address: Address,
    name: Option<String>,
    symbol: Option<String>,
    decimals: u8,
}

impl From<RaindexVaultToken> for IndexedToken {
    fn from(token: RaindexVaultToken) -> Self {
        Self {
            address: token.raw_address(),
            name: token.raw_name().map(ToOwned::to_owned),
            symbol: token.raw_symbol().map(ToOwned::to_owned),
            decimals: token.raw_decimals(),
        }
    }
}

#[derive(Debug, Clone)]
struct IndexedOrder {
    chain_id: u32,
    raindex: Address,
    inputs: Vec<IndexedToken>,
    outputs: Vec<IndexedToken>,
}

impl From<RaindexOrder> for IndexedOrder {
    fn from(order: RaindexOrder) -> Self {
        Self {
            chain_id: order.chain_id(),
            raindex: order.raw_raindex(),
            inputs: order
                .input_vaults()
                .iter()
                .map(|vault| vault.token().into())
                .collect(),
            outputs: order
                .output_vaults()
                .iter()
                .map(|vault| vault.token().into())
                .collect(),
        }
    }
}

struct MarketAccumulator {
    base: RaindexMarketToken,
    quote: RaindexMarketToken,
    raindex_addresses: BTreeSet<Address>,
}

pub(super) async fn discover_markets(
    client: &RaindexClient,
    options: &MarketListOptions,
) -> Result<Vec<RaindexMarket>, RaindexError> {
    let registry_tokens = client.get_all_tokens()?.into_values().collect::<Vec<_>>();
    let quotes = quote_tokens(&registry_tokens, options)?;
    if quotes.is_empty() {
        return Ok(Vec::new());
    }

    let chain_ids = quotes.keys().copied().collect::<Vec<_>>();
    let quote_addresses = quotes
        .values()
        .map(|quote| quote.address)
        .collect::<Vec<_>>();
    let orders = fetch_active_orders(client, chain_ids, quote_addresses).await?;
    markets_from_indexed_orders(
        orders.into_iter().map(IndexedOrder::from),
        &registry_tokens,
        &quotes,
        options,
    )
}

async fn fetch_active_orders(
    client: &RaindexClient,
    chain_ids: Vec<u32>,
    quote_addresses: Vec<Address>,
) -> Result<Vec<RaindexOrder>, RaindexError> {
    let mut orders = Vec::new();
    for filters in active_order_filters(quote_addresses) {
        orders.extend(fetch_market_orders_paginated(client, chain_ids.clone(), filters).await?);
    }
    Ok(orders)
}

fn active_order_filters(quote_addresses: Vec<Address>) -> [GetOrdersFilters; 2] {
    [
        GetOrdersFilters {
            active: Some(true),
            tokens: Some(GetOrdersTokenFilter {
                inputs: Some(quote_addresses.clone()),
                outputs: None,
            }),
            ..Default::default()
        },
        GetOrdersFilters {
            active: Some(true),
            tokens: Some(GetOrdersTokenFilter {
                inputs: None,
                outputs: Some(quote_addresses),
            }),
            ..Default::default()
        },
    ]
}

fn quote_tokens(
    tokens: &[TokenCfg],
    options: &MarketListOptions,
) -> Result<BTreeMap<u32, TokenCfg>, RaindexError> {
    let selected_chains = options
        .chain_ids
        .as_ref()
        .map(|ids| ids.iter().copied().collect::<HashSet<_>>());
    let mut quotes = BTreeMap::<u32, BTreeMap<Address, TokenCfg>>::new();
    for token in tokens.iter().filter(|token| {
        extension_bool(token, MARKET_QUOTE_EXTENSION)
            && selected_chains
                .as_ref()
                .is_none_or(|chains| chains.contains(&token.network.chain_id))
    }) {
        quotes
            .entry(token.network.chain_id)
            .or_default()
            .entry(token.address)
            .or_insert_with(|| token.clone());
    }

    quotes
        .into_iter()
        .map(|(chain_id, tokens)| match tokens.into_values().collect::<Vec<_>>().as_slice() {
            [quote] => Ok((chain_id, quote.clone())),
            _ => Err(RaindexError::PreflightError(format!(
                "multiple tokens with {MARKET_QUOTE_EXTENSION}=true are configured for chain {chain_id}"
            ))),
        })
        .collect()
}

fn markets_from_indexed_orders(
    orders: impl IntoIterator<Item = IndexedOrder>,
    registry_tokens: &[TokenCfg],
    quotes: &BTreeMap<u32, TokenCfg>,
    options: &MarketListOptions,
) -> Result<Vec<RaindexMarket>, RaindexError> {
    let registry_lookup = registry_token_lookup(registry_tokens)?;
    let mut accumulators = BTreeMap::<(u32, Address), MarketAccumulator>::new();

    for order in orders {
        let Some(quote_cfg) = quotes.get(&order.chain_id) else {
            continue;
        };
        let quote_address = quote_cfg.address;
        let quote = market_token(quote_cfg)?;

        if order
            .inputs
            .iter()
            .any(|token| token.address == quote_address)
        {
            for token in order
                .outputs
                .iter()
                .filter(|token| token.address != quote_address)
            {
                add_market(
                    &mut accumulators,
                    order.chain_id,
                    order.raindex,
                    token,
                    quote.clone(),
                    &registry_lookup,
                )?;
            }
        }
        if order
            .outputs
            .iter()
            .any(|token| token.address == quote_address)
        {
            for token in order
                .inputs
                .iter()
                .filter(|token| token.address != quote_address)
            {
                add_market(
                    &mut accumulators,
                    order.chain_id,
                    order.raindex,
                    token,
                    quote.clone(),
                    &registry_lookup,
                )?;
            }
        }
    }

    let selected_tickers = options
        .ticker_ids
        .as_ref()
        .map(|ids| ids.iter().cloned().collect::<HashSet<_>>());
    let mut markets = accumulators
        .into_values()
        .map(|accumulator| {
            let id = format!(
                "{}:{:#x}:{:#x}",
                accumulator.base.chain_id, accumulator.base.address, accumulator.quote.address
            );
            let ticker_id = format!(
                "{:#x}_{:#x}",
                accumulator.base.address, accumulator.quote.address
            );
            RaindexMarket {
                id,
                ticker_id,
                chain_id: accumulator.base.chain_id,
                base: accumulator.base,
                quote: accumulator.quote,
                raindex_addresses: accumulator.raindex_addresses.into_iter().collect(),
            }
        })
        .filter(|market| {
            selected_tickers
                .as_ref()
                .is_none_or(|tickers| tickers.contains(&market.ticker_id))
        })
        .collect::<Vec<_>>();
    markets.sort_by(|a, b| {
        (a.chain_id, a.base.symbol.as_str(), a.base.address).cmp(&(
            b.chain_id,
            b.base.symbol.as_str(),
            b.base.address,
        ))
    });
    Ok(markets)
}

fn add_market(
    markets: &mut BTreeMap<(u32, Address), MarketAccumulator>,
    chain_id: u32,
    raindex: Address,
    indexed_token: &IndexedToken,
    quote: RaindexMarketToken,
    registry_lookup: &HashMap<(u32, Address), TokenCfg>,
) -> Result<(), RaindexError> {
    let base = match registry_lookup.get(&(chain_id, indexed_token.address)) {
        Some(token) => market_token(token)?,
        None => indexed_market_token(chain_id, indexed_token),
    };
    if base.address == quote.address {
        return Ok(());
    }
    markets
        .entry((chain_id, base.address))
        .and_modify(|market| {
            market.raindex_addresses.insert(raindex);
        })
        .or_insert_with(|| MarketAccumulator {
            base,
            quote,
            raindex_addresses: BTreeSet::from([raindex]),
        });
    Ok(())
}

fn registry_token_lookup(
    tokens: &[TokenCfg],
) -> Result<HashMap<(u32, Address), TokenCfg>, RaindexError> {
    let mut sorted = tokens.to_vec();
    sorted.sort_by_key(|token| (token.network.chain_id, token.address, token.key.clone()));
    let mut lookup = sorted
        .iter()
        .map(|token| ((token.network.chain_id, token.address), token.clone()))
        .collect::<HashMap<_, _>>();

    // Variant declarations intentionally override an exact-address registry entry:
    // an indexed legacy or underlying token still belongs to the declared canonical market.
    for token in sorted {
        for address in [
            extension_address(&token, "unwrappedAddress")?,
            extension_address(&token, "legacyAddress")?,
        ]
        .into_iter()
        .flatten()
        {
            lookup.insert((token.network.chain_id, address), token.clone());
        }
    }
    Ok(lookup)
}

fn indexed_market_token(chain_id: u32, token: &IndexedToken) -> RaindexMarketToken {
    let address = token.address;
    RaindexMarketToken {
        chain_id,
        address,
        name: token
            .name
            .clone()
            .or_else(|| token.symbol.clone())
            .unwrap_or_else(|| format!("{address:#x}")),
        symbol: token
            .symbol
            .clone()
            .or_else(|| token.name.clone())
            .unwrap_or_else(|| format!("{address:#x}")),
        decimals: Some(token.decimals),
        logo_uri: None,
        extensions: None,
        unwrapped_address: None,
        legacy_address: None,
        receipt_address: None,
        variants: vec![address],
    }
}

fn market_token(token: &TokenCfg) -> Result<RaindexMarketToken, RaindexError> {
    let unwrapped_address = extension_address(token, "unwrappedAddress")?;
    let legacy_address = extension_address(token, "legacyAddress")?;
    let receipt_address = extension_address(token, "receiptAddress")?;
    let mut variants = [Some(token.address), unwrapped_address, legacy_address]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    variants.sort_unstable();
    variants.dedup();

    Ok(RaindexMarketToken {
        chain_id: token.network.chain_id,
        address: token.address,
        name: token.label.clone().unwrap_or_else(|| token.key.clone()),
        symbol: token.symbol.clone().unwrap_or_else(|| token.key.clone()),
        decimals: token.decimals,
        logo_uri: token.logo_uri.as_ref().map(ToString::to_string),
        extensions: token.extensions.clone(),
        unwrapped_address,
        legacy_address,
        receipt_address,
        variants,
    })
}

fn extension_string<'a>(token: &'a TokenCfg, key: &str) -> Option<&'a str> {
    token
        .extensions
        .as_ref()
        .and_then(|extensions| extensions.get(key))
        .and_then(Value::as_str)
}

fn extension_bool(token: &TokenCfg, key: &str) -> bool {
    token
        .extensions
        .as_ref()
        .and_then(|extensions| extensions.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn extension_address(token: &TokenCfg, key: &str) -> Result<Option<Address>, RaindexError> {
    extension_string(token, key)
        .map(|address| {
            address.parse::<Address>().map_err(|error| {
                RaindexError::PreflightError(format!(
                    "token {} has an invalid {key}: {error}",
                    token.key
                ))
            })
        })
        .transpose()
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use httpmock::MockServer;

    const USDC: &str = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913";
    const CANONICAL: &str = "0xfb5b41acdba20a3230f84be995173cfb98b8d6e7";
    const UNDERLYING: &str = "0x7271a3c91bb6070ed09333b84a815949d4f16d14";
    const GENERIC: &str = "0x1111111111111111111111111111111111111111";
    const WETH: &str = "0x4200000000000000000000000000000000000006";
    const RAINDEX: &str = "0xe522cb4a5fcb2eb31a52ff41a4653d85a4fd7c9d";

    async fn client_with_tokens(tokens: Value) -> RaindexClient {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method("GET").path("/tokens");
                then.status(200).json_body(serde_json::json!({
                    "name": "Markets",
                    "timestamp": "2026-01-01T00:00:00Z",
                    "version": { "major": 1, "minor": 0, "patch": 0 },
                    "tokens": tokens
                }));
            })
            .await;
        let yaml = format!(
            r#"
version: 6
networks:
  base:
    rpcs:
      - http://localhost:8545
    chain-id: 8453
subgraphs:
  base: http://localhost:8080/subgraph
raindexes:
  base:
    address: {RAINDEX}
    network: base
    subgraph: base
    deployment-block: 1
using-tokens-from:
  - {}/tokens
"#,
            server.base_url()
        );
        RaindexClient::new(vec![yaml], None, None).await.unwrap()
    }

    fn token(address: &str, symbol: &str, extensions: Value) -> Value {
        serde_json::json!({
            "chainId": 8453,
            "address": address,
            "name": symbol,
            "symbol": symbol,
            "decimals": 18,
            "logoURI": format!("https://example.com/{symbol}.png"),
            "extensions": extensions
        })
    }

    fn indexed(address: &str, symbol: &str, decimals: u8) -> IndexedToken {
        IndexedToken {
            address: address.parse().unwrap(),
            name: Some(symbol.to_string()),
            symbol: Some(symbol.to_string()),
            decimals,
        }
    }

    fn order(inputs: Vec<IndexedToken>, outputs: Vec<IndexedToken>) -> IndexedOrder {
        IndexedOrder {
            chain_id: 8453,
            raindex: RAINDEX.parse().unwrap(),
            inputs,
            outputs,
        }
    }

    async fn registry_parts(tokens: Value) -> (Vec<TokenCfg>, BTreeMap<u32, TokenCfg>) {
        let client = client_with_tokens(tokens).await;
        let tokens = client
            .get_all_tokens()
            .unwrap()
            .into_values()
            .collect::<Vec<_>>();
        let quotes = quote_tokens(&tokens, &MarketListOptions::default()).unwrap();
        (tokens, quotes)
    }

    #[test]
    fn discovery_queries_only_active_orders() {
        let quote = USDC.parse().unwrap();
        let filters = active_order_filters(vec![quote]);
        assert!(filters.iter().all(|filter| filter.active == Some(true)));
        assert_eq!(
            filters[0]
                .tokens
                .as_ref()
                .and_then(|tokens| tokens.inputs.as_ref()),
            Some(&vec![quote])
        );
        assert_eq!(
            filters[1]
                .tokens
                .as_ref()
                .and_then(|tokens| tokens.outputs.as_ref()),
            Some(&vec![quote])
        );
    }

    #[tokio::test]
    async fn discovers_direct_quote_pairs_and_ignores_unrelated_pairs() {
        let (registry, quotes) = registry_parts(serde_json::json!([token(
            USDC,
            "USDC",
            serde_json::json!({ "marketQuote": true })
        ),]))
        .await;
        let orders = vec![
            order(
                vec![indexed(USDC, "USDC", 6)],
                vec![indexed(GENERIC, "GEN", 8)],
            ),
            order(
                vec![indexed(WETH, "WETH", 18)],
                vec![indexed(GENERIC, "GEN", 8)],
            ),
        ];

        let markets =
            markets_from_indexed_orders(orders, &registry, &quotes, &MarketListOptions::default())
                .unwrap();

        assert_eq!(markets.len(), 1);
        assert_eq!(markets[0].base.address, GENERIC.parse::<Address>().unwrap());
        assert_eq!(markets[0].base.symbol, "GEN");
        assert_eq!(markets[0].base.decimals, Some(8));
        assert!(markets[0].base.logo_uri.is_none());
    }

    #[tokio::test]
    async fn registry_enriches_and_canonicalizes_an_indexed_variant() {
        let (registry, quotes) = registry_parts(serde_json::json!([
            token(USDC, "USDC", serde_json::json!({ "marketQuote": true })),
            token(
                CANONICAL,
                "WRAPPED",
                serde_json::json!({
                    "unwrappedAddress": UNDERLYING,
                    "opaqueMetadata": "preserved"
                })
            )
        ]))
        .await;
        let orders = vec![order(
            vec![indexed(UNDERLYING, "TOKEN", 18)],
            vec![indexed(USDC, "USDC", 6)],
        )];

        let markets =
            markets_from_indexed_orders(orders, &registry, &quotes, &MarketListOptions::default())
                .unwrap();

        assert_eq!(markets.len(), 1);
        assert_eq!(
            markets[0].base.address,
            CANONICAL.parse::<Address>().unwrap()
        );
        assert_eq!(markets[0].base.symbol, "WRAPPED");
        assert_eq!(
            markets[0]
                .base
                .extensions
                .as_ref()
                .and_then(|extensions| extensions.get("opaqueMetadata"))
                .and_then(Value::as_str),
            Some("preserved")
        );
        assert!(markets[0].base.logo_uri.is_some());
    }

    #[tokio::test]
    async fn deduplicates_markets_and_collects_only_contributing_raindexes() {
        let second_raindex = "0x2222222222222222222222222222222222222222";
        let (registry, quotes) = registry_parts(serde_json::json!([token(
            USDC,
            "USDC",
            serde_json::json!({ "marketQuote": true })
        ),]))
        .await;
        let mut second = order(
            vec![indexed(GENERIC, "GEN", 8)],
            vec![indexed(USDC, "USDC", 6)],
        );
        second.raindex = second_raindex.parse().unwrap();

        let markets = markets_from_indexed_orders(
            vec![
                order(
                    vec![indexed(USDC, "USDC", 6)],
                    vec![indexed(GENERIC, "GEN", 8)],
                ),
                second,
            ],
            &registry,
            &quotes,
            &MarketListOptions::default(),
        )
        .unwrap();

        assert_eq!(markets.len(), 1);
        assert_eq!(markets[0].raindex_addresses.len(), 2);
    }

    #[tokio::test]
    async fn ticker_filter_is_applied_after_active_market_discovery() {
        let (registry, quotes) = registry_parts(serde_json::json!([token(
            USDC,
            "USDC",
            serde_json::json!({ "marketQuote": true })
        ),]))
        .await;
        let ticker_id = format!("{GENERIC}_{USDC}");
        let options = MarketListOptions {
            ticker_ids: Some(vec![ticker_id]),
            ..Default::default()
        };

        let markets = markets_from_indexed_orders(
            vec![order(
                vec![indexed(USDC, "USDC", 6)],
                vec![indexed(GENERIC, "GEN", 8)],
            )],
            &registry,
            &quotes,
            &options,
        )
        .unwrap();

        assert_eq!(markets.len(), 1);
    }

    #[tokio::test]
    async fn no_quote_marker_produces_no_markets() {
        let client = client_with_tokens(serde_json::json!([token(
            GENERIC,
            "GEN",
            serde_json::json!({})
        )]))
        .await;
        let tokens = client
            .get_all_tokens()
            .unwrap()
            .into_values()
            .collect::<Vec<_>>();

        assert!(quote_tokens(&tokens, &MarketListOptions::default())
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn rejects_ambiguous_quote_tokens() {
        let client = client_with_tokens(serde_json::json!([
            token(USDC, "USDC", serde_json::json!({ "marketQuote": true })),
            token(GENERIC, "USDT", serde_json::json!({ "marketQuote": true }))
        ]))
        .await;
        let tokens = client
            .get_all_tokens()
            .unwrap()
            .into_values()
            .collect::<Vec<_>>();

        let error = quote_tokens(&tokens, &MarketListOptions::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("multiple tokens with marketQuote=true"));
    }

    #[tokio::test]
    async fn rejects_malformed_variant_addresses() {
        let client = client_with_tokens(serde_json::json!([
            token(USDC, "USDC", serde_json::json!({ "marketQuote": true })),
            token(
                CANONICAL,
                "WRAPPED",
                serde_json::json!({ "unwrappedAddress": "not-an-address" })
            )
        ]))
        .await;
        let tokens = client
            .get_all_tokens()
            .unwrap()
            .into_values()
            .collect::<Vec<_>>();

        let error = registry_token_lookup(&tokens).unwrap_err().to_string();
        assert!(error.contains("invalid unwrappedAddress"));
    }
}
