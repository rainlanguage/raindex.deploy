use super::*;

struct PreparedOrder {
    order: RaindexOrder,
    decoded: OrderV4,
    pairs: Vec<raindex_quote::Pair>,
}

pub(super) async fn read_ratios(
    rpcs: &[url::Url],
    shares: Vec<Address>,
) -> Result<RatioRead, String> {
    if shares.is_empty() {
        return Ok(RatioRead::default());
    }
    let vaults = shares
        .into_iter()
        .map(Erc4626BatchVault::new)
        .collect::<Vec<_>>();
    if rpcs.is_empty() {
        return Err("no RPC URLs configured".to_string());
    }
    let expected_ratio_count = vaults.len();
    let mut failures = Vec::new();
    let mut best_partial = None;
    for rpc in rpcs {
        let provider = match mk_read_provider(std::slice::from_ref(rpc)) {
            Ok(provider) => provider,
            Err(error) => {
                failures.push(format!("{rpc}: {error}"));
                continue;
            }
        };
        let ratio_vaults = vaults.clone();
        let response = with_market_timeout(
            async move { erc4626::batch_share_ratios(&provider, ratio_vaults, None).await },
            RATIO_RPC_TIMEOUT_MS,
        )
        .await;
        match response {
            Some(Ok(response)) => {
                let read = ratio_read_from_response(response);
                if read.values.len() == expected_ratio_count {
                    return Ok(read);
                }
                failures.push(format!(
                    "{rpc}: returned {} usable ratios out of {expected_ratio_count}",
                    read.values.len()
                ));
                if best_partial.as_ref().is_none_or(|best: &RatioRead| {
                    read.values.len() > best.values.len()
                        || (read.values.len() == best.values.len()
                            && read.block_number > best.block_number)
                }) {
                    best_partial = Some(read);
                }
            }
            Some(Err(error)) => failures.push(format!("{rpc}: {error}")),
            None => failures.push(format!("{rpc}: timed out")),
        }
    }
    if let Some(read) = best_partial {
        return Ok(read);
    }
    Err(format!(
        "all RPC ratio reads failed: {}",
        failures.join("; ")
    ))
}

fn ratio_read_from_response(response: Erc4626BatchResponse) -> RatioRead {
    let block_number = response.block_number;
    let values = response
        .items
        .into_iter()
        .filter_map(|item| {
            item.data.and_then(|data| {
                Float::parse(data.assets_display.clone())
                    .ok()
                    .map(|assets_per_share| {
                        (
                            item.vault_address,
                            RatioValue {
                                asset_address: data.asset_address,
                                assets_per_share,
                                formatted_assets_per_share: data.assets_display,
                            },
                        )
                    })
            })
        })
        .collect();
    RatioRead {
        values,
        block_number: Some(block_number),
    }
}

pub(super) fn build_variant_map(
    markets: &[RaindexMarket],
    ratios: &HashMap<Address, RatioValue>,
) -> Result<VariantBuild, RaindexError> {
    let identity = Float::parse("1".to_string())?;
    Ok(markets.iter().fold(
        (HashMap::new(), Vec::new()),
        |(mut variants, mut errors), market| {
            variants.insert(
                market.base.address,
                Variant {
                    canonical_address: market.base.address,
                    price_multiplier: identity,
                },
            );
            let canonical_ratio = ratios.get(&market.base.address).filter(|ratio| {
                market
                    .base
                    .unwrapped_address
                    .is_none_or(|unwrapped| ratio.asset_address == unwrapped)
            });

            if let Some(unwrapped) = market.base.unwrapped_address {
                match canonical_ratio {
                    Some(ratio) => {
                        variants.insert(
                            unwrapped,
                            Variant {
                                canonical_address: market.base.address,
                                price_multiplier: ratio.assets_per_share,
                            },
                        );
                    }
                    None => errors.push((
                        market.id.clone(),
                        "canonical ERC4626 ratio is unavailable or has the wrong asset".into(),
                    )),
                }
            }

            if let Some(legacy) = market.base.legacy_address {
                match (canonical_ratio, ratios.get(&legacy)) {
                    (Some(canonical), Some(legacy_ratio)) => {
                        match canonical
                            .assets_per_share
                            .div(legacy_ratio.assets_per_share)
                        {
                            Ok(price_multiplier) => {
                                variants.insert(
                                    legacy,
                                    Variant {
                                        canonical_address: market.base.address,
                                        price_multiplier,
                                    },
                                );
                            }
                            Err(error) => errors.push((market.id.clone(), error.to_string())),
                        }
                    }
                    _ => errors.push((
                        market.id.clone(),
                        "legacy ERC4626 ratio is unavailable".into(),
                    )),
                }
            }
            (variants, errors)
        },
    ))
}

pub(super) fn build_canonical_variant_map(
    markets: &[RaindexMarket],
) -> Result<HashMap<Address, Variant>, RaindexError> {
    let identity = Float::parse("1".to_string())?;
    Ok(markets
        .iter()
        .map(|market| {
            (
                market.base.address,
                Variant {
                    canonical_address: market.base.address,
                    price_multiplier: identity,
                },
            )
        })
        .collect())
}

pub(super) async fn fetch_book_levels(
    client: &RaindexClient,
    markets: &[RaindexMarket],
    variant_map: &HashMap<Address, Variant>,
) -> Result<BookRead, RaindexError> {
    let first_market = markets
        .first()
        .ok_or_else(|| RaindexError::PreflightError("market list is empty".into()))?;
    let chain_id = first_market.chain_id;
    let quote_token = first_market.quote.address;
    let raindex_addresses = market_raindex_addresses(markets);
    let filters = market_order_filters(quote_token, variant_map, &raindex_addresses);
    let orders = fetch_orders(client, chain_id, &filters, quote_token, variant_map).await?;
    let token_decimals = orders
        .iter()
        .flat_map(|prepared| {
            prepared
                .order
                .input_vaults()
                .iter()
                .chain(prepared.order.output_vaults())
        })
        .map(|vault| (vault.token_address(), vault.token().decimals()))
        .collect::<HashMap<_, _>>();

    let quote_orders = orders
        .iter()
        .map(|prepared| prepared.order.clone())
        .collect::<Vec<_>>();
    let selected_pairs = orders
        .iter()
        .map(|prepared| prepared.pairs.clone())
        .collect::<Vec<_>>();
    let quote_results =
        get_order_quotes_batch_for_pairs(&quote_orders, &selected_pairs, None, None).await?;
    require_at_least_one_successful_quote(&quote_results)?;
    let errors = orders
        .iter()
        .zip(&quote_results)
        .flat_map(|(prepared, quotes)| {
            quote_errors_from_order(prepared, quotes, quote_token, variant_map)
        })
        .collect();
    let levels = orders
        .iter()
        .zip(quote_results)
        .flat_map(|(prepared, quotes)| {
            levels_from_order(prepared, &quotes, quote_token, variant_map, &token_decimals)
                .unwrap_or_else(|error| {
                    tracing::warn!(
                        order_hash = %prepared.order.raw_order_hash(),
                        error = %error,
                        "excluding invalid order from market snapshot"
                    );
                    Vec::new()
                })
        })
        .collect();
    Ok(BookRead { levels, errors })
}

fn require_at_least_one_successful_quote(
    quote_results: &[Vec<RaindexOrderQuote>],
) -> Result<(), RaindexError> {
    let attempted = quote_results.iter().map(Vec::len).sum::<usize>();
    if attempted > 0
        && !quote_results
            .iter()
            .flatten()
            .any(|quote| quote.success && quote.data.is_some())
    {
        return Err(RaindexError::PreflightError(format!(
            "all {attempted} order quote attempts failed"
        )));
    }
    Ok(())
}

fn market_order_filters(
    quote_token: Address,
    variant_map: &HashMap<Address, Variant>,
    raindex_addresses: &[Address],
) -> [GetOrdersFilters; 2] {
    let mut base_tokens = variant_map.keys().copied().collect::<Vec<_>>();
    base_tokens.sort_unstable();
    base_tokens.dedup();
    [
        GetOrdersFilters {
            active: Some(true),
            tokens: Some(GetOrdersTokenFilter {
                inputs: Some(vec![quote_token]),
                outputs: Some(base_tokens.clone()),
            }),
            raindex_addresses: Some(raindex_addresses.to_vec()),
            ..Default::default()
        },
        GetOrdersFilters {
            active: Some(true),
            tokens: Some(GetOrdersTokenFilter {
                inputs: Some(base_tokens),
                outputs: Some(vec![quote_token]),
            }),
            raindex_addresses: Some(raindex_addresses.to_vec()),
            ..Default::default()
        },
    ]
}

async fn fetch_orders(
    client: &RaindexClient,
    chain_id: u32,
    filters: &[GetOrdersFilters],
    quote_token: Address,
    variant_map: &HashMap<Address, Variant>,
) -> Result<Vec<PreparedOrder>, RaindexError> {
    let mut seen = HashSet::new();
    let mut orders = Vec::new();
    for filter in filters {
        let direction =
            fetch_orders_for_filter(client, chain_id, filter, quote_token, variant_map).await?;
        orders.extend(direction.into_iter().filter(|prepared| {
            seen.insert((
                prepared.order.raw_raindex(),
                prepared.order.raw_order_hash(),
            ))
        }));
    }
    Ok(orders)
}

async fn fetch_orders_for_filter(
    client: &RaindexClient,
    chain_id: u32,
    filter: &GetOrdersFilters,
    quote_token: Address,
    variant_map: &HashMap<Address, Variant>,
) -> Result<Vec<PreparedOrder>, RaindexError> {
    let mut orders = Vec::new();
    let mut seen = HashSet::new();
    for order in fetch_market_orders_paginated(client, vec![chain_id], filter.clone()).await? {
        if seen.insert((order.raw_raindex(), order.raw_order_hash())) {
            if let Some(order) = prepare_order(order, quote_token, variant_map) {
                orders.push(order);
            }
        }
    }

    Ok(orders)
}

fn quote_errors_from_order(
    prepared: &PreparedOrder,
    quotes: &[RaindexOrderQuote],
    quote_token: Address,
    variant_map: &HashMap<Address, Variant>,
) -> Vec<(Address, String)> {
    quotes
        .iter()
        .filter(|quote| !quote.success || quote.data.is_none())
        .filter_map(|quote| {
            let input = prepared
                .decoded
                .validInputs
                .get(quote.pair.input_index as usize)?;
            let output = prepared
                .decoded
                .validOutputs
                .get(quote.pair.output_index as usize)?;
            let source_token = if input.token == quote_token {
                output.token
            } else if output.token == quote_token {
                input.token
            } else {
                return None;
            };
            let market = variant_map.get(&source_token)?.canonical_address;
            let message = quote.error.clone().unwrap_or_else(|| {
                format!(
                    "order {:#x} returned no quote data for IO pair {}/{}",
                    prepared.order.raw_order_hash(),
                    quote.pair.input_index,
                    quote.pair.output_index
                )
            });
            Some((market, message))
        })
        .collect()
}

fn prepare_order(
    order: RaindexOrder,
    quote_token: Address,
    variant_map: &HashMap<Address, Variant>,
) -> Option<PreparedOrder> {
    let decoded = OrderV4::abi_decode(order.raw_order_bytes().as_ref()).ok()?;
    let pairs = market_pairs_from_order(&decoded, quote_token, variant_map);
    (!pairs.is_empty()).then_some(PreparedOrder {
        order,
        decoded,
        pairs,
    })
}

fn market_pairs_from_order(
    decoded: &OrderV4,
    quote_token: Address,
    variant_map: &HashMap<Address, Variant>,
) -> Vec<raindex_quote::Pair> {
    decoded
        .validInputs
        .iter()
        .enumerate()
        .flat_map(|(input_index, input)| {
            decoded
                .validOutputs
                .iter()
                .enumerate()
                .filter(move |(_, output)| {
                    (input.token == quote_token && variant_map.contains_key(&output.token))
                        || (output.token == quote_token && variant_map.contains_key(&input.token))
                })
                .map(move |(output_index, _)| raindex_quote::Pair {
                    pair_name: String::new(),
                    input_index: input_index as u32,
                    output_index: output_index as u32,
                })
        })
        .collect()
}

fn levels_from_order(
    prepared: &PreparedOrder,
    quotes: &[RaindexOrderQuote],
    quote_token: Address,
    variant_map: &HashMap<Address, Variant>,
    token_decimals: &HashMap<Address, u8>,
) -> Result<Vec<BookLevel>, RaindexError> {
    Ok(quotes
        .iter()
        .filter_map(|quote| {
            let data = quote.success.then_some(quote.data.as_ref()).flatten()?;
            let input = prepared
                .decoded
                .validInputs
                .get(quote.pair.input_index as usize)?;
            let output = prepared
                .decoded
                .validOutputs
                .get(quote.pair.output_index as usize)?;
            let (side, source_token, variant, price, base_quantity, target_quantity) =
                if input.token == quote_token {
                    let variant = variant_map.get(&output.token)?;
                    (
                        BookSide::Ask,
                        output.token,
                        *variant,
                        data.ratio.mul(variant.price_multiplier).ok()?,
                        data.max_output.div(variant.price_multiplier).ok()?,
                        data.max_input,
                    )
                } else if output.token == quote_token {
                    let variant = variant_map.get(&input.token)?;
                    (
                        BookSide::Bid,
                        input.token,
                        *variant,
                        data.inverse_ratio.mul(variant.price_multiplier).ok()?,
                        data.max_input.div(variant.price_multiplier).ok()?,
                        data.max_output,
                    )
                } else {
                    return None;
                };
            has_executable_atomic_amounts(
                data.max_output,
                data.ratio,
                *token_decimals.get(&input.token)?,
                *token_decimals.get(&output.token)?,
            )
            .ok()
            .filter(|executable| *executable)?;
            Some(BookLevel {
                canonical_address: variant.canonical_address,
                side,
                price,
                base_quantity,
                target_quantity,
                chain_id: prepared.order.chain_id(),
                raindex: prepared.order.raw_raindex(),
                order_hash: prepared.order.raw_order_hash(),
                source_token,
                block_number: quote.block_number,
            })
        })
        .collect())
}

pub(super) fn has_executable_atomic_amounts(
    max_output: Float,
    ratio: Float,
    input_decimals: u8,
    output_decimals: u8,
) -> Result<bool, rain_math_float::FloatError> {
    let max_input = max_output.mul(ratio)?;
    Ok(
        max_input.to_fixed_decimal_lossy(input_decimals)?.0 != U256::ZERO
            && max_output.to_fixed_decimal_lossy(output_decimals)?.0 != U256::ZERO,
    )
}

pub(super) fn apply_book_levels(
    snapshots: &mut BTreeMap<String, RaindexMarketSnapshot>,
    markets: &[RaindexMarket],
    levels: Vec<BookLevel>,
    depth: usize,
) {
    for market in markets {
        let mut bids = levels
            .iter()
            .filter(|level| {
                level.canonical_address == market.base.address && level.side == BookSide::Bid
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut asks = levels
            .iter()
            .filter(|level| {
                level.canonical_address == market.base.address && level.side == BookSide::Ask
            })
            .cloned()
            .collect::<Vec<_>>();
        bids.sort_by(|a, b| float_cmp_desc(a.price, b.price));
        asks.sort_by(|a, b| float_cmp_asc(a.price, b.price));
        let per_side = depth.div_ceil(2);
        bids.truncate(per_side);
        asks.truncate(per_side);

        let best_bid = bids.first().map(|level| level.price);
        let best_ask = asks.first().map(|level| level.price);
        let best_price_comparison = best_bid.zip(best_ask).map(|(bid, ask)| bid.gt(ask));
        let midpoint = match (best_bid, best_ask, &best_price_comparison) {
            (Some(bid), Some(ask), Some(Ok(false))) => Float::parse("2".into())
                .and_then(|two| bid.add(ask)?.div(two))
                .ok(),
            _ => None,
        };
        let block_number = bids
            .iter()
            .chain(&asks)
            .map(|level| level.block_number)
            .max();
        let book = RaindexMarketOrderbook {
            best_bid: best_bid.and_then(|value| value.format().ok()),
            best_ask: best_ask.and_then(|value| value.format().ok()),
            midpoint: midpoint.and_then(|value| value.format().ok()),
            bids: bids.into_iter().filter_map(format_level).collect(),
            asks: asks.into_iter().filter_map(format_level).collect(),
        };
        if let Some(snapshot) = snapshots.get_mut(&market.id) {
            snapshot.orderbook = book;
            snapshot.block_number = block_number;
            match best_price_comparison {
                Some(Ok(true)) => snapshot.errors.push(RaindexMarketDataError {
                    source: "orderbook".into(),
                    severity: RaindexMarketDataErrorSeverity::Error,
                    message: "best bid is greater than best ask".into(),
                }),
                Some(Err(error)) => snapshot.errors.push(RaindexMarketDataError {
                    source: "orderbook".into(),
                    severity: RaindexMarketDataErrorSeverity::Error,
                    message: format!("unable to compare best bid and best ask: {error}"),
                }),
                _ => {}
            }
        }
    }
}

pub(super) fn apply_book_errors(
    snapshots: &mut BTreeMap<String, RaindexMarketSnapshot>,
    markets: &[RaindexMarket],
    errors: Vec<(Address, String)>,
) {
    let mut seen = HashSet::new();
    errors
        .into_iter()
        .filter(|error| seen.insert(error.clone()))
        .for_each(|(canonical_address, message)| {
            markets
                .iter()
                .filter(|market| market.base.address == canonical_address)
                .for_each(|market| push_warning(snapshots, &market.id, "quote", message.clone()));
        });
}

fn format_level(level: BookLevel) -> Option<RaindexMarketOrderbookLevel> {
    Some(RaindexMarketOrderbookLevel {
        price: level.price.format().ok()?,
        base_quantity: level.base_quantity.format().ok()?,
        target_quantity: level.target_quantity.format().ok()?,
        chain_id: level.chain_id,
        raindex: level.raindex,
        order_hash: level.order_hash,
        source_token: level.source_token,
        block_number: level.block_number,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raindex_client::order_quotes::RaindexOrderQuoteValue;
    use alloy::primitives::address;

    const BASE: Address = address!("1111111111111111111111111111111111111111");
    const QUOTE: Address = address!("2222222222222222222222222222222222222222");
    const UNWRAPPED: Address = address!("3333333333333333333333333333333333333333");
    const LEGACY: Address = address!("4444444444444444444444444444444444444444");

    fn float(value: &str) -> Float {
        Float::parse(value.to_string()).unwrap()
    }

    fn quote(success: bool) -> RaindexOrderQuote {
        let ratio = float("1");
        RaindexOrderQuote {
            pair: raindex_quote::Pair {
                pair_name: String::new(),
                input_index: 0,
                output_index: 0,
            },
            block_number: 1,
            data: success.then_some(RaindexOrderQuoteValue {
                max_output: ratio,
                formatted_max_output: "1".into(),
                max_input: ratio,
                formatted_max_input: "1".into(),
                ratio,
                formatted_ratio: "1".into(),
                inverse_ratio: ratio,
                formatted_inverse_ratio: "1".into(),
                formatted_max_output_as_percent_of_vault: None,
            }),
            success,
            error: (!success).then(|| "quote failed".into()),
            signed_context: Vec::new(),
        }
    }

    fn token(address: Address, symbol: &str) -> RaindexMarketToken {
        RaindexMarketToken {
            chain_id: 8453,
            address,
            name: symbol.to_string(),
            symbol: symbol.to_string(),
            decimals: Some(18),
            logo_uri: None,
            extensions: None,
            unwrapped_address: None,
            legacy_address: None,
            receipt_address: None,
            variants: vec![address],
        }
    }

    fn market() -> RaindexMarket {
        RaindexMarket {
            id: "8453:base:quote".to_string(),
            ticker_id: "base_quote".to_string(),
            chain_id: 8453,
            base: token(BASE, "BASE"),
            quote: token(QUOTE, "QUOTE"),
            raindex_addresses: Vec::new(),
        }
    }

    fn snapshot(market: RaindexMarket) -> RaindexMarketSnapshot {
        RaindexMarketSnapshot {
            market,
            orderbook: RaindexMarketOrderbook::default(),
            stats: RaindexMarketStats::default(),
            recent_trades: Vec::new(),
            observed_at: 0,
            block_number: None,
            ratio_block_number: None,
            assets_per_share: None,
            errors: Vec::new(),
        }
    }

    fn level(side: BookSide, price: &str, block_number: u64) -> BookLevel {
        BookLevel {
            canonical_address: BASE,
            side,
            price: float(price),
            base_quantity: float("2"),
            target_quantity: float("20"),
            chain_id: 8453,
            raindex: Address::ZERO,
            order_hash: B256::from([block_number as u8; 32]),
            source_token: BASE,
            block_number,
        }
    }

    #[tokio::test]
    async fn ratio_read_skips_rpc_configuration_when_no_share_tokens_exist() {
        let read = read_ratios(&[], Vec::new()).await.unwrap();

        assert!(read.values.is_empty());
        assert_eq!(read.block_number, None);
    }

    #[tokio::test]
    async fn ratio_read_requires_an_rpc_for_share_tokens() {
        let error = read_ratios(&[], vec![BASE]).await.unwrap_err();

        assert_eq!(error, "no RPC URLs configured");
    }

    #[tokio::test]
    async fn ratio_read_reports_provider_failures_for_every_rpc() {
        let rpcs = [
            url::Url::parse("ftp://first.invalid").unwrap(),
            url::Url::parse("file:///second.invalid").unwrap(),
        ];

        let error = read_ratios(&rpcs, vec![BASE]).await.unwrap_err();

        assert!(error.starts_with("all RPC ratio reads failed:"));
        assert!(error.contains("ftp://first.invalid"));
        assert!(error.contains("file:///second.invalid"));
    }

    #[test]
    fn ratio_response_keeps_only_usable_items() {
        use rain_erc::erc4626::{
            Erc4626BatchItem, Erc4626BatchResponse, Erc4626ShareAssetConversion,
        };

        let response = Erc4626BatchResponse {
            block_number: 42,
            block_timestamp: 0,
            captured_at: 0,
            items: vec![
                Erc4626BatchItem {
                    vault_address: BASE,
                    success: true,
                    data: Some(Erc4626ShareAssetConversion {
                        share_token_address: BASE,
                        share_token_decimals: 18,
                        asset_address: UNWRAPPED,
                        asset_decimals: 18,
                        shares: U256::from(1),
                        shares_display: "1".into(),
                        assets: U256::from(2),
                        assets_display: "2".into(),
                    }),
                    error: None,
                },
                Erc4626BatchItem {
                    vault_address: LEGACY,
                    success: false,
                    data: None,
                    error: Some("unavailable".into()),
                },
            ],
        };

        let read = ratio_read_from_response(response);

        assert_eq!(read.block_number, Some(42));
        assert_eq!(read.values.len(), 1);
        assert!(read.values.contains_key(&BASE));
        assert!(!read.values.contains_key(&LEGACY));
    }

    #[test]
    fn variant_map_normalizes_unwrapped_and_legacy_tokens_to_the_canonical_market() {
        let mut configured_market = market();
        configured_market.base.unwrapped_address = Some(UNWRAPPED);
        configured_market.base.legacy_address = Some(LEGACY);
        let ratios = HashMap::from([
            (
                BASE,
                RatioValue {
                    asset_address: UNWRAPPED,
                    assets_per_share: float("2"),
                    formatted_assets_per_share: "2".to_string(),
                },
            ),
            (
                LEGACY,
                RatioValue {
                    asset_address: UNWRAPPED,
                    assets_per_share: float("4"),
                    formatted_assets_per_share: "4".to_string(),
                },
            ),
        ]);

        let (variants, errors) = build_variant_map(&[configured_market], &ratios).unwrap();

        assert!(errors.is_empty());
        assert_eq!(variants[&BASE].canonical_address, BASE);
        assert_eq!(variants[&UNWRAPPED].price_multiplier.format().unwrap(), "2");
        assert_eq!(variants[&LEGACY].price_multiplier.format().unwrap(), "0.5");
    }

    #[test]
    fn variant_map_keeps_canonical_market_and_reports_missing_ratios() {
        let mut configured_market = market();
        configured_market.base.unwrapped_address = Some(UNWRAPPED);
        configured_market.base.legacy_address = Some(LEGACY);

        let (variants, errors) = build_variant_map(&[configured_market], &HashMap::new()).unwrap();

        assert_eq!(variants.len(), 1);
        assert_eq!(variants[&BASE].price_multiplier.format().unwrap(), "1");
        assert_eq!(errors.len(), 2);
        assert!(errors[0].1.contains("canonical ERC4626 ratio"));
        assert!(errors[1].1.contains("legacy ERC4626 ratio"));
    }

    #[test]
    fn canonical_variant_map_excludes_wrapper_variants() {
        let mut configured_market = market();
        configured_market.base.unwrapped_address = Some(UNWRAPPED);
        configured_market.base.legacy_address = Some(LEGACY);

        let variants = build_canonical_variant_map(&[configured_market]).unwrap();

        assert_eq!(variants.len(), 1);
        assert_eq!(variants[&BASE].canonical_address, BASE);
        assert_eq!(variants[&BASE].price_multiplier.format().unwrap(), "1");
        assert!(!variants.contains_key(&UNWRAPPED));
        assert!(!variants.contains_key(&LEGACY));
    }

    #[test]
    fn order_filters_query_only_quote_to_base_directions() {
        let variants = HashMap::from([
            (
                BASE,
                Variant {
                    canonical_address: BASE,
                    price_multiplier: float("1"),
                },
            ),
            (
                LEGACY,
                Variant {
                    canonical_address: BASE,
                    price_multiplier: float("0.5"),
                },
            ),
        ]);

        let raindexes = [BASE];
        let [asks, bids] = market_order_filters(QUOTE, &variants, &raindexes);
        let ask_tokens = asks.tokens.unwrap();
        let bid_tokens = bids.tokens.unwrap();

        assert_eq!(ask_tokens.inputs, Some(vec![QUOTE]));
        assert_eq!(ask_tokens.outputs, Some(vec![BASE, LEGACY]));
        assert_eq!(bid_tokens.inputs, Some(vec![BASE, LEGACY]));
        assert_eq!(bid_tokens.outputs, Some(vec![QUOTE]));
        assert_eq!(asks.has_positive_output_vault_balance, None);
        assert_eq!(bids.has_positive_output_vault_balance, None);
        assert_eq!(asks.raindex_addresses, Some(raindexes.to_vec()));
        assert_eq!(bids.raindex_addresses, Some(raindexes.to_vec()));
    }

    #[test]
    fn book_levels_are_sorted_truncated_and_summarized() {
        let market = market();
        let mut snapshots = BTreeMap::from([(market.id.clone(), snapshot(market.clone()))]);
        let levels = vec![
            level(BookSide::Bid, "8", 1),
            level(BookSide::Bid, "10", 2),
            level(BookSide::Bid, "9", 3),
            level(BookSide::Ask, "13", 4),
            level(BookSide::Ask, "11", 5),
            level(BookSide::Ask, "12", 6),
        ];

        apply_book_levels(&mut snapshots, std::slice::from_ref(&market), levels, 4);

        let snapshot = &snapshots[&market.id];
        assert_eq!(snapshot.orderbook.best_bid.as_deref(), Some("10"));
        assert_eq!(snapshot.orderbook.best_ask.as_deref(), Some("11"));
        assert_eq!(snapshot.orderbook.midpoint.as_deref(), Some("10.5"));
        assert_eq!(
            snapshot
                .orderbook
                .bids
                .iter()
                .map(|level| level.price.as_str())
                .collect::<Vec<_>>(),
            vec!["10", "9"]
        );
        assert_eq!(
            snapshot
                .orderbook
                .asks
                .iter()
                .map(|level| level.price.as_str())
                .collect::<Vec<_>>(),
            vec!["11", "12"]
        );
        assert_eq!(snapshot.block_number, Some(6));
        assert!(snapshot.errors.is_empty());
    }

    #[test]
    fn one_level_depth_keeps_the_best_level_on_each_side() {
        let market = market();
        let mut snapshots = BTreeMap::from([(market.id.clone(), snapshot(market.clone()))]);

        apply_book_levels(
            &mut snapshots,
            std::slice::from_ref(&market),
            vec![
                level(BookSide::Bid, "10", 1),
                level(BookSide::Bid, "9", 2),
                level(BookSide::Ask, "11", 3),
                level(BookSide::Ask, "12", 4),
            ],
            1,
        );

        let snapshot = &snapshots[&market.id];
        assert_eq!(snapshot.orderbook.bids.len(), 1);
        assert_eq!(snapshot.orderbook.asks.len(), 1);
        assert_eq!(snapshot.orderbook.best_bid.as_deref(), Some("10"));
        assert_eq!(snapshot.orderbook.best_ask.as_deref(), Some("11"));
    }

    #[test]
    fn crossed_book_is_exposed_with_an_explicit_error_and_no_midpoint() {
        let market = market();
        let mut snapshots = BTreeMap::from([(market.id.clone(), snapshot(market.clone()))]);

        apply_book_levels(
            &mut snapshots,
            std::slice::from_ref(&market),
            vec![level(BookSide::Bid, "12", 1), level(BookSide::Ask, "11", 2)],
            100,
        );

        let snapshot = &snapshots[&market.id];
        assert_eq!(snapshot.orderbook.midpoint, None);
        assert_eq!(snapshot.errors.len(), 1);
        assert_eq!(snapshot.errors[0].source, "orderbook");
        assert!(snapshot.errors[0].message.contains("best bid"));
    }

    #[test]
    fn quote_errors_are_deduplicated_and_scoped_to_the_matching_market() {
        let market = market();
        let mut snapshots = BTreeMap::from([(market.id.clone(), snapshot(market.clone()))]);

        apply_book_errors(
            &mut snapshots,
            std::slice::from_ref(&market),
            vec![
                (BASE, "quote failed".to_string()),
                (BASE, "quote failed".to_string()),
                (QUOTE, "unrelated".to_string()),
            ],
        );

        assert_eq!(snapshots[&market.id].errors.len(), 1);
        assert_eq!(snapshots[&market.id].errors[0].message, "quote failed");
        assert_eq!(
            snapshots[&market.id].errors[0].severity,
            RaindexMarketDataErrorSeverity::Warning
        );
    }

    #[test]
    fn atomic_liquidity_requires_nonzero_input_and_output_amounts() {
        assert!(has_executable_atomic_amounts(float("1"), float("2"), 6, 18).unwrap());
        assert!(!has_executable_atomic_amounts(float("0.0000001"), float("1"), 6, 18).unwrap());
        assert!(has_executable_atomic_amounts(float("0.0000001"), float("1"), 18, 18).unwrap());
    }

    #[test]
    fn partial_quote_batches_keep_successful_results() {
        assert!(require_at_least_one_successful_quote(&[vec![quote(false), quote(true),]]).is_ok());
    }

    #[test]
    fn fully_failed_quote_batches_remain_fatal() {
        let error = require_at_least_one_successful_quote(&[vec![quote(false), quote(false)]])
            .expect_err("all quotes failed");

        assert!(error
            .to_string()
            .contains("all 2 order quote attempts failed"));
    }
}
