use super::*;
use crate::types::VaultBalanceChangeKind;

pub(super) struct NormalizedTradeRead {
    pub window: Vec<NormalizedTrade>,
    pub latest: Vec<NormalizedTrade>,
}

pub(super) async fn fetch_normalized_trades(
    client: &RaindexClient,
    markets: &[RaindexMarket],
    variant_map: &HashMap<Address, Variant>,
    observed_at: u64,
) -> Result<NormalizedTradeRead, RaindexError> {
    let first_market = markets
        .first()
        .ok_or_else(|| RaindexError::PreflightError("market list is empty".into()))?;
    let mut tokens = variant_map.keys().copied().collect::<Vec<_>>();
    tokens.sort_unstable();
    tokens.dedup();
    let raindex_addresses = market_raindex_addresses(markets);
    let filters = market_trade_filters(
        first_market.quote.address,
        &tokens,
        &raindex_addresses,
        observed_at,
    );
    let trade_reads = join_all(
        filters
            .iter()
            .map(|filter| fetch_trades(client, first_market.chain_id, filter)),
    )
    .await;
    let mut trades = Vec::new();
    for read in trade_reads {
        trades.extend(read?);
    }
    let window = normalize_trades(&trades, first_market.quote.address, variant_map);
    let traded_canonical_addresses = window
        .iter()
        .map(|trade| trade.canonical_address)
        .collect::<HashSet<_>>();
    let latest_source_tokens = variant_map
        .iter()
        .filter_map(|(source, variant)| {
            (!traded_canonical_addresses.contains(&variant.canonical_address)).then_some(*source)
        })
        .collect();
    let latest = normalize_trades(
        &client
            .get_latest_market_trades(
                first_market.chain_id,
                first_market.quote.address,
                latest_source_tokens,
                raindex_addresses,
            )
            .await?,
        first_market.quote.address,
        variant_map,
    );

    Ok(NormalizedTradeRead { window, latest })
}

fn normalize_trades(
    trades: &[RaindexTrade],
    quote_token: Address,
    variant_map: &HashMap<Address, Variant>,
) -> Vec<NormalizedTrade> {
    collapse_clear_events(
        trades
            .iter()
            .filter_map(|trade| match normalize_trade(trade, quote_token, variant_map) {
                Ok(trade) => trade,
                Err(error) => {
                    tracing::warn!(error = %error, "excluding invalid trade from market snapshot");
                    None
                }
            })
            .collect(),
    )
}

fn market_trade_filters(
    quote_token: Address,
    base_tokens: &[Address],
    raindex_addresses: &[Address],
    observed_at: u64,
) -> [GetTradesFilters; 2] {
    let direction = |inputs, outputs| GetTradesFilters {
        tokens: Some(GetTradesTokenFilter {
            inputs: Some(inputs),
            outputs: Some(outputs),
        }),
        raindex_addresses: Some(raindex_addresses.to_vec()),
        time_filter: Some(TimeFilter {
            start: Some(observed_at.saturating_sub(86_400)),
            end: Some(observed_at),
        }),
        ..Default::default()
    };
    [
        direction(vec![quote_token], base_tokens.to_vec()),
        direction(base_tokens.to_vec(), vec![quote_token]),
    ]
}

async fn fetch_trades(
    client: &RaindexClient,
    chain_id: u32,
    filters: &GetTradesFilters,
) -> Result<Vec<RaindexTrade>, RaindexError> {
    client
        .get_trades_unpaginated(chain_id, filters.clone())
        .await
}

pub(super) fn collapse_clear_events(trades: Vec<NormalizedTrade>) -> Vec<NormalizedTrade> {
    trades
        .into_iter()
        .fold(
            (
                Vec::<NormalizedTrade>::new(),
                HashMap::<(Address, u32, Address, String), usize>::new(),
            ),
            |(mut collapsed, mut clear_indices), trade| {
                if trade.trade.trade_event_kind.eq_ignore_ascii_case("clear") {
                    let key = (
                        trade.canonical_address,
                        trade.trade.chain_id,
                        trade.trade.raindex,
                        trade.trade.trade_event_id.clone(),
                    );
                    match clear_indices.get(&key).copied() {
                        Some(index)
                            if trade.trade.side == RaindexMarketTradeSide::Buy
                                && collapsed[index].trade.side != RaindexMarketTradeSide::Buy =>
                        {
                            collapsed[index] = trade;
                        }
                        Some(_) => {}
                        None => {
                            clear_indices.insert(key, collapsed.len());
                            collapsed.push(trade);
                        }
                    }
                } else {
                    collapsed.push(trade);
                }
                (collapsed, clear_indices)
            },
        )
        .0
}

fn normalize_trade(
    trade: &RaindexTrade,
    quote_token: Address,
    variant_map: &HashMap<Address, Variant>,
) -> Result<Option<NormalizedTrade>, RaindexError> {
    let input_address = trade.raw_input_change().raw_token_address();
    let output_address = trade.raw_output_change().raw_token_address();
    let input_amount = trade.raw_input_change().raw_amount();
    let output_amount = Float::zero()?.sub(trade.raw_output_change().raw_amount())?;
    let (variant, side, base_volume, target_volume, source_token) = if input_address == quote_token
    {
        let Some(variant) = variant_map.get(&output_address).copied() else {
            return Ok(None);
        };
        (
            variant,
            RaindexMarketTradeSide::Buy,
            output_amount.div(variant.price_multiplier)?,
            input_amount,
            output_address,
        )
    } else if output_address == quote_token {
        let Some(variant) = variant_map.get(&input_address).copied() else {
            return Ok(None);
        };
        (
            variant,
            RaindexMarketTradeSide::Sell,
            input_amount.div(variant.price_multiplier)?,
            output_amount,
            input_address,
        )
    } else {
        return Ok(None);
    };
    if base_volume.is_zero()? || target_volume.is_zero()? {
        return Ok(None);
    }
    let price = target_volume.div(base_volume)?;
    let timestamp = u64::try_from(trade.raw_timestamp())
        .map_err(|_| RaindexError::PreflightError("trade timestamp does not fit u64".into()))?;
    let block_number = u64::try_from(trade.raw_block_number())
        .map_err(|_| RaindexError::PreflightError("trade block number does not fit u64".into()))?;
    Ok(Some(NormalizedTrade {
        canonical_address: variant.canonical_address,
        source_log_index: trade.raw_source_log_index(),
        trade: RaindexMarketTrade {
            trade_id: trade.raw_id().to_string(),
            price: price.format()?,
            base_volume: base_volume.format()?,
            target_volume: target_volume.format()?,
            timestamp,
            block_number,
            trade_event_id: trade.raw_trade_event_id().to_string(),
            trade_event_kind: normalized_trade_event_kind(trade.raw_trade_event_kind()).to_string(),
            side,
            chain_id: trade.chain_id(),
            raindex: trade.raw_raindex(),
            order_hash: trade.raw_order_hash(),
            source_token,
        },
        price,
        base_volume,
        target_volume,
    }))
}

fn normalized_trade_event_kind(kind: &str) -> &'static str {
    let subgraph_kind = VaultBalanceChangeKind::from_subgraph_typename(kind);
    let normalized = if subgraph_kind == VaultBalanceChangeKind::Unknown {
        VaultBalanceChangeKind::from_local_db_trade_kind(kind)
    } else {
        subgraph_kind
    };
    match normalized {
        VaultBalanceChangeKind::Clear => "clear",
        VaultBalanceChangeKind::TakeOrder => "takeOrder",
        _ => "unknown",
    }
}

pub(super) fn apply_trades(
    snapshots: &mut BTreeMap<String, RaindexMarketSnapshot>,
    markets: &[RaindexMarket],
    trades: Vec<NormalizedTrade>,
    latest_trades: Vec<NormalizedTrade>,
    recent_limit: usize,
) {
    for market in markets {
        let mut market_trades = trades
            .iter()
            .filter(|trade| trade.canonical_address == market.base.address)
            .cloned()
            .collect::<Vec<_>>();
        market_trades.sort_by(compare_trades_descending);
        let latest_order_is_ambiguous = market_trades
            .first()
            .zip(market_trades.get(1))
            .is_some_and(|(first, second)| {
                first.trade.block_number == second.trade.block_number
                    && (first.source_log_index.is_none() || second.source_log_index.is_none())
            });
        let latest_trade = market_trades.first().or_else(|| {
            latest_trades
                .iter()
                .filter(|trade| trade.canonical_address == market.base.address)
                .max_by(|a, b| compare_trades_descending(b, a))
        });
        let stats = match stats_from_trades(&market_trades, latest_trade) {
            Ok(stats) => stats,
            Err(error) => {
                push_error(snapshots, &market.id, "trades", error.to_string());
                RaindexMarketStats::default()
            }
        };
        let recent_trades = market_trades
            .into_iter()
            .take(recent_limit)
            .map(|trade| trade.trade)
            .collect();
        if let Some(snapshot) = snapshots.get_mut(&market.id) {
            snapshot.stats = stats;
            snapshot.recent_trades = recent_trades;
            if latest_order_is_ambiguous {
                snapshot.errors.push(RaindexMarketDataError {
                    source: "trades".into(),
                    severity: RaindexMarketDataErrorSeverity::Warning,
                    message: "multiple executions share the latest indexed block; the source does not expose transaction/log ordering"
                        .into(),
                });
            }
        }
    }
}

fn compare_trades_descending(a: &NormalizedTrade, b: &NormalizedTrade) -> std::cmp::Ordering {
    b.trade
        .block_number
        .cmp(&a.trade.block_number)
        .then_with(|| match (b.source_log_index, a.source_log_index) {
            (Some(b_index), Some(a_index)) => b_index.cmp(&a_index),
            _ => b
                .trade
                .trade_event_id
                .as_str()
                .cmp(a.trade.trade_event_id.as_str()),
        })
}

pub(super) fn stats_from_trades(
    trades: &[NormalizedTrade],
    latest_trade: Option<&NormalizedTrade>,
) -> Result<RaindexMarketStats, RaindexError> {
    let Some(first) = trades.first() else {
        return Ok(RaindexMarketStats {
            last_price: latest_trade.map(|trade| trade.price.format()).transpose()?,
            ..Default::default()
        });
    };
    let (high, low, base_volume, target_volume) = trades.iter().try_fold(
        (first.price, first.price, Float::zero()?, Float::zero()?),
        |(high, low, base, target), trade| {
            Ok::<_, RaindexError>((
                if trade.price.gt(high)? {
                    trade.price
                } else {
                    high
                },
                if trade.price.lt(low)? {
                    trade.price
                } else {
                    low
                },
                base.add(trade.base_volume)?,
                target.add(trade.target_volume)?,
            ))
        },
    )?;
    Ok(RaindexMarketStats {
        last_price: Some(latest_trade.unwrap_or(first).price.format()?),
        high_24h: Some(high.format()?),
        low_24h: Some(low.format()?),
        base_volume_24h: base_volume.format()?,
        target_volume_24h: target_volume.format()?,
        trade_count_24h: trades.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    const BASE: Address = address!("1111111111111111111111111111111111111111");
    const QUOTE: Address = address!("2222222222222222222222222222222222222222");
    const RAINDEX: Address = address!("3333333333333333333333333333333333333333");

    fn normalized_trade(price: &str, base: &str, target: &str, block: u64) -> NormalizedTrade {
        let canonical_address = address!("1111111111111111111111111111111111111111");
        NormalizedTrade {
            canonical_address,
            source_log_index: None,
            trade: RaindexMarketTrade {
                trade_id: format!("0x{block:064x}"),
                price: price.to_string(),
                base_volume: base.to_string(),
                target_volume: target.to_string(),
                timestamp: block,
                block_number: block,
                trade_event_id: format!("0x{block:064x}"),
                trade_event_kind: "takeOrder".to_string(),
                side: RaindexMarketTradeSide::Buy,
                chain_id: 8453,
                raindex: Address::ZERO,
                order_hash: B256::ZERO,
                source_token: canonical_address,
            },
            price: Float::parse(price.to_string()).unwrap(),
            base_volume: Float::parse(base.to_string()).unwrap(),
            target_volume: Float::parse(target.to_string()).unwrap(),
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
            raindex_addresses: vec![RAINDEX],
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

    #[test]
    fn stats_use_latest_trade_and_aggregate_exact_values() {
        let trades = vec![
            normalized_trade("12", "2", "24", 3),
            normalized_trade("10", "1.5", "15", 2),
            normalized_trade("15", "1", "15", 1),
        ];

        let stats = stats_from_trades(&trades, trades.first()).unwrap();

        assert_eq!(stats.last_price.as_deref(), Some("12"));
        assert_eq!(stats.high_24h.as_deref(), Some("15"));
        assert_eq!(stats.low_24h.as_deref(), Some("10"));
        assert_eq!(stats.base_volume_24h, "4.5");
        assert_eq!(stats.target_volume_24h, "54");
        assert_eq!(stats.trade_count_24h, 3);
    }

    #[test]
    fn empty_stats_keep_market_visible_with_zero_volumes() {
        assert_eq!(
            stats_from_trades(&[], None).unwrap(),
            RaindexMarketStats::default()
        );
    }

    #[test]
    fn historical_last_price_does_not_change_24_hour_aggregates() {
        let historical = normalized_trade("9", "10", "90", 1);

        let stats = stats_from_trades(&[], Some(&historical)).unwrap();

        assert_eq!(stats.last_price.as_deref(), Some("9"));
        assert_eq!(stats.high_24h, None);
        assert_eq!(stats.low_24h, None);
        assert_eq!(stats.base_volume_24h, "0");
        assert_eq!(stats.target_volume_24h, "0");
        assert_eq!(stats.trade_count_24h, 0);
    }

    #[test]
    fn trade_event_kinds_preserve_unknown_values() {
        assert_eq!(normalized_trade_event_kind("Clear"), "clear");
        assert_eq!(normalized_trade_event_kind("clear"), "clear");
        assert_eq!(normalized_trade_event_kind("TakeOrder"), "takeOrder");
        assert_eq!(normalized_trade_event_kind("take"), "takeOrder");
        assert_eq!(normalized_trade_event_kind("unexpected"), "unknown");
    }

    #[test]
    fn same_block_trade_ordering_is_a_nonfatal_warning() {
        let market = market();
        let mut second = normalized_trade("11", "1", "11", 3);
        second.trade.trade_event_id = "0x02".to_string();
        let mut snapshots = BTreeMap::from([(market.id.clone(), snapshot(market.clone()))]);

        apply_trades(
            &mut snapshots,
            std::slice::from_ref(&market),
            vec![normalized_trade("12", "2", "24", 3), second],
            vec![],
            20,
        );

        let snapshot = &snapshots[&market.id];
        assert_eq!(snapshot.stats.trade_count_24h, 2);
        assert_eq!(snapshot.errors.len(), 1);
        assert_eq!(
            snapshot.errors[0].severity,
            RaindexMarketDataErrorSeverity::Warning
        );
    }

    #[test]
    fn same_block_local_trades_use_log_index_ordering() {
        let market = market();
        let mut earlier = normalized_trade("11", "1", "11", 3);
        earlier.source_log_index = Some(4);
        earlier.trade.trade_event_id = "0xffff".to_string();
        let mut later = normalized_trade("12", "1", "12", 3);
        later.source_log_index = Some(5);
        later.trade.trade_event_id = "0x0000".to_string();
        let mut snapshots = BTreeMap::from([(market.id.clone(), snapshot(market.clone()))]);

        apply_trades(
            &mut snapshots,
            std::slice::from_ref(&market),
            vec![earlier, later],
            vec![],
            20,
        );

        let snapshot = &snapshots[&market.id];
        assert_eq!(snapshot.stats.last_price.as_deref(), Some("12"));
        assert_eq!(snapshot.recent_trades[0].price, "12");
        assert!(snapshot.errors.is_empty());
    }

    #[test]
    fn clear_sides_collapse_to_one_market_execution_and_prefer_buy() {
        let mut buy = normalized_trade("12", "2", "24", 3);
        buy.trade.trade_event_kind = "clear".to_string();
        let mut sell = buy.clone();
        sell.trade.side = RaindexMarketTradeSide::Sell;
        sell.trade.trade_id = "0x02".to_string();

        let collapsed = collapse_clear_events(vec![sell, buy]);

        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].trade.side, RaindexMarketTradeSide::Buy);
        assert_eq!(
            stats_from_trades(&collapsed, collapsed.first())
                .unwrap()
                .trade_count_24h,
            1
        );
    }

    #[test]
    fn distinct_clear_event_ids_remain_distinct_executions() {
        let mut first = normalized_trade("12", "2", "24", 3);
        first.trade.trade_event_kind = "clear".to_string();
        let mut second = first.clone();
        second.trade.trade_event_id = "0x02".to_string();

        let collapsed = collapse_clear_events(vec![first, second]);

        assert_eq!(collapsed.len(), 2);
    }

    #[test]
    fn trade_filters_pin_both_market_directions() {
        let filters = market_trade_filters(QUOTE, &[BASE], &[RAINDEX], 100_000);

        let first = filters[0].tokens.as_ref().unwrap();
        assert_eq!(first.inputs, Some(vec![QUOTE]));
        assert_eq!(first.outputs, Some(vec![BASE]));
        let second = filters[1].tokens.as_ref().unwrap();
        assert_eq!(second.inputs, Some(vec![BASE]));
        assert_eq!(second.outputs, Some(vec![QUOTE]));
        for filter in filters {
            assert_eq!(filter.raindex_addresses, Some(vec![RAINDEX]));
            assert_eq!(filter.time_filter.unwrap().start, Some(13_600));
        }
    }
}
