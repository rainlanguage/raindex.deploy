use super::*;
use crate::raindex_client::orders::RaindexOrder;
use crate::raindex_client::orders_list::RaindexOrders;
use crate::raindex_client::vaults::RaindexVault;
use crate::utils::timing::Timing;
use alloy::primitives::{Address, B256};
use rain_math_float::Float;
use raindex_bindings::IRaindexV7::{OrderV4, SignedContextV2, IOV2};
use raindex_quote::{
    get_order_quotes, get_order_quotes_for_pairs, BatchOrderQuotesResponse, NoopInjector,
    OrderQuoteValue, Pair, SignedContextInjector,
};
use raindex_subgraph_client::utils::float::{F0, F1};
use std::ops::{Div, Mul};
use tracing::{debug, error, info, info_span, warn, Instrument};
use wasm_bindgen_utils::impl_wasm_traits;

#[derive(Serialize, Deserialize, Debug, Clone, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct RaindexOrderQuote {
    pub pair: Pair,
    pub block_number: u64,
    #[tsify(optional)]
    pub data: Option<RaindexOrderQuoteValue>,
    pub success: bool,
    #[tsify(optional)]
    pub error: Option<String>,
    /// Composed signed context that was attached to the quote RPC: oracle
    /// entries first, then any injector-contributed entries. Propagated so
    /// the candidate builder can reuse the same context the quote saw.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[tsify(optional)]
    pub signed_context: Vec<SignedContextV2>,
}
impl_wasm_traits!(RaindexOrderQuote);
impl RaindexOrderQuote {
    pub fn try_from_batch_order_quotes_response(
        value: BatchOrderQuotesResponse,
    ) -> Result<Self, RaindexError> {
        Ok(Self {
            pair: value.pair,
            block_number: value.block_number,
            data: value
                .data
                .map(RaindexOrderQuoteValue::try_from_order_quote_value)
                .transpose()?,
            success: value.success,
            error: value.error,
            signed_context: value.signed_context,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct RaindexOrderQuoteValue {
    #[tsify(type = "Hex")]
    pub max_output: Float,
    pub formatted_max_output: String,
    #[tsify(type = "Hex")]
    pub max_input: Float,
    pub formatted_max_input: String,
    #[tsify(type = "Hex")]
    pub ratio: Float,
    pub formatted_ratio: String,
    #[tsify(type = "Hex")]
    pub inverse_ratio: Float,
    pub formatted_inverse_ratio: String,
    /// `maxOutput` expressed as a percentage of the current balance of the
    /// output vault this pair sells from, formatted like the other amounts
    /// (e.g. `"25"` for 25%). `None` (omitted) when the output vault cannot be
    /// matched or its balance is zero (the percentage would be undefined).
    /// Populated after the quote is fetched, from already-fetched vault
    /// balances.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub formatted_max_output_as_percent_of_vault: Option<String>,
}
impl_wasm_traits!(RaindexOrderQuoteValue);

/// Formats `amount / balance * 100` the same way the other quote amounts are
/// formatted (so `1` of a `4`-balance vault renders as `"25"`). Returns `None`
/// when `balance` is zero, because the percentage is undefined and dividing
/// would error.
fn format_amount_as_percent_of_balance(
    amount: Float,
    balance: Float,
) -> Result<Option<String>, RaindexError> {
    if balance.is_zero()? {
        return Ok(None);
    }
    let hundred = Float::parse("100".to_string())?;
    let percent = amount.div(balance)?.mul(hundred)?;
    Ok(Some(percent.format()?))
}

/// Finds the balance of the vault whose `(token, vaultId)` matches `io`. The
/// quote pair indices address the on-chain `validInputs`/`validOutputs`
/// (decoded from the order bytes), whose ordering is independent of the
/// subgraph `inputs`/`outputs` arrays, so the match is by identity rather than
/// by position.
fn vault_balance_for_io(io: &IOV2, vaults: &[RaindexVault]) -> Option<Float> {
    vaults
        .iter()
        .find(|vault| {
            vault.token_address() == io.token && B256::from(vault.raw_vault_id()) == io.vaultId
        })
        .map(|vault| vault.balance())
}

impl RaindexOrderQuoteValue {
    pub fn try_from_order_quote_value(value: OrderQuoteValue) -> Result<Self, RaindexError> {
        let inverse_ratio = if F0.eq(value.ratio)? {
            F0
        } else {
            F1.div(value.ratio)?
        };

        let formatted_inverse_ratio = if F0.eq(value.ratio)? {
            "Infinity".to_string()
        } else {
            inverse_ratio.format()?
        };

        let max_input = value.max_output.mul(value.ratio)?;

        Ok(Self {
            max_output: value.max_output,
            formatted_max_output: value.max_output.format()?,
            max_input,
            formatted_max_input: max_input.format()?,
            ratio: value.ratio,
            formatted_ratio: value.ratio.format()?,
            inverse_ratio,
            formatted_inverse_ratio,
            formatted_max_output_as_percent_of_vault: None,
        })
    }
}

#[wasm_export]
impl RaindexOrder {
    /// Executes quotes directly from complete order objects without additional data fetching
    ///
    /// This function performs quote calculations using complete order data structures
    /// that typically come from previous subgraph queries. It generates quotes for all
    /// possible input/output token pairs within each order, providing comprehensive
    /// trading information without requiring additional network calls for order data.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await getOrderQuote();
    /// if (result.error) {
    ///   console.error("Error:", result.error.readableMsg);
    ///   return;
    /// }
    /// const quoteResponses = result.value;
    /// // Do something with the quoteResponses
    /// ```
    #[wasm_export(
        js_name = "getQuotes",
        return_description = "List of batch quote responses with trading pair information",
        unchecked_return_type = "RaindexOrderQuote[]"
    )]
    pub async fn get_quotes(
        &self,
        #[wasm_export(
            js_name = "blockNumber",
            param_description = "Optional specific block number for historical quotes (uses latest if None)"
        )]
        block_number: Option<u64>,
        #[wasm_export(
            js_name = "chunkSize",
            param_description = "Optional quote chunk size override (defaults to 16)"
        )]
        chunk_size: Option<u32>,
    ) -> Result<Vec<RaindexOrderQuote>, RaindexError> {
        self.get_quotes_with_injector(block_number, chunk_size, Address::ZERO, &NoopInjector)
            .await
    }
}

impl RaindexOrder {
    /// Fills in the per-pair vault-relative percentage on a quote's `data`,
    /// so the UI can show each trade's max output amount as a percentage of
    /// the output vault it sells from. This is the drawdown signal: how much
    /// of the vault a single trade depletes (the input side measures inflow,
    /// not drawdown, so it is deliberately not computed). Uses already-fetched
    /// subgraph vault balances; never issues a network call. A successful
    /// quote with no matching vault (or a zero balance) simply leaves the
    /// percentage `None`. `order_v4` must be this order's decoded bytes, whose
    /// `validOutputs` the pair output index addresses.
    fn enrich_quote_with_vault_percentages(
        &self,
        quote: &mut RaindexOrderQuote,
        order_v4: &OrderV4,
    ) -> Result<(), RaindexError> {
        let data = match quote.data.as_mut() {
            Some(data) => data,
            None => return Ok(()),
        };

        let output_balance = order_v4
            .validOutputs
            .get(quote.pair.output_index as usize)
            .and_then(|io| vault_balance_for_io(io, self.output_vaults()));
        if let Some(balance) = output_balance {
            data.formatted_max_output_as_percent_of_vault =
                format_amount_as_percent_of_balance(data.max_output, balance)?;
        }

        Ok(())
    }

    /// Non-wasm variant of [`Self::get_quotes`] that threads a `counterparty`
    /// address and a caller-supplied [`SignedContextInjector`] through to the
    /// quote RPC. Used by single-take flows that need to populate signed
    /// context for gated orders whose `calculate-io` asserts on `signer<0>()`
    /// or similar. The resulting `RaindexOrderQuote.signed_context` carries
    /// the composed `[oracle..., injected...]` list that the multicall saw.
    pub async fn get_quotes_with_injector(
        &self,
        block_number: Option<u64>,
        chunk_size: Option<u32>,
        counterparty: Address,
        injector: &dyn SignedContextInjector,
    ) -> Result<Vec<RaindexOrderQuote>, RaindexError> {
        let span = info_span!(
            "get_order_quotes",
            chain_id = self.chain_id(),
            raindex = %self.raindex(),
            order_hash = %self.order_hash(),
            block_number = ?block_number,
            chunk_size = ?chunk_size,
            counterparty = %counterparty,
        );

        async move {
            let started_at = Timing::now();
            let rpcs = self.get_rpc_urls()?;
            let rpc_url_count = rpcs.len();
            let sg_order = self.clone().into_sg_order()?;
            let order_v4: OrderV4 = sg_order.clone().try_into()?;

            info!(rpc_url_count, "starting order quotes");

            let quote_started_at = Timing::now();
            let order_quotes = get_order_quotes(
                vec![sg_order],
                block_number,
                rpcs.iter().map(|s| s.to_string()).collect(),
                chunk_size.map(|v| v as usize),
                counterparty,
                injector,
            )
            .instrument(info_span!("order_quotes.executing_batch_quote"))
            .await
            .map_err(|err| {
                error!(
                    rpc_url_count,
                    duration_ms = quote_started_at.elapsed_ms(),
                    error = %err,
                    "failed executing order quotes"
                );
                err
            })?;

            let conversion_started_at = Timing::now();
            let mut result_order_quotes = order_quotes
                .into_iter()
                .map(RaindexOrderQuote::try_from_batch_order_quotes_response)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| {
                    error!(
                        duration_ms = conversion_started_at.elapsed_ms(),
                        error = %err,
                        "failed converting order quotes"
                    );
                    err
                })?;

            for quote in result_order_quotes.iter_mut() {
                self.enrich_quote_with_vault_percentages(quote, &order_v4)?;
            }

            let successful_quote_count = result_order_quotes
                .iter()
                .filter(|quote| quote.success)
                .count();
            let failed_quote_count = result_order_quotes
                .len()
                .saturating_sub(successful_quote_count);
            for quote in result_order_quotes.iter().filter(|quote| !quote.success) {
                debug!(
                    input_index = quote.pair.input_index,
                    output_index = quote.pair.output_index,
                    error = ?quote.error,
                    "order quote failed"
                );
            }
            info!(
                rpc_url_count,
                quote_count = result_order_quotes.len(),
                successful_quote_count,
                failed_quote_count,
                duration_ms = started_at.elapsed_ms(),
                "completed order quotes"
            );
            Ok(result_order_quotes)
        }
        .instrument(span)
        .await
    }
}

#[wasm_export]
impl RaindexClient {
    /// Executes quotes for multiple orders in a single multicall
    ///
    /// This function batches all order pairs into one multicall request, which is
    /// significantly more efficient than calling `getQuotes` on each order individually.
    /// Results are positionally aligned with the input orders: `result[i]` contains
    /// the quotes for `orders[i]`.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const orders = (await client.getOrders()).value;
    /// const result = await client.getOrderQuotesBatch(orders, null, null);
    /// if (result.error) {
    ///   console.error("Error:", result.error.readableMsg);
    ///   return;
    /// }
    /// for (const [order, quotes] of orders.map((o, i) => [o, result.value[i]])) {
    ///   console.log("Order", order.orderHash, "quotes:", quotes);
    /// }
    /// ```
    #[wasm_export(
        js_name = "getOrderQuotesBatch",
        return_description = "List of quote lists, one per input order, positionally aligned",
        unchecked_return_type = "RaindexOrderQuote[][]"
    )]
    pub async fn get_order_quotes_batch(
        &self,
        #[wasm_export(
            js_name = "orders",
            param_description = "List of orders to quote; all must share the same chain"
        )]
        orders: &RaindexOrders,
        #[wasm_export(
            js_name = "blockNumber",
            param_description = "Optional specific block number for historical quotes (uses latest if None)"
        )]
        block_number: Option<u64>,
        #[wasm_export(
            js_name = "chunkSize",
            param_description = "Optional quote chunk size override (defaults to 16)"
        )]
        chunk_size: Option<u32>,
    ) -> Result<Vec<Vec<RaindexOrderQuote>>, RaindexError> {
        get_order_quotes_batch(orders.inner(), block_number, chunk_size).await
    }
}

pub async fn get_order_quotes_batch(
    orders: &[RaindexOrder],
    block_number: Option<u64>,
    chunk_size: Option<u32>,
) -> Result<Vec<Vec<RaindexOrderQuote>>, RaindexError> {
    get_order_quotes_batch_with_injector(
        orders,
        block_number,
        chunk_size,
        Address::ZERO,
        &NoopInjector,
    )
    .await
}

#[cfg(not(target_family = "wasm"))]
pub(crate) async fn get_order_quotes_batch_for_pairs(
    orders: &[RaindexOrder],
    selected_pairs: &[Vec<Pair>],
    block_number: Option<u64>,
    chunk_size: Option<u32>,
) -> Result<Vec<Vec<RaindexOrderQuote>>, RaindexError> {
    get_order_quotes_batch_inner(
        orders,
        Some(selected_pairs),
        block_number,
        chunk_size,
        Address::ZERO,
        &NoopInjector,
    )
    .await
}

/// Batch variant of [`get_order_quotes_batch`] that threads a
/// `counterparty` address and a caller-supplied [`SignedContextInjector`]
/// through to the quote RPC. Oracle-fetched contexts come first, followed by
/// entries produced by the injector; the composed list is attached to each
/// `QuoteV3.signedContext` before the multicall and is therefore visible to
/// any `calculate-io` execution run during quoting (e.g. gated orders that
/// assert on `signer<0>()`).
pub async fn get_order_quotes_batch_with_injector(
    orders: &[RaindexOrder],
    block_number: Option<u64>,
    chunk_size: Option<u32>,
    counterparty: Address,
    injector: &dyn SignedContextInjector,
) -> Result<Vec<Vec<RaindexOrderQuote>>, RaindexError> {
    get_order_quotes_batch_inner(
        orders,
        None,
        block_number,
        chunk_size,
        counterparty,
        injector,
    )
    .await
}

async fn get_order_quotes_batch_inner(
    orders: &[RaindexOrder],
    selected_pairs: Option<&[Vec<Pair>]>,
    block_number: Option<u64>,
    chunk_size: Option<u32>,
    counterparty: Address,
    injector: &dyn SignedContextInjector,
) -> Result<Vec<Vec<RaindexOrderQuote>>, RaindexError> {
    let started_at = Timing::now();
    if orders.is_empty() {
        info!(
            order_count = 0usize,
            "quote batch skipped for empty order list"
        );
        return Ok(vec![]);
    }

    let expected_chain_id = orders[0].chain_id();
    for order in &orders[1..] {
        if order.chain_id() != expected_chain_id {
            warn!(
                expected_chain_id,
                found_chain_id = order.chain_id(),
                "quote batch rejected mixed chain IDs"
            );
            return Err(RaindexError::PreflightError(format!(
                "All orders must share the same chain ID, expected {} but found {}",
                expected_chain_id,
                order.chain_id()
            )));
        }
    }

    let rpcs: Vec<String> = orders[0]
        .get_rpc_urls()?
        .into_iter()
        .map(|u| u.to_string())
        .collect();
    let rpc_url_count = rpcs.len();

    let sg_orders = orders
        .iter()
        .map(|o| o.clone().into_sg_order())
        .collect::<Result<Vec<_>, _>>()?;

    if selected_pairs.is_some_and(|pairs| pairs.len() != orders.len()) {
        return Err(RaindexError::PreflightError(
            "Selected quote pairs must align with orders".into(),
        ));
    }
    let pair_counts: Vec<usize> = sg_orders
        .iter()
        .enumerate()
        .map(|(order_index, sg)| {
            let order_v4: OrderV4 = sg.clone().try_into()?;
            let mut count = 0usize;
            for (input_index, input) in order_v4.validInputs.iter().enumerate() {
                for (output_index, output) in order_v4.validOutputs.iter().enumerate() {
                    let is_selected = selected_pairs.is_none_or(|pairs| {
                        pairs[order_index].iter().any(|pair| {
                            pair.input_index == input_index as u32
                                && pair.output_index == output_index as u32
                        })
                    });
                    if input.token != output.token && is_selected {
                        count += 1;
                    }
                }
            }
            Ok::<usize, RaindexError>(count)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let total_pair_count: usize = pair_counts.iter().sum();

    info!(
        order_count = orders.len(),
        expected_chain_id,
        total_pair_count,
        chunk_size = ?chunk_size,
        block_number = ?block_number,
        rpc_url_count,
        "starting order quote batch"
    );

    let flat_results = match selected_pairs {
        Some(pairs) => {
            get_order_quotes_for_pairs(
                sg_orders,
                pairs,
                block_number,
                rpcs,
                chunk_size.map(|v| v as usize),
                counterparty,
                injector,
            )
            .await?
        }
        None => {
            get_order_quotes(
                sg_orders,
                block_number,
                rpcs,
                chunk_size.map(|v| v as usize),
                counterparty,
                injector,
            )
            .await?
        }
    };

    let flat_raindex: Vec<RaindexOrderQuote> = flat_results
        .into_iter()
        .map(RaindexOrderQuote::try_from_batch_order_quotes_response)
        .collect::<Result<Vec<_>, _>>()?;
    if flat_raindex.len() != total_pair_count {
        return Err(RaindexError::PreflightError(format!(
            "Quote response count mismatch: expected {total_pair_count}, got {}",
            flat_raindex.len()
        )));
    }
    let successful_quote_count = flat_raindex.iter().filter(|quote| quote.success).count();
    let failed_quote_count = flat_raindex.len().saturating_sub(successful_quote_count);
    for quote in flat_raindex.iter().filter(|quote| !quote.success) {
        debug!(
            input_index = quote.pair.input_index,
            output_index = quote.pair.output_index,
            error = ?quote.error,
            "order quote failed"
        );
    }

    let mut result = Vec::with_capacity(orders.len());
    let mut offset = 0;
    for count in pair_counts {
        result.push(flat_raindex[offset..offset + count].to_vec());
        offset += count;
    }

    // Express each pair's max input/output as a percentage of the vault it
    // draws from, using the already-fetched subgraph balances. Done per order
    // so the pair indices resolve against that order's own decoded IOs.
    for (order, quotes) in orders.iter().zip(result.iter_mut()) {
        let order_v4: OrderV4 = order.clone().into_sg_order()?.try_into()?;
        for quote in quotes.iter_mut() {
            order.enrich_quote_with_vault_percentages(quote, &order_v4)?;
        }
    }

    for (order, quotes) in orders.iter().zip(&result) {
        for quote in quotes.iter().filter(|quote| !quote.success) {
            debug!(
                order_hash = %order.order_hash(),
                raindex = %order.raindex(),
                input_index = quote.pair.input_index,
                output_index = quote.pair.output_index,
                error = ?quote.error,
                "order quote failed for order"
            );
        }
    }

    info!(
        order_count = orders.len(),
        expected_chain_id,
        total_pair_count,
        chunk_size = ?chunk_size,
        block_number = ?block_number,
        rpc_url_count,
        successful_quote_count,
        failed_quote_count,
        duration_ms = started_at.elapsed_ms(),
        "completed order quote batch"
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_family = "wasm"))]
    use super::*;

    #[cfg(not(target_family = "wasm"))]
    mod quote_non_wasm_tests {
        use super::*;
        use crate::local_db::RaindexIdentifier;
        use crate::raindex_client::tests::{get_test_yaml, CHAIN_ID_1_RAINDEX_ADDRESS};
        use alloy::hex::encode_prefixed;
        use alloy::primitives::{b256, Address, Bytes, U256};
        use alloy::{sol, sol_types::SolValue};
        use httpmock::MockServer;
        use rain_math_float::Float;
        use raindex_subgraph_client::utils::float::{F0_5, F2};
        use serde_json::{json, Value};
        use tracing_test::traced_test;

        sol!(
            struct quoteReturn {
                bool exists;
                uint256 outputMax;
                uint256 ioRatio;
            }
        );

        // OZ Multicall returns `bytes[]`; helper to wrap per-target
        // `quote3Return`s into the outer multicall return payload.
        fn encode_multicall_bytes(inner: Vec<quoteReturn>) -> String {
            let elements: Vec<Bytes> = inner.into_iter().map(|r| r.abi_encode().into()).collect();
            encode_prefixed(<Vec<Bytes> as SolValue>::abi_encode(&elements))
        }

        fn get_order1_json() -> Value {
            json!(                        {
              "id": "0x46891c626a8a188610b902ee4a0ce8a7e81915e1b922584f8168d14525899dfb",
              "orderBytes": "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000005f6c104ca9812ef91fe2e26a2e7187b92d3b0e800000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000022009cd210f509c66e18fab61fd30f76fb17c6c6cd09f0972ce0815b5b7630a1b050000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000075000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015020000000c02020002011000000110000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372",
              "orderHash": "0x283508c8f56f4de2f21ee91749d64ec3948c16bc6b4bfe4f8d11e4e67d76f4e0",
              "owner": "0x0000000000000000000000000000000000000000",
              "outputs": [
                {
                  "id": "0x0000000000000000000000000000000000000000",
                  "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                  "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                  "balance": Float::parse("0.987".to_string()).unwrap(),
                  "token": {
                    "id": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                    "address": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                    "name": "Staked FLR",
                    "symbol": "sFLR",
                    "decimals": "18"
                  },
                  "raindex": {
                    "id": CHAIN_ID_1_RAINDEX_ADDRESS
                  },
                  "ordersAsOutput": [],
                  "ordersAsInput": [],
                  "balanceChanges": []
                }
              ],
              "inputs": [
                {
                  "id": "0x0000000000000000000000000000000000000000",
                  "owner": "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                  "vaultId": "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                  "balance": Float::parse("0.79799".to_string()).unwrap(),
                  "token": {
                    "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                    "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                    "name": "WFLR",
                    "symbol": "WFLR",
                    "decimals": "18"
                  },
                  "raindex": {
                    "id": CHAIN_ID_1_RAINDEX_ADDRESS
                  },
                  "ordersAsOutput": [],
                  "ordersAsInput": [],
                  "balanceChanges": []
                },
              ],
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

        #[tokio::test]
        async fn test_get_order_quote() {
            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.path("/sg");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [get_order1_json()]
                    }
                }));
            });

            // block number 1
            server.mock(|when, then| {
                when.path("/rpc").body_contains("blockNumber");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x1",
                }));
            });

            let response_hex = encode_multicall_bytes(vec![quoteReturn {
                exists: true,
                outputMax: U256::from(1),
                ioRatio: U256::from(2),
            }]);
            server.mock(|when, then| {
                when.path("/rpc");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": response_hex,
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &server.url("/sg"),
                    "http://localhost:3000",
                    &server.url("/rpc"),
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
            let res = order.get_quotes(None, None).await.unwrap();
            assert_eq!(res.len(), 1);

            assert!(res[0].data.as_ref().unwrap().max_output.eq(F1).unwrap());

            assert!((res[0].data.as_ref().unwrap().ratio.eq(F2)).unwrap());

            assert!(res[0].success);
            assert_eq!(res[0].error, None);
            assert_eq!(res[0].pair.pair_name, "WFLR/sFLR");
            assert_eq!(res[0].pair.input_index, 0);
            assert_eq!(res[0].pair.output_index, 0);

            let res = res[0].clone();
            let data = res.data.unwrap();
            assert!(data.max_output.eq(F1).unwrap());
            assert_eq!(data.formatted_max_output, "1");
            assert!(data.max_input.eq(F2).unwrap());
            assert_eq!(data.formatted_max_input, "2");
            assert!(data.ratio.eq(F2).unwrap());
            assert_eq!(data.formatted_ratio, "2");
            assert!(data.inverse_ratio.eq(F0_5).unwrap());
            assert_eq!(data.formatted_inverse_ratio, "0.5");
            // The subgraph vaults in `get_order1_json` carry a different
            // vaultId than the order bytes' validOutputs, so no vault matches
            // and the vault-relative percentage stays `None`. This guards
            // against blind index-based matching producing a bogus percentage
            // from a mismatched vault.
            assert_eq!(data.formatted_max_output_as_percent_of_vault, None);
            assert!(res.success);
            assert_eq!(res.error, None);
            assert_eq!(res.pair.pair_name, "WFLR/sFLR");
            assert_eq!(res.pair.input_index, 0);
            assert_eq!(res.pair.output_index, 0);
        }

        // Same order bytes as `get_order1_json`, whose decoded validInputs[0]
        // (WFLR) and validOutputs[0] (sFLR) both carry vaultId 0x12 (18). We
        // point the subgraph vaults at that *same* vaultId so they match the
        // on-chain IOs by (token, vaultId), plus round balances (output sFLR =
        // 4, input WFLR = 5) so the percentages come out exact.
        fn get_order_matching_vaults_json() -> Value {
            let mut order = get_order1_json();
            let matching_vault_id = "18";
            order["outputs"][0]["vaultId"] = json!(matching_vault_id);
            order["outputs"][0]["balance"] = json!(Float::parse("4".to_string()).unwrap());
            order["inputs"][0]["vaultId"] = json!(matching_vault_id);
            order["inputs"][0]["balance"] = json!(Float::parse("5".to_string()).unwrap());
            order
        }

        #[tokio::test]
        async fn test_get_order_quote_vault_percentages() {
            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.path("/sg");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [get_order_matching_vaults_json()]
                    }
                }));
            });

            server.mock(|when, then| {
                when.path("/rpc").body_contains("blockNumber");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x1",
                }));
            });

            // outputMax = 1 (sFLR), ioRatio = 2 => maxInput = 2 (WFLR).
            let response_hex = encode_multicall_bytes(vec![quoteReturn {
                exists: true,
                outputMax: U256::from(1),
                ioRatio: U256::from(2),
            }]);
            server.mock(|when, then| {
                when.path("/rpc");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": response_hex,
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &server.url("/sg"),
                    "http://localhost:3000",
                    &server.url("/rpc"),
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
            let res = order.get_quotes(None, None).await.unwrap();
            assert_eq!(res.len(), 1);
            let data = res[0].data.as_ref().unwrap();

            // The amounts themselves are unchanged: maxOutput = 1, maxInput = 2.
            assert_eq!(data.formatted_max_output, "1");
            assert_eq!(data.formatted_max_input, "2");
            // maxOutput 1 / output vault balance 4 * 100 = 25%. Only the output
            // side is computed (drawdown signal); there is no input percentage.
            assert_eq!(
                data.formatted_max_output_as_percent_of_vault,
                Some("25".to_string())
            );
        }

        #[tokio::test]
        async fn test_get_order_quote_vault_percentages_zero_balance() {
            let server = MockServer::start_async().await;
            let mut order_json = get_order_matching_vaults_json();
            // Zero the output vault balance: dividing by it is undefined, so the
            // output percentage must be `None` even though the output vault
            // matches by (token, vaultId). This isolates the divide-by-zero
            // branch from the no-match branch (which the sibling test covers).
            order_json["outputs"][0]["balance"] = json!(Float::parse("0".to_string()).unwrap());
            server.mock(|when, then| {
                when.path("/sg");
                then.status(200).json_body_obj(&json!({
                    "data": { "orders": [order_json] }
                }));
            });
            server.mock(|when, then| {
                when.path("/rpc").body_contains("blockNumber");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x1",
                }));
            });
            let response_hex = encode_multicall_bytes(vec![quoteReturn {
                exists: true,
                outputMax: U256::from(1),
                ioRatio: U256::from(2),
            }]);
            server.mock(|when, then| {
                when.path("/rpc");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": response_hex,
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &server.url("/sg"),
                    "http://localhost:3000",
                    &server.url("/rpc"),
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
            let res = order.get_quotes(None, None).await.unwrap();
            let data = res[0].data.as_ref().unwrap();
            // The output amount is still reported; only the percentage is
            // suppressed because the vault balance is zero.
            assert_eq!(data.formatted_max_output, "1");
            assert_eq!(data.formatted_max_output_as_percent_of_vault, None);
        }

        #[traced_test]
        #[tokio::test]
        async fn test_get_order_quotes_batch_empty() {
            let result = get_order_quotes_batch(&[], None, None).await;
            assert!(result.is_ok());
            assert!(result.unwrap().is_empty());
            assert!(logs_contain("quote batch skipped for empty order list"));
        }

        #[tokio::test]
        async fn test_get_order_quote_with_chunk_override() {
            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.path("/sg");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [get_order1_json()]
                    }
                }));
            });

            server.mock(|when, then| {
                when.path("/rpc").body_contains("blockNumber");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x1",
                }));
            });

            let response_hex = encode_multicall_bytes(vec![quoteReturn {
                exists: true,
                outputMax: U256::from(1),
                ioRatio: U256::from(2),
            }]);
            server.mock(|when, then| {
                when.path("/rpc");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": response_hex,
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &server.url("/sg"),
                    "http://localhost:3000",
                    &server.url("/rpc"),
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

            let res = order.get_quotes(None, Some(8)).await.unwrap();
            assert_eq!(res.len(), 1);
        }

        #[tokio::test]
        async fn test_get_order_quotes_batch_single_order() {
            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.path("/sg");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [get_order1_json()]
                    }
                }));
            });

            server.mock(|when, then| {
                when.path("/rpc").body_contains("blockNumber");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x1",
                }));
            });

            let response_hex = encode_multicall_bytes(vec![quoteReturn {
                exists: true,
                outputMax: U256::from(1),
                ioRatio: U256::from(2),
            }]);
            server.mock(|when, then| {
                when.path("/rpc");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": response_hex,
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &server.url("/sg"),
                    "http://localhost:3000",
                    &server.url("/rpc"),
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

            let result = get_order_quotes_batch(&[order], None, None).await.unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].len(), 1);
            assert!(result[0][0].success);
            assert_eq!(result[0][0].error, None);
            assert!(result[0][0]
                .data
                .as_ref()
                .unwrap()
                .max_output
                .eq(F1)
                .unwrap());
            assert!(result[0][0].data.as_ref().unwrap().ratio.eq(F2).unwrap());
            assert_eq!(result[0][0].pair.pair_name, "WFLR/sFLR");
        }

        #[tokio::test]
        async fn test_get_order_quotes_batch_multiple_orders() {
            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.path("/sg");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "orders": [get_order1_json()]
                    }
                }));
            });

            server.mock(|when, then| {
                when.path("/rpc").body_contains("blockNumber");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x1",
                }));
            });

            let response_hex = encode_multicall_bytes(vec![
                quoteReturn {
                    exists: true,
                    outputMax: U256::from(1),
                    ioRatio: U256::from(2),
                },
                quoteReturn {
                    exists: true,
                    outputMax: U256::from(2),
                    ioRatio: U256::from(1),
                },
            ]);
            server.mock(|when, then| {
                when.path("/rpc");
                then.json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": response_hex,
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &server.url("/sg"),
                    "http://localhost:3000",
                    &server.url("/rpc"),
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
            let orders = vec![order.clone(), order];

            let result = get_order_quotes_batch(&orders, None, None).await.unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].len(), 1);
            assert_eq!(result[1].len(), 1);
            assert!(result[0][0]
                .data
                .as_ref()
                .unwrap()
                .max_output
                .eq(F1)
                .unwrap());
            assert!(result[0][0].data.as_ref().unwrap().ratio.eq(F2).unwrap());
            assert!(result[1][0]
                .data
                .as_ref()
                .unwrap()
                .max_output
                .eq(F2)
                .unwrap());
            assert!(result[1][0].data.as_ref().unwrap().ratio.eq(F1).unwrap());
            assert_eq!(result[0][0].pair.pair_name, "WFLR/sFLR");
            assert_eq!(result[1][0].pair.pair_name, "WFLR/sFLR");
        }
    }
}
