use super::*;
use crate::local_db::query::fetch_latest_trades_per_token::FetchLatestTradesPerTokenArgs;
use crate::local_db::query::fetch_trades::{
    extract_trades_count, FetchTradesArgs, FetchTradesTokensFilter,
};
use crate::raindex_client::local_db::query::fetch_latest_trades_per_token::fetch_latest_trades_per_token;
use crate::raindex_client::local_db::query::fetch_trades::{fetch_trades, fetch_trades_count};
use crate::utils::timing::Timing;
use futures::future::join_all;
use raindex_subgraph_client::types::common::{
    SgBigInt, SgBytes, SgTrade, SgTradeEventFilter, SgTradeOrderFilter,
    SgTradeVaultBalanceChangeTokenFilter, SgTradeVaultTokenFilter, SgTradesListQueryFilters,
    SgTradesTokensFilterArgs,
};
use raindex_subgraph_client::{MultiRaindexSubgraphClient, SgPaginationArgs};
use tracing::{debug, error, info, info_span, Instrument};
use tsify::Tsify;
use wasm_bindgen_utils::impl_wasm_traits;

const DEFAULT_PAGE_SIZE: u16 = 100;

#[derive(Serialize, Deserialize, Debug, Clone, Tsify, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetTradesTokenFilter {
    #[cfg_attr(target_family = "wasm", tsify(optional, type = "Address[]"))]
    pub inputs: Option<Vec<Address>>,
    #[cfg_attr(target_family = "wasm", tsify(optional, type = "Address[]"))]
    pub outputs: Option<Vec<Address>>,
}
impl_wasm_traits!(GetTradesTokenFilter);

#[derive(Serialize, Deserialize, Debug, Clone, Tsify, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GetTradesFilters {
    #[tsify(optional, type = "Address[]")]
    pub owners: Vec<Address>,
    #[tsify(optional, type = "Address[]")]
    pub takers: Vec<Address>,
    #[tsify(optional, type = "Hex")]
    pub order_hash: Option<B256>,
    #[tsify(optional)]
    pub tokens: Option<GetTradesTokenFilter>,
    #[tsify(optional, type = "Address[]")]
    pub raindex_addresses: Option<Vec<Address>>,
    #[tsify(optional)]
    pub time_filter: Option<TimeFilter>,
}
impl_wasm_traits!(GetTradesFilters);

impl From<GetTradesTokenFilter> for Option<SgTradesTokensFilterArgs> {
    fn from(filter: GetTradesTokenFilter) -> Self {
        let inputs: Vec<String> = filter
            .inputs
            .unwrap_or_default()
            .into_iter()
            .map(|token| token.to_string().to_lowercase())
            .collect();
        let outputs: Vec<String> = filter
            .outputs
            .unwrap_or_default()
            .into_iter()
            .map(|token| token.to_string().to_lowercase())
            .collect();

        if inputs.is_empty() && outputs.is_empty() {
            None
        } else {
            Some(SgTradesTokensFilterArgs { inputs, outputs })
        }
    }
}

impl From<GetTradesFilters> for SgTradesListQueryFilters {
    fn from(filters: GetTradesFilters) -> Self {
        let time_filter = filters.time_filter.unwrap_or_default();
        let mut sg_filters = SgTradesListQueryFilters {
            timestamp_gte: Some(
                time_filter
                    .start
                    .map_or(SgBigInt("0".to_string()), |v| SgBigInt(v.to_string())),
            ),
            timestamp_lte: Some(
                time_filter
                    .end
                    .map_or(SgBigInt(u64::MAX.to_string()), |v| SgBigInt(v.to_string())),
            ),
            raindex_in: filters
                .raindex_addresses
                .unwrap_or_default()
                .into_iter()
                .map(|address| address.to_string().to_lowercase())
                .collect(),
            ..Default::default()
        };

        if !filters.owners.is_empty() || filters.order_hash.is_some() {
            sg_filters.order_ = Some(SgTradeOrderFilter {
                owner_in: filters
                    .owners
                    .into_iter()
                    .map(|owner| SgBytes(owner.to_string()))
                    .collect(),
                order_hash: filters.order_hash.map(|hash| SgBytes(hash.to_string())),
                order_hash_in: Vec::new(),
            });
        }

        if !filters.takers.is_empty() {
            sg_filters.trade_event_ = Some(SgTradeEventFilter {
                sender_in: filters
                    .takers
                    .into_iter()
                    .map(|taker| SgBytes(taker.to_string()))
                    .collect(),
            });
        }

        if let Some(tokens) = filters.tokens {
            let tokens: Option<SgTradesTokensFilterArgs> = tokens.into();
            let tokens = normalize_trade_tokens(tokens.unwrap_or_default());
            let vault_filter = |tokens: Vec<String>| SgTradeVaultBalanceChangeTokenFilter {
                vault_: Some(SgTradeVaultTokenFilter { token_in: tokens }),
            };
            if !tokens.inputs.is_empty()
                && !tokens.outputs.is_empty()
                && tokens.inputs == tokens.outputs
            {
                sg_filters.or = Some(vec![
                    SgTradesListQueryFilters {
                        input_vault_balance_change_: Some(vault_filter(tokens.inputs)),
                        ..Default::default()
                    },
                    SgTradesListQueryFilters {
                        output_vault_balance_change_: Some(vault_filter(tokens.outputs)),
                        ..Default::default()
                    },
                ]);
            } else {
                if !tokens.inputs.is_empty() {
                    sg_filters.input_vault_balance_change_ = Some(vault_filter(tokens.inputs));
                }
                if !tokens.outputs.is_empty() {
                    sg_filters.output_vault_balance_change_ = Some(vault_filter(tokens.outputs));
                }
            }
        }

        sg_filters
    }
}

fn local_trades_pagination(
    has_subgraph_sources: bool,
    page_number: u16,
    page_size: u16,
) -> (PaginationParams, bool) {
    if has_subgraph_sources {
        (PaginationParams::default(), false)
    } else {
        (
            PaginationParams {
                page: Some(page_number),
                page_size: Some(page_size),
            },
            true,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TradeFilterTraceSummary {
    owners_count: usize,
    takers_count: usize,
    input_tokens_count: usize,
    output_tokens_count: usize,
    raindexes_count: usize,
    has_order_hash_filter: bool,
    has_time_filter: bool,
}

fn summarize_trade_filters(filters: &GetTradesFilters) -> TradeFilterTraceSummary {
    TradeFilterTraceSummary {
        owners_count: filters.owners.len(),
        takers_count: filters.takers.len(),
        input_tokens_count: filters
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.inputs.as_ref())
            .map_or(0, Vec::len),
        output_tokens_count: filters
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.outputs.as_ref())
            .map_or(0, Vec::len),
        raindexes_count: filters.raindex_addresses.as_ref().map_or(0, Vec::len),
        has_order_hash_filter: filters.order_hash.is_some(),
        has_time_filter: filters.time_filter.is_some(),
    }
}

fn latest_market_trade_filter(
    quote_token: Address,
    base_token: Address,
    raindex_addresses: &[Address],
) -> SgTradesListQueryFilters {
    let vault_filter = |token: Address| SgTradeVaultBalanceChangeTokenFilter {
        vault_: Some(SgTradeVaultTokenFilter {
            token_in: vec![token.to_string().to_ascii_lowercase()],
        }),
    };
    let direction = |input: Address, output: Address| SgTradesListQueryFilters {
        input_vault_balance_change_: Some(vault_filter(input)),
        output_vault_balance_change_: Some(vault_filter(output)),
        ..Default::default()
    };
    SgTradesListQueryFilters {
        raindex_in: raindex_addresses
            .iter()
            .map(|address| address.to_string().to_ascii_lowercase())
            .collect(),
        or: Some(vec![
            direction(quote_token, base_token),
            direction(base_token, quote_token),
        ]),
        ..Default::default()
    }
}

pub(super) fn normalize_trade_tokens(
    mut tokens: SgTradesTokensFilterArgs,
) -> SgTradesTokensFilterArgs {
    tokens.inputs.sort_unstable();
    tokens.inputs.dedup();
    tokens.outputs.sort_unstable();
    tokens.outputs.dedup();
    tokens
}

pub(super) fn sg_trade_matches_token_filter(
    trade: &SgTrade,
    tokens: &SgTradesTokensFilterArgs,
) -> bool {
    let has_inputs = !tokens.inputs.is_empty();
    let has_outputs = !tokens.outputs.is_empty();
    let input_token = trade
        .input_vault_balance_change
        .vault
        .token
        .address
        .0
        .to_lowercase();
    let output_token = trade
        .output_vault_balance_change
        .vault
        .token
        .address
        .0
        .to_lowercase();

    if has_inputs && has_outputs && tokens.inputs == tokens.outputs {
        tokens.inputs.contains(&input_token) || tokens.outputs.contains(&output_token)
    } else {
        (!has_inputs || tokens.inputs.contains(&input_token))
            && (!has_outputs || tokens.outputs.contains(&output_token))
    }
}

#[wasm_export]
impl RaindexClient {
    #[wasm_export(
        js_name = "getTrades",
        return_description = "Trades list result with total count and per-pair summary",
        unchecked_return_type = "RaindexTradesListResult",
        preserve_js_class
    )]
    pub async fn get_trades(
        &self,
        #[wasm_export(
            js_name = "chainIds",
            param_description = "Specific blockchain networks to query (optional, queries all networks if not specified)"
        )]
        chain_ids: Option<ChainIds>,
        #[wasm_export(
            param_description = "Filtering criteria including owners, order hash, token addresses, raindexes, and time range"
        )]
        filters: Option<GetTradesFilters>,
        #[wasm_export(param_description = "Page number for pagination (optional, defaults to 1)")]
        page: Option<u16>,
        #[wasm_export(
            js_name = "pageSize",
            param_description = "Number of items per page (optional, defaults to 100)"
        )]
        page_size: Option<u16>,
    ) -> Result<RaindexTradesListResult, RaindexError> {
        let route_started = Timing::now();
        let filters = filters.unwrap_or_default();
        let page_number = page.unwrap_or(1).max(1);
        let page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE).max(1);
        let ids = chain_ids.map(|ChainIds(ids)| ids);
        let requested_chain_ids_count = ids.as_ref().map_or(0, Vec::len);
        let filter_summary = summarize_trade_filters(&filters);
        let span = info_span!(
            "get_trades",
            requested_chain_ids_count,
            page = page_number,
            page_size,
            owners_count = filter_summary.owners_count,
            takers_count = filter_summary.takers_count,
            input_tokens_count = filter_summary.input_tokens_count,
            output_tokens_count = filter_summary.output_tokens_count,
            raindexes_count = filter_summary.raindexes_count,
            has_order_hash_filter = filter_summary.has_order_hash_filter,
            has_time_filter = filter_summary.has_time_filter,
        );

        async move {
            let (local_db, local_ids, sg_ids) = self.classify_chains(ids)?;
            let local_chain_ids_count = local_ids.len();
            let subgraph_chain_ids_count = sg_ids.len();

            info!(
                local_chain_ids_count,
                subgraph_chain_ids_count, "fetching trades"
            );

            let mut all_trades = Vec::new();
            let mut total_count: u64 = 0;
            let mut local_fetch_ms = None;
            let mut local_count_ms = None;
            let mut local_rows = 0usize;
            let mut local_total_count = 0u64;
            let mut local_query_paginated = false;
            let mut subgraph_fetch_ms = None;
            let mut subgraph_rows_before_token_filter = 0usize;
            let mut subgraph_rows_after_token_filter = 0usize;

            if let Some(db) = local_db {
                let (local_pagination, is_local_query_paginated) =
                    local_trades_pagination(!sg_ids.is_empty(), page_number, page_size);
                local_query_paginated = is_local_query_paginated;
                let local_tokens = filters
                    .tokens
                    .clone()
                    .map(|tokens| FetchTradesTokensFilter {
                        inputs: tokens.inputs.unwrap_or_default(),
                        outputs: tokens.outputs.unwrap_or_default(),
                    })
                    .unwrap_or_default();
                let local_fetch_started = Timing::now();
                let local_trades = match fetch_trades(
                    &db,
                    FetchTradesArgs {
                        chain_ids: local_ids.clone(),
                        raindex_addresses: filters.raindex_addresses.clone().unwrap_or_default(),
                        owners: filters.owners.clone(),
                        takers: filters.takers.clone(),
                        order_hash: filters.order_hash,
                        order_hashes: Vec::new(),
                        tokens: local_tokens.clone(),
                        time_filter: filters.time_filter.clone().unwrap_or_default(),
                        pagination: local_pagination,
                    },
                )
                .await
                {
                    Ok(trades) => trades,
                    Err(err) => {
                        error!(
                            duration_ms = local_fetch_started.elapsed_ms(),
                            local_chain_ids_count,
                            error = %err,
                            "getTrades local DB fetch failed"
                        );
                        return Err(err.into());
                    }
                };
                local_fetch_ms = Some(local_fetch_started.elapsed_ms());
                local_rows = local_trades.len();

                let local_count_started = Timing::now();
                let count_rows = match fetch_trades_count(
                    &db,
                    FetchTradesArgs {
                        chain_ids: local_ids,
                        raindex_addresses: filters.raindex_addresses.clone().unwrap_or_default(),
                        owners: filters.owners.clone(),
                        takers: filters.takers.clone(),
                        order_hash: filters.order_hash,
                        order_hashes: Vec::new(),
                        tokens: local_tokens,
                        time_filter: filters.time_filter.clone().unwrap_or_default(),
                        pagination: PaginationParams::default(),
                    },
                )
                .await
                {
                    Ok(count_rows) => count_rows,
                    Err(err) => {
                        error!(
                            duration_ms = local_count_started.elapsed_ms(),
                            local_chain_ids_count,
                            error = %err,
                            "getTrades local DB count failed"
                        );
                        return Err(err.into());
                    }
                };
                local_count_ms = Some(local_count_started.elapsed_ms());
                local_total_count = extract_trades_count(&count_rows);
                total_count += local_total_count;
                all_trades.extend(
                    local_trades
                        .into_iter()
                        .map(RaindexTrade::try_from_local_db_trade)
                        .collect::<Result<Vec<_>, _>>()?,
                );
                debug!(
                    local_chain_ids_count,
                    rows = local_rows,
                    total_count = local_total_count,
                    fetch_ms = local_fetch_ms,
                    count_ms = local_count_ms,
                    "getTrades local DB completed"
                );
                info!(
                    source = "local_db",
                    local_chain_ids_count,
                    rows = local_rows,
                    total_count = local_total_count,
                    fetch_ms = local_fetch_ms,
                    count_ms = local_count_ms,
                    local_query_paginated,
                    "fetched trades"
                );
            }

            if !sg_ids.is_empty() {
                let multi_subgraph_args = self.get_multi_subgraph_args(Some(sg_ids))?;
                let sg_filters: SgTradesListQueryFilters = filters.clone().into();
                let sg_token_filter = filters
                    .tokens
                    .clone()
                    .and_then(Option::<SgTradesTokensFilterArgs>::from)
                    .map(normalize_trade_tokens);
                let name_to_chain_id: std::collections::HashMap<&str, u32> = multi_subgraph_args
                    .iter()
                    .flat_map(|(chain_id, args)| {
                        args.iter().map(|arg| (arg.name.as_str(), *chain_id))
                    })
                    .collect();
                let client = MultiRaindexSubgraphClient::new(
                    multi_subgraph_args.values().flatten().cloned().collect(),
                );
                let subgraph_fetch_started = Timing::now();
                let sg_trades = match client.trades_list_all(sg_filters).await {
                    Ok(trades) => trades,
                    Err(err) => {
                        error!(
                            duration_ms = subgraph_fetch_started.elapsed_ms(),
                            subgraph_chain_ids_count,
                            error = %err,
                            "getTrades subgraph fetch failed"
                        );
                        return Err(err.into());
                    }
                };
                subgraph_fetch_ms = Some(subgraph_fetch_started.elapsed_ms());
                subgraph_rows_before_token_filter = sg_trades.len();
                let sg_trades: Vec<_> = if let Some(tokens) = sg_token_filter {
                    let filtered_trades: Vec<_> = sg_trades
                        .into_iter()
                        .filter(|trade| sg_trade_matches_token_filter(&trade.trade, &tokens))
                        .collect();
                    total_count += filtered_trades.len() as u64;
                    subgraph_rows_after_token_filter = filtered_trades.len();
                    filtered_trades
                } else {
                    total_count += sg_trades.len() as u64;
                    subgraph_rows_after_token_filter = sg_trades.len();
                    sg_trades
                };
                for trade_with_name in sg_trades {
                    let chain_id = name_to_chain_id
                        .get(trade_with_name.subgraph_name.as_str())
                        .copied()
                        .ok_or(RaindexError::SubgraphNotFound(
                            trade_with_name.subgraph_name.clone(),
                            trade_with_name.trade.id.0.clone(),
                        ))?;
                    all_trades.push(RaindexTrade::try_from_sg_trade(
                        chain_id,
                        trade_with_name.trade,
                    )?);
                }
                debug!(
                    subgraph_chain_ids_count,
                    rows_before_token_filter = subgraph_rows_before_token_filter,
                    rows_after_token_filter = subgraph_rows_after_token_filter,
                    fetch_ms = subgraph_fetch_ms,
                    "getTrades subgraph completed"
                );
                info!(
                    source = "subgraph",
                    subgraph_chain_ids_count,
                    rows_before_token_filter = subgraph_rows_before_token_filter,
                    rows_after_token_filter = subgraph_rows_after_token_filter,
                    fetch_ms = subgraph_fetch_ms,
                    "fetched trades"
                );
            }

            let merge_started = Timing::now();
            all_trades.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| a.id.cmp(&b.id)));
            let offset = if local_query_paginated && subgraph_chain_ids_count == 0 {
                0
            } else {
                ((page_number - 1) as usize) * (page_size as usize)
            };
            let limit = page_size as usize;
            let merged_rows_before_pagination = all_trades.len();
            let trades: Vec<_> = all_trades.into_iter().skip(offset).take(limit).collect();
            let returned_rows = trades.len();
            let summary = RaindexPairSummary::from_trades(&trades)?;
            let merge_sort_page_ms = merge_started.elapsed_ms();
            debug!(
                page = page_number,
                page_size,
                requested_chain_ids_count,
                local_chain_ids_count,
                subgraph_chain_ids_count,
                owners_count = filter_summary.owners_count,
                takers_count = filter_summary.takers_count,
                raindexes_count = filter_summary.raindexes_count,
                has_order_hash = filter_summary.has_order_hash_filter,
                has_time_filter = filter_summary.has_time_filter,
                input_tokens_count = filter_summary.input_tokens_count,
                output_tokens_count = filter_summary.output_tokens_count,
                local_fetch_ms,
                local_count_ms,
                local_query_paginated,
                local_rows,
                local_total_count,
                subgraph_fetch_ms,
                subgraph_rows_before_token_filter,
                subgraph_rows_after_token_filter,
                merged_rows_before_pagination,
                returned_rows,
                total_count,
                merge_sort_page_ms,
                total_ms = route_started.elapsed_ms(),
                "getTrades completed"
            );
            info!(
                page = page_number,
                page_size,
                requested_chain_ids_count,
                local_chain_ids_count,
                subgraph_chain_ids_count,
                owners_count = filter_summary.owners_count,
                takers_count = filter_summary.takers_count,
                raindexes_count = filter_summary.raindexes_count,
                has_order_hash = filter_summary.has_order_hash_filter,
                has_time_filter = filter_summary.has_time_filter,
                input_tokens_count = filter_summary.input_tokens_count,
                output_tokens_count = filter_summary.output_tokens_count,
                local_fetch_ms,
                local_count_ms,
                local_query_paginated,
                local_rows,
                local_total_count,
                subgraph_fetch_ms,
                subgraph_rows_before_token_filter,
                subgraph_rows_after_token_filter,
                merged_rows_before_pagination,
                returned_rows,
                total_count,
                merge_sort_page_ms,
                duration_ms = route_started.elapsed_ms(),
                "completed get_trades"
            );
            Ok(RaindexTradesListResult {
                trades,
                total_count,
                summary: Some(summary),
            })
        }
        .instrument(span)
        .await
    }
}

#[cfg(not(target_family = "wasm"))]
impl RaindexClient {
    /// Fetches every trade matching a filter in one source read.
    ///
    /// This is intended for bounded aggregate queries, such as a 24-hour market
    /// window. It deliberately has no public WASM binding because interactive
    /// callers should use the paginated `get_trades` API.
    pub(crate) async fn get_trades_unpaginated(
        &self,
        chain_id: u32,
        filters: GetTradesFilters,
    ) -> Result<Vec<RaindexTrade>, RaindexError> {
        match self.query_source(chain_id) {
            QuerySource::LocalDb(db) => {
                let tokens = filters
                    .tokens
                    .clone()
                    .map(|tokens| FetchTradesTokensFilter {
                        inputs: tokens.inputs.unwrap_or_default(),
                        outputs: tokens.outputs.unwrap_or_default(),
                    })
                    .unwrap_or_default();
                fetch_trades(
                    &db,
                    FetchTradesArgs {
                        chain_ids: vec![chain_id],
                        raindex_addresses: filters.raindex_addresses.unwrap_or_default(),
                        owners: filters.owners,
                        takers: filters.takers,
                        order_hash: filters.order_hash,
                        order_hashes: Vec::new(),
                        tokens,
                        time_filter: filters.time_filter.unwrap_or_default(),
                        pagination: PaginationParams::default(),
                    },
                )
                .await?
                .into_iter()
                .map(RaindexTrade::try_from_local_db_trade)
                .collect()
            }
            QuerySource::Subgraph => {
                let multi_subgraph_args = self.get_multi_subgraph_args(Some(vec![chain_id]))?;
                let token_filter = filters
                    .tokens
                    .clone()
                    .and_then(Option::<SgTradesTokensFilterArgs>::from)
                    .map(normalize_trade_tokens);
                let name_to_chain_id: std::collections::HashMap<&str, u32> = multi_subgraph_args
                    .iter()
                    .flat_map(|(chain_id, args)| {
                        args.iter().map(|arg| (arg.name.as_str(), *chain_id))
                    })
                    .collect();
                let client = MultiRaindexSubgraphClient::new(
                    multi_subgraph_args.values().flatten().cloned().collect(),
                );
                let mut trades = client
                    .trades_list_all(filters.into())
                    .await?
                    .into_iter()
                    .filter(|trade| {
                        token_filter.as_ref().is_none_or(|tokens| {
                            sg_trade_matches_token_filter(&trade.trade, tokens)
                        })
                    })
                    .map(|trade| {
                        let resolved_chain_id = name_to_chain_id
                            .get(trade.subgraph_name.as_str())
                            .copied()
                            .ok_or_else(|| {
                                RaindexError::SubgraphNotFound(
                                    trade.subgraph_name.clone(),
                                    trade.trade.id.0.clone(),
                                )
                            })?;
                        RaindexTrade::try_from_sg_trade(resolved_chain_id, trade.trade)
                    })
                    .collect::<Result<Vec<_>, RaindexError>>()?;
                trades.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| a.id.cmp(&b.id)));
                Ok(trades)
            }
        }
    }

    /// Returns at most one latest trade for every requested source token.
    ///
    /// The local DB performs this as one window-function query. The subgraph
    /// fallback issues bounded, concurrent `first: 1` queries so historical
    /// last prices never require downloading complete trade histories.
    pub(crate) async fn get_latest_market_trades(
        &self,
        chain_id: u32,
        quote_token: Address,
        mut base_tokens: Vec<Address>,
        raindex_addresses: Vec<Address>,
    ) -> Result<Vec<RaindexTrade>, RaindexError> {
        base_tokens.sort_unstable();
        base_tokens.dedup();
        if base_tokens.is_empty() {
            return Ok(Vec::new());
        }

        match self.query_source(chain_id) {
            QuerySource::LocalDb(db) => fetch_latest_trades_per_token(
                &db,
                FetchLatestTradesPerTokenArgs {
                    chain_id,
                    raindex_addresses,
                    quote_token,
                    base_tokens,
                },
            )
            .await?
            .into_iter()
            .map(RaindexTrade::try_from_local_db_trade)
            .collect(),
            QuerySource::Subgraph => {
                let multi_subgraph_args = self.get_multi_subgraph_args(Some(vec![chain_id]))?;
                let name_to_chain_id: std::collections::HashMap<&str, u32> = multi_subgraph_args
                    .iter()
                    .flat_map(|(chain_id, args)| {
                        args.iter().map(|arg| (arg.name.as_str(), *chain_id))
                    })
                    .collect();
                let client = MultiRaindexSubgraphClient::new(
                    multi_subgraph_args.values().flatten().cloned().collect(),
                );
                let queries = base_tokens.into_iter().map(|base_token| {
                    client.trades_list(
                        latest_market_trade_filter(quote_token, base_token, &raindex_addresses),
                        SgPaginationArgs {
                            page: 1,
                            page_size: 1,
                        },
                    )
                });
                let mut trades = Vec::new();
                for result in join_all(queries).await {
                    trades.extend(result?);
                }
                trades
                    .into_iter()
                    .map(|trade_with_name| {
                        let resolved_chain_id = name_to_chain_id
                            .get(trade_with_name.subgraph_name.as_str())
                            .copied()
                            .ok_or_else(|| {
                                RaindexError::SubgraphNotFound(
                                    trade_with_name.subgraph_name.clone(),
                                    trade_with_name.trade.id.0.clone(),
                                )
                            })?;
                        RaindexTrade::try_from_sg_trade(resolved_chain_id, trade_with_name.trade)
                    })
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raindex_client::tests::get_test_yaml;
    use alloy::primitives::{address, b256};
    #[cfg(not(target_family = "wasm"))]
    use httpmock::MockServer;
    use raindex_subgraph_client::types::common::{
        SgErc20, SgRaindex, SgTradeEvent, SgTradeEventTypename, SgTradeRef,
        SgTradeStructPartialOrder, SgTradeVaultBalanceChange, SgTransaction,
        SgVaultBalanceChangeVault,
    };
    #[cfg(not(target_family = "wasm"))]
    use serde_json::json;
    #[cfg(not(target_family = "wasm"))]
    use tracing_test::traced_test;
    #[cfg(not(target_family = "wasm"))]
    use {
        crate::local_db::query::{
            FromDbJson, LocalDbQueryError, LocalDbQueryExecutor, SqlStatement, SqlStatementBatch,
        },
        crate::raindex_client::{local_db::LocalDb, tests::new_with_local_db},
        async_trait::async_trait,
        std::sync::{Arc, Mutex},
    };

    #[test]
    fn latest_market_filter_matches_both_trade_directions() {
        let quote = address!("2222222222222222222222222222222222222222");
        let base = address!("1111111111111111111111111111111111111111");
        let raindex = address!("3333333333333333333333333333333333333333");

        let filter = latest_market_trade_filter(quote, base, &[raindex]);

        assert_eq!(filter.raindex_in, vec![raindex.to_string()]);
        let directions = filter.or.expect("directional pair filters");
        assert_eq!(directions.len(), 2);
        let token = |direction: &SgTradesListQueryFilters, input: bool| {
            let change = if input {
                direction.input_vault_balance_change_.as_ref()
            } else {
                direction.output_vault_balance_change_.as_ref()
            }
            .expect("token change filter");
            change
                .vault_
                .as_ref()
                .expect("vault filter")
                .token_in
                .first()
                .expect("token")
                .clone()
        };
        assert_eq!(token(&directions[0], true), quote.to_string());
        assert_eq!(token(&directions[0], false), base.to_string());
        assert_eq!(token(&directions[1], true), base.to_string());
        assert_eq!(token(&directions[1], false), quote.to_string());
    }

    #[cfg(not(target_family = "wasm"))]
    #[derive(Clone)]
    struct EmptyLocalDb;

    #[cfg(not(target_family = "wasm"))]
    #[async_trait]
    impl LocalDbQueryExecutor for EmptyLocalDb {
        async fn execute_batch(&self, _batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            Ok(())
        }

        async fn query_json<T>(&self, _stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: FromDbJson,
        {
            serde_json::from_str("[]")
                .map_err(|error| LocalDbQueryError::deserialization(error.to_string()))
        }

        async fn query_text(&self, _stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            Err(LocalDbQueryError::not_implemented("query_text"))
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::not_implemented("wipe_and_recreate"))
        }
    }

    #[cfg(not(target_family = "wasm"))]
    #[derive(Clone)]
    struct UnexpectedLocalDb;

    #[cfg(not(target_family = "wasm"))]
    #[async_trait]
    impl LocalDbQueryExecutor for UnexpectedLocalDb {
        async fn execute_batch(&self, _batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::database("unexpected local DB write"))
        }

        async fn query_json<T>(&self, _stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: FromDbJson,
        {
            Err(LocalDbQueryError::database("unexpected local DB read"))
        }

        async fn query_text(&self, _stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            Err(LocalDbQueryError::database("unexpected local DB read"))
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::database("unexpected local DB write"))
        }
    }

    #[test]
    fn trade_filter_trace_summary_counts_optional_filters() {
        let filters = GetTradesFilters {
            owners: vec![address!("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")],
            takers: vec![
                address!("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                address!("0xcccccccccccccccccccccccccccccccccccccccc"),
            ],
            order_hash: Some(b256!(
                "0x0000000000000000000000000000000000000000000000000000000000abcdef"
            )),
            tokens: Some(GetTradesTokenFilter {
                inputs: Some(vec![address!("0x1111111111111111111111111111111111111111")]),
                outputs: Some(vec![address!("0x2222222222222222222222222222222222222222")]),
            }),
            raindex_addresses: Some(vec![address!("0xdddddddddddddddddddddddddddddddddddddddd")]),
            time_filter: Some(TimeFilter {
                start: Some(10),
                end: Some(20),
            }),
        };

        assert_eq!(
            summarize_trade_filters(&filters),
            TradeFilterTraceSummary {
                owners_count: 1,
                takers_count: 2,
                input_tokens_count: 1,
                output_tokens_count: 1,
                raindexes_count: 1,
                has_order_hash_filter: true,
                has_time_filter: true,
            }
        );
    }

    #[test]
    fn local_trades_pagination_is_applied_for_local_only_queries() {
        let (pagination, is_paginated) = local_trades_pagination(false, 2, 25);

        assert!(is_paginated);
        assert_eq!(pagination.page, Some(2));
        assert_eq!(pagination.page_size, Some(25));
    }

    #[test]
    fn local_trades_pagination_is_deferred_for_mixed_source_queries() {
        let (pagination, is_paginated) = local_trades_pagination(true, 2, 25);

        assert!(!is_paginated);
        assert_eq!(pagination.page, None);
        assert_eq!(pagination.page_size, None);
    }

    #[test]
    fn maps_trade_filters_to_subgraph_filters() {
        let owner = address!("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let taker = address!("0xcccccccccccccccccccccccccccccccccccccccc");
        let raindex = address!("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        let order_hash =
            b256!("0x0000000000000000000000000000000000000000000000000000000000abcdef");

        let filters: SgTradesListQueryFilters = GetTradesFilters {
            owners: vec![owner],
            takers: vec![taker],
            order_hash: Some(order_hash),
            raindex_addresses: Some(vec![raindex]),
            time_filter: Some(TimeFilter {
                start: Some(10),
                end: Some(20),
            }),
            ..Default::default()
        }
        .into();

        let order_filter = filters.order_.unwrap();
        assert_eq!(order_filter.owner_in, vec![SgBytes(owner.to_string())]);
        assert_eq!(
            order_filter.order_hash,
            Some(SgBytes(order_hash.to_string()))
        );
        let trade_event_filter = filters.trade_event_.unwrap();
        assert_eq!(
            trade_event_filter.sender_in,
            vec![SgBytes(taker.to_string())]
        );
        assert_eq!(filters.raindex_in, vec![format!("{raindex:#x}")]);
        assert_eq!(filters.timestamp_gte, Some(SgBigInt("10".to_string())));
        assert_eq!(filters.timestamp_lte, Some(SgBigInt("20".to_string())));
    }

    #[test]
    fn deserializes_partial_filters_without_owners() {
        let json = serde_json::json!({
            "tokens": {
                "inputs": ["0x1111111111111111111111111111111111111111"]
            }
        });

        let filters: GetTradesFilters = serde_json::from_value(json).unwrap();

        assert!(filters.owners.is_empty());
        assert!(filters.tokens.is_some());
    }

    #[test]
    fn maps_equal_token_filters_to_subgraph_direction_union() {
        let owner = address!("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let raindex = address!("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        let token_a = address!("0x1111111111111111111111111111111111111111");
        let token_b = address!("0x2222222222222222222222222222222222222222");
        let filters: SgTradesListQueryFilters = GetTradesFilters {
            owners: vec![owner],
            raindex_addresses: Some(vec![raindex]),
            time_filter: Some(TimeFilter {
                start: Some(10),
                end: Some(20),
            }),
            tokens: Some(GetTradesTokenFilter {
                inputs: Some(vec![token_b, token_a, token_a]),
                outputs: Some(vec![token_a, token_b]),
            }),
            ..Default::default()
        }
        .into();

        let order_filter = filters.order_.as_ref().unwrap();
        assert_eq!(order_filter.owner_in, vec![SgBytes(owner.to_string())]);
        assert_eq!(filters.timestamp_gte, Some(SgBigInt("10".to_string())));
        assert_eq!(filters.timestamp_lte, Some(SgBigInt("20".to_string())));
        assert_eq!(filters.raindex_in, vec![format!("{raindex:#x}")]);
        let directions = filters.or.as_ref().expect("token direction union");
        assert_eq!(directions.len(), 2);
        assert!(filters.input_vault_balance_change_.is_none());
        assert!(filters.output_vault_balance_change_.is_none());
        assert!(filters.trade_event_.is_none());
        assert_eq!(
            directions[0]
                .input_vault_balance_change_
                .as_ref()
                .unwrap()
                .vault_
                .as_ref()
                .unwrap()
                .token_in,
            vec![token_a.to_string(), token_b.to_string()]
        );
        assert_eq!(
            directions[1]
                .output_vault_balance_change_
                .as_ref()
                .unwrap()
                .vault_
                .as_ref()
                .unwrap()
                .token_in,
            vec![token_a.to_string(), token_b.to_string()]
        );
    }

    #[test]
    fn maps_directional_token_filters_to_subgraph_intersection() {
        let input = address!("0x1111111111111111111111111111111111111111");
        let output = address!("0x2222222222222222222222222222222222222222");
        let filters: SgTradesListQueryFilters = GetTradesFilters {
            tokens: Some(GetTradesTokenFilter {
                inputs: Some(vec![input]),
                outputs: Some(vec![output]),
            }),
            ..Default::default()
        }
        .into();

        assert!(filters.or.is_none());
        assert_eq!(
            filters
                .input_vault_balance_change_
                .unwrap()
                .vault_
                .unwrap()
                .token_in,
            vec![input.to_string()]
        );
        assert_eq!(
            filters
                .output_vault_balance_change_
                .unwrap()
                .vault_
                .unwrap()
                .token_in,
            vec![output.to_string()]
        );
    }

    #[cfg(not(target_family = "wasm"))]
    #[traced_test]
    #[tokio::test]
    async fn test_get_trades_empty_result_logs_completion() {
        let sg_server = MockServer::start_async().await;
        sg_server.mock(|when, then| {
            when.path("/sg");
            then.status(200).json_body_obj(&json!({
                "data": { "trades": [] }
            }));
        });

        let client = RaindexClient::new(
            vec![get_test_yaml(
                &sg_server.url("/sg"),
                &sg_server.url("/sg"),
                "http://localhost:3000",
                "http://localhost:3000",
            )],
            None,
            None,
        )
        .await
        .unwrap();

        let result = client
            .get_trades(
                Some(ChainIds(vec![1])),
                Some(GetTradesFilters::default()),
                Some(1),
                None,
            )
            .await
            .unwrap();

        assert_eq!(result.total_count(), 0);
        assert!(result.trades().is_empty());
        assert!(logs_contain("completed get_trades"));
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn unpaginated_trades_use_ready_local_db_without_subgraph_fallback() {
        let sg_server = MockServer::start_async().await;
        let subgraph = sg_server.mock(|when, then| {
            when.path("/sg");
            then.status(500);
        });
        let client = new_with_local_db(
            vec![get_test_yaml(
                &sg_server.url("/sg"),
                &sg_server.url("/sg"),
                "http://localhost:3000",
                "http://localhost:3000",
            )],
            LocalDb::new(EmptyLocalDb),
            vec![1],
        )
        .await;

        let trades = client
            .get_trades_unpaginated(1, GetTradesFilters::default())
            .await
            .unwrap();

        assert!(trades.is_empty());
        subgraph.assert_hits(0);
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn unpaginated_trades_use_subgraph_when_local_db_is_not_ready() {
        let sg_server = MockServer::start_async().await;
        let subgraph = sg_server.mock(|when, then| {
            when.path("/sg");
            then.status(200).json_body_obj(&json!({
                "data": { "trades": [] }
            }));
        });
        let mut client = RaindexClient::new(
            vec![get_test_yaml(
                &sg_server.url("/sg"),
                &sg_server.url("/sg"),
                "http://localhost:3000",
                "http://localhost:3000",
            )],
            None,
            None,
        )
        .await
        .unwrap();
        client.local_db_state.db = Arc::new(Mutex::new(Some(LocalDb::new(UnexpectedLocalDb))));
        client.local_db_state.sync_configured_chains.insert(1);

        let trades = client
            .get_trades_unpaginated(1, GetTradesFilters::default())
            .await
            .unwrap();

        assert!(trades.is_empty());
        subgraph.assert_hits(1);
    }

    fn test_sg_trade(input_token: Address, output_token: Address) -> SgTrade {
        fn erc20(token: Address) -> SgErc20 {
            SgErc20 {
                id: SgBytes(token.to_string()),
                address: SgBytes(token.to_string()),
                name: Some("Token".to_string()),
                symbol: Some("TKN".to_string()),
                decimals: Some(SgBigInt("18".to_string())),
            }
        }

        fn balance_change(token: Address) -> SgTradeVaultBalanceChange {
            SgTradeVaultBalanceChange {
                id: SgBytes(format!("{}-change", token)),
                __typename: "TradeVaultBalanceChange".to_string(),
                amount: SgBytes("0x01".to_string()),
                new_vault_balance: SgBytes("0x01".to_string()),
                old_vault_balance: SgBytes("0x00".to_string()),
                vault: SgVaultBalanceChangeVault {
                    id: SgBytes(format!("{}-vault", token)),
                    vault_id: SgBytes("0x01".to_string()),
                    token: erc20(token),
                },
                timestamp: SgBigInt("10".to_string()),
                transaction: SgTransaction {
                    id: SgBytes(
                        "0x0000000000000000000000000000000000000000000000000000000000000001"
                            .to_string(),
                    ),
                    from: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                    block_number: SgBigInt("1".to_string()),
                    timestamp: SgBigInt("10".to_string()),
                },
                raindex: SgRaindex {
                    id: SgBytes("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
                },
                trade: SgTradeRef {
                    trade_event: SgTradeEventTypename {
                        __typename: "TakeOrder".to_string(),
                    },
                },
            }
        }

        SgTrade {
            id: SgBytes(
                "0x0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            ),
            trade_event: SgTradeEvent {
                id: SgBytes(
                    "0x0000000000000000000000000000000000000000000000000000000000000001"
                        .to_string(),
                ),
                __typename: "TakeOrder".to_string(),
                transaction: SgTransaction {
                    id: SgBytes(
                        "0x0000000000000000000000000000000000000000000000000000000000000001"
                            .to_string(),
                    ),
                    from: SgBytes("0x0000000000000000000000000000000000000000".to_string()),
                    block_number: SgBigInt("1".to_string()),
                    timestamp: SgBigInt("10".to_string()),
                },
                sender: SgBytes("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
            },
            output_vault_balance_change: balance_change(output_token),
            order: SgTradeStructPartialOrder {
                id: SgBytes("0xorder".to_string()),
                order_hash: SgBytes(
                    "0x0000000000000000000000000000000000000000000000000000000000000001"
                        .to_string(),
                ),
                owner: SgBytes("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
            },
            input_vault_balance_change: balance_change(input_token),
            timestamp: SgBigInt("10".to_string()),
            raindex: SgRaindex {
                id: SgBytes("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
            },
        }
    }

    #[test]
    fn same_token_post_filter_matches_either_input_or_output_token() {
        let token_a = address!("0x1111111111111111111111111111111111111111");
        let token_b = address!("0x2222222222222222222222222222222222222222");
        let token_c = address!("0x3333333333333333333333333333333333333333");
        let tokens = normalize_trade_tokens(SgTradesTokensFilterArgs {
            inputs: vec![token_a.to_string(), token_b.to_string()],
            outputs: vec![token_b.to_string(), token_a.to_string()],
        });

        assert!(sg_trade_matches_token_filter(
            &test_sg_trade(token_a, token_c),
            &tokens
        ));
        assert!(sg_trade_matches_token_filter(
            &test_sg_trade(token_c, token_b),
            &tokens
        ));
        assert!(!sg_trade_matches_token_filter(
            &test_sg_trade(token_c, token_c),
            &tokens
        ));
    }

    #[test]
    fn directional_post_filter_requires_matching_input_and_output_tokens() {
        let input = address!("0x1111111111111111111111111111111111111111");
        let output = address!("0x2222222222222222222222222222222222222222");
        let other = address!("0x3333333333333333333333333333333333333333");
        let tokens = normalize_trade_tokens(SgTradesTokensFilterArgs {
            inputs: vec![input.to_string()],
            outputs: vec![output.to_string()],
        });

        assert!(sg_trade_matches_token_filter(
            &test_sg_trade(input, output),
            &tokens
        ));
        assert!(!sg_trade_matches_token_filter(
            &test_sg_trade(input, other),
            &tokens
        ));
        assert!(!sg_trade_matches_token_filter(
            &test_sg_trade(other, output),
            &tokens
        ));
    }
}
