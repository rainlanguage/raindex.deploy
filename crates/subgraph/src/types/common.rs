use crate::schema;
use serde::{Deserialize, Serialize};
use wasm_bindgen_utils::{impl_wasm_traits, prelude::*};

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, Default)]
#[serde(rename_all = "camelCase")]
pub struct SgOrdersTokensFilterArgs {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}
impl_wasm_traits!(SgOrdersTokensFilterArgs);

#[derive(Debug, Clone, Serialize, Deserialize, Tsify, Default)]
#[serde(rename_all = "camelCase")]
pub struct SgTradesTokensFilterArgs {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}
impl_wasm_traits!(SgTradesTokensFilterArgs);

#[derive(cynic::QueryVariables, Debug, Clone, Tsify)]
pub struct SgIdQueryVariables<'a> {
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub id: &'a cynic::Id,
}

#[derive(Debug, Clone, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct SgOrdersListFilterArgs {
    pub owners: Vec<SgBytes>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub active: Option<bool>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub order_hash: Option<SgBytes>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub tokens: Option<SgOrdersTokensFilterArgs>,
    pub raindexes: Vec<String>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub has_positive_output_vault_balance: Option<bool>,
}
impl_wasm_traits!(SgOrdersListFilterArgs);

#[derive(cynic::QueryVariables, Debug, Clone, Tsify)]
pub struct SgPaginationQueryVariables {
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub first: Option<i32>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub skip: Option<i32>,
}

#[derive(cynic::InputObject, Debug, Clone, Tsify, Default)]
#[cynic(graphql_type = "Order_filter")]
pub struct SgOrdersListQueryFilters {
    #[cynic(rename = "owner_in", skip_serializing_if = "Vec::is_empty")]
    pub owner_in: Vec<SgBytes>,
    #[cynic(rename = "active", skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[cynic(rename = "orderHash", skip_serializing_if = "Option::is_none")]
    pub order_hash: Option<SgBytes>,
    #[cynic(rename = "inputs_", skip_serializing_if = "Option::is_none")]
    pub inputs_: Option<SgVaultTokenFilter>,
    #[cynic(rename = "outputs_", skip_serializing_if = "Option::is_none")]
    pub outputs_: Option<SgVaultTokenFilter>,
    #[cynic(rename = "raindex_in", skip_serializing_if = "Vec::is_empty")]
    pub raindex_in: Vec<String>,
    #[cynic(rename = "or", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub or: Option<Vec<SgOrdersListQueryFilters>>,
}

#[derive(cynic::InputObject, Debug, Clone, Tsify)]
#[cynic(graphql_type = "Vault_filter")]
pub struct SgVaultTokenFilter {
    #[cynic(rename = "token_in", skip_serializing_if = "Vec::is_empty")]
    pub token_in: Vec<String>,
    #[cynic(rename = "vaultId_not", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub vault_id_not: Option<SgBytes>,
    #[cynic(rename = "balance_gt", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub balance_gt: Option<SgBytes>,
}

#[derive(cynic::QueryVariables, Debug, Clone, Tsify)]
pub struct SgOrdersListQueryVariables {
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub first: Option<i32>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub skip: Option<i32>,
    #[cynic(rename = "filters")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub filters: Option<SgOrdersListQueryFilters>,
}

#[derive(cynic::QueryVariables, Debug, Clone, Tsify)]
pub struct SgPaginationWithIdQueryVariables {
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub first: Option<i32>,
    pub id: SgBytes,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub skip: Option<i32>,
}

#[derive(cynic::QueryVariables, Debug, Clone, Tsify)]
pub struct SgPaginationWithTimestampQueryVariables {
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub first: Option<i32>,
    pub id: SgBytes,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub skip: Option<i32>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub timestamp_gte: Option<SgBigInt>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub timestamp_lte: Option<SgBigInt>,
}

#[derive(cynic::QueryVariables, Debug, Clone, Tsify)]
pub struct SgPaginationWithTxIdQueryVariables {
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub first: Option<i32>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub skip: Option<i32>,
    pub tx_id: String,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub raindex_in: Option<Vec<String>>,
}

#[derive(cynic::QueryVariables, Debug, Clone, Tsify)]
pub struct SgOwnerTradesQueryVariables {
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub first: Option<i32>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub skip: Option<i32>,
    pub owner: SgBytes,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub timestamp_gte: Option<SgBigInt>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub timestamp_lte: Option<SgBigInt>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub raindex_in: Option<Vec<String>>,
}

#[derive(cynic::QueryVariables, Debug, Clone, Tsify)]
pub struct SgTradesListQueryVariables {
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub first: Option<i32>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub skip: Option<i32>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub filters: Option<SgTradesListQueryFilters>,
}

#[derive(cynic::InputObject, Debug, Clone, Tsify, Default)]
#[cynic(graphql_type = "Order_filter")]
pub struct SgTradeOrderFilter {
    #[cynic(rename = "owner_in", skip_serializing_if = "Vec::is_empty")]
    pub owner_in: Vec<SgBytes>,
    #[cynic(rename = "orderHash", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub order_hash: Option<SgBytes>,
    #[cynic(rename = "orderHash_in", skip_serializing_if = "Vec::is_empty")]
    pub order_hash_in: Vec<SgBytes>,
}

#[derive(cynic::InputObject, Debug, Clone, Tsify, Default)]
#[cynic(graphql_type = "Vault_filter")]
pub struct SgTradeVaultTokenFilter {
    #[cynic(rename = "token_in", skip_serializing_if = "Vec::is_empty")]
    pub token_in: Vec<String>,
}

#[derive(cynic::InputObject, Debug, Clone, Tsify, Default)]
#[cynic(graphql_type = "TradeVaultBalanceChange_filter")]
pub struct SgTradeVaultBalanceChangeTokenFilter {
    #[cynic(rename = "vault_", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub vault_: Option<SgTradeVaultTokenFilter>,
}

#[derive(cynic::InputObject, Debug, Clone, Tsify, Default)]
#[cynic(graphql_type = "TradeEvent_filter")]
pub struct SgTradeEventFilter {
    #[cynic(rename = "sender_in", skip_serializing_if = "Vec::is_empty")]
    pub sender_in: Vec<SgBytes>,
}

#[derive(cynic::InputObject, Debug, Clone, Tsify, Default)]
#[cynic(graphql_type = "Trade_filter")]
pub struct SgTradesListQueryFilters {
    #[cynic(rename = "order_", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub order_: Option<SgTradeOrderFilter>,
    #[cynic(rename = "timestamp_gte", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub timestamp_gte: Option<SgBigInt>,
    #[cynic(rename = "timestamp_lte", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub timestamp_lte: Option<SgBigInt>,
    #[cynic(rename = "raindex_in", skip_serializing_if = "Vec::is_empty")]
    pub raindex_in: Vec<String>,
    #[cynic(
        rename = "inputVaultBalanceChange_",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub input_vault_balance_change_: Option<SgTradeVaultBalanceChangeTokenFilter>,
    #[cynic(
        rename = "outputVaultBalanceChange_",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub output_vault_balance_change_: Option<SgTradeVaultBalanceChangeTokenFilter>,
    #[cynic(rename = "tradeEvent_", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub trade_event_: Option<SgTradeEventFilter>,
    #[cynic(rename = "or", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub or: Option<Vec<SgTradesListQueryFilters>>,
}

#[derive(cynic::QueryFragment, Debug, Serialize, Clone, Tsify)]
#[cynic(graphql_type = "Raindex")]
pub struct SgRaindex {
    pub id: SgBytes,
}

#[cfg_attr(target_family = "wasm", tsify::declare)]
pub type SgRainMetaV1 = SgBytes;

#[derive(cynic::QueryFragment, Debug, Serialize, Clone, Tsify)]
#[serde(rename_all = "camelCase")]
#[cynic(graphql_type = "Order")]
pub struct SgOrder {
    pub id: SgBytes,
    pub order_bytes: SgBytes,
    pub order_hash: SgBytes,
    pub owner: SgBytes,
    pub outputs: Vec<SgVault>,
    pub inputs: Vec<SgVault>,
    pub raindex: SgRaindex,
    pub active: bool,
    pub timestamp_added: SgBigInt,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub meta: Option<SgRainMetaV1>,
    pub add_events: Vec<SgAddOrder>,
    pub trades: Vec<SgOrderStructPartialTrade>,
    pub remove_events: Vec<SgRemoveOrder>,
}
impl_wasm_traits!(SgOrder);

#[derive(Debug, Serialize, Deserialize, Clone, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct SgOrderWithSubgraphName {
    pub order: SgOrder,
    pub subgraph_name: String,
}
impl_wasm_traits!(SgOrderWithSubgraphName);

#[derive(Debug, Serialize, Deserialize, Clone, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct SgTradeWithSubgraphName {
    pub trade: SgTrade,
    pub subgraph_name: String,
}

#[derive(cynic::QueryFragment, Debug, Serialize, Clone, Tsify)]
#[cynic(graphql_type = "Order")]
#[serde(rename_all = "camelCase")]
pub struct SgTradeStructPartialOrder {
    pub id: SgBytes,
    pub order_hash: SgBytes,
    pub owner: SgBytes,
}

#[derive(cynic::QueryFragment, Debug, Serialize, Clone, Tsify)]
#[cynic(graphql_type = "Order")]
#[serde(rename_all = "camelCase")]
pub struct SgOrderAsIO {
    pub id: SgBytes,
    pub order_hash: SgBytes,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct SgVaultsListFilterArgs {
    pub owners: Vec<SgBytes>,
    pub hide_zero_balance: bool,
    pub tokens: Vec<String>,
    pub raindexes: Vec<String>,
    pub only_active_orders: bool,
}
impl_wasm_traits!(SgVaultsListFilterArgs);

#[derive(cynic::InputObject, Debug, Clone, Tsify, Default)]
#[cynic(graphql_type = "Vault_filter")]
pub struct SgVaultsListQueryFilters {
    #[cynic(rename = "owner_in", skip_serializing_if = "Vec::is_empty")]
    pub owner_in: Vec<SgBytes>,
    #[cynic(rename = "balance_not", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub balance_not: Option<SgBytes>,
    #[cynic(rename = "token_in", skip_serializing_if = "Vec::is_empty")]
    pub token_in: Vec<String>,
    #[cynic(rename = "raindex_in", skip_serializing_if = "Vec::is_empty")]
    pub raindex_in: Vec<String>,
    #[cynic(rename = "ordersAsInput_", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub orders_as_input_: Option<Box<SgOrdersListQueryFilters>>,
    #[cynic(rename = "ordersAsOutput_", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub orders_as_output_: Option<Box<SgOrdersListQueryFilters>>,
    #[cynic(rename = "or", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub or: Option<Vec<SgVaultsListQueryFilters>>,
}

#[derive(cynic::QueryVariables, Debug, Clone, Tsify)]
pub struct SgVaultsListQueryVariables {
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub first: Option<i32>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub skip: Option<i32>,
    #[cynic(rename = "filters")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub filters: Option<SgVaultsListQueryFilters>,
}

#[derive(cynic::QueryFragment, Debug, Serialize, Clone, Tsify)]
#[serde(rename_all = "camelCase")]
#[cynic(graphql_type = "Vault")]
pub struct SgVault {
    pub id: SgBytes,
    pub owner: SgBytes,
    pub vault_id: SgBytes,
    pub balance: SgBytes,
    pub token: SgErc20,
    pub raindex: SgRaindex,
    // latest orders
    #[arguments(orderBy: timestampAdded, orderDirection: desc)]
    pub orders_as_output: Vec<SgOrderAsIO>,
    // latest orders
    #[arguments(orderBy: timestampAdded, orderDirection: desc)]
    pub orders_as_input: Vec<SgOrderAsIO>,
    pub balance_changes: Vec<SgVaultBalanceChangeType>,
}
impl_wasm_traits!(SgVault);

#[derive(Debug, Serialize, Deserialize, Clone, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct SgVaultWithSubgraphName {
    pub vault: SgVault,
    pub subgraph_name: String,
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize, Tsify)]
#[cynic(graphql_type = "Vault")]
#[serde(rename_all = "camelCase")]
pub struct SgVaultBalanceChangeVault {
    pub id: SgBytes,
    pub vault_id: SgBytes,
    pub token: SgErc20,
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize, Tsify)]
#[cynic(graphql_type = "VaultBalanceChange")]
#[serde(rename_all = "camelCase")]
pub struct SgVaultBalanceChangeUnwrapped {
    #[serde(rename = "__typename")]
    pub __typename: String,
    pub amount: SgBytes,
    pub new_vault_balance: SgBytes,
    pub old_vault_balance: SgBytes,
    pub vault: SgVaultBalanceChangeVault,
    pub timestamp: SgBigInt,
    pub transaction: SgTransaction,
    pub raindex: SgRaindex,
}

#[derive(cynic::InlineFragments, Debug, Clone, Serialize, Tsify)]
#[serde(tag = "__typename", content = "data")]
#[serde(rename_all = "camelCase")]
#[cynic(graphql_type = "VaultBalanceChange")]
pub enum SgVaultBalanceChangeType {
    Withdrawal(SgWithdrawal),
    TradeVaultBalanceChange(SgTradeVaultBalanceChange),
    Deposit(SgDeposit),
    ClearBounty(SgClearBounty),
    #[cynic(fallback)]
    Unknown,
}

impl SgVaultBalanceChangeType {
    pub fn typename(&self) -> &str {
        match self {
            SgVaultBalanceChangeType::Withdrawal(w) => &w.__typename,
            SgVaultBalanceChangeType::TradeVaultBalanceChange(t) => &t.__typename,
            SgVaultBalanceChangeType::Deposit(d) => &d.__typename,
            SgVaultBalanceChangeType::ClearBounty(c) => &c.__typename,
            SgVaultBalanceChangeType::Unknown => "Unknown",
        }
    }

    pub fn timestamp(&self) -> Option<&SgBigInt> {
        match self {
            SgVaultBalanceChangeType::Withdrawal(w) => Some(&w.timestamp),
            SgVaultBalanceChangeType::TradeVaultBalanceChange(t) => Some(&t.timestamp),
            SgVaultBalanceChangeType::Deposit(d) => Some(&d.timestamp),
            SgVaultBalanceChangeType::ClearBounty(c) => Some(&c.timestamp),
            SgVaultBalanceChangeType::Unknown => None,
        }
    }

    pub fn amount(&self) -> Option<&SgBytes> {
        match self {
            SgVaultBalanceChangeType::Withdrawal(w) => Some(&w.amount),
            SgVaultBalanceChangeType::TradeVaultBalanceChange(t) => Some(&t.amount),
            SgVaultBalanceChangeType::Deposit(d) => Some(&d.amount),
            SgVaultBalanceChangeType::ClearBounty(c) => Some(&c.amount),
            SgVaultBalanceChangeType::Unknown => None,
        }
    }

    pub fn new_vault_balance(&self) -> Option<&SgBytes> {
        match self {
            SgVaultBalanceChangeType::Withdrawal(w) => Some(&w.new_vault_balance),
            SgVaultBalanceChangeType::TradeVaultBalanceChange(t) => Some(&t.new_vault_balance),
            SgVaultBalanceChangeType::Deposit(d) => Some(&d.new_vault_balance),
            SgVaultBalanceChangeType::ClearBounty(c) => Some(&c.new_vault_balance),
            SgVaultBalanceChangeType::Unknown => None,
        }
    }

    pub fn transaction(&self) -> Option<&SgTransaction> {
        match self {
            SgVaultBalanceChangeType::Withdrawal(w) => Some(&w.transaction),
            SgVaultBalanceChangeType::TradeVaultBalanceChange(t) => Some(&t.transaction),
            SgVaultBalanceChangeType::Deposit(d) => Some(&d.transaction),
            SgVaultBalanceChangeType::ClearBounty(c) => Some(&c.transaction),
            SgVaultBalanceChangeType::Unknown => None,
        }
    }
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[cynic(graphql_type = "Deposit")]
pub struct SgDeposit {
    pub id: SgBytes,
    #[serde(rename = "__typename")]
    pub __typename: String,
    pub amount: SgBytes,
    pub new_vault_balance: SgBytes,
    pub old_vault_balance: SgBytes,
    pub vault: SgVaultBalanceChangeVault,
    pub timestamp: SgBigInt,
    pub transaction: SgTransaction,
    pub raindex: SgRaindex,
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[cynic(graphql_type = "Withdrawal")]
pub struct SgWithdrawal {
    pub id: SgBytes,
    #[serde(rename = "__typename")]
    pub __typename: String,
    pub amount: SgBytes,
    pub new_vault_balance: SgBytes,
    pub old_vault_balance: SgBytes,
    pub vault: SgVaultBalanceChangeVault,
    pub timestamp: SgBigInt,
    pub transaction: SgTransaction,
    pub raindex: SgRaindex,
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[cynic(graphql_type = "TradeVaultBalanceChange")]
pub struct SgTradeVaultBalanceChange {
    pub id: SgBytes,
    #[serde(rename = "__typename")]
    pub __typename: String,
    pub amount: SgBytes,
    pub new_vault_balance: SgBytes,
    pub old_vault_balance: SgBytes,
    pub vault: SgVaultBalanceChangeVault,
    pub timestamp: SgBigInt,
    pub transaction: SgTransaction,
    pub raindex: SgRaindex,
    pub trade: SgTradeRef,
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[cynic(graphql_type = "ClearBounty")]
pub struct SgClearBounty {
    pub id: SgBytes,
    #[serde(rename = "__typename")]
    pub __typename: String,
    pub amount: SgBytes,
    pub new_vault_balance: SgBytes,
    pub old_vault_balance: SgBytes,
    pub vault: SgVaultBalanceChangeVault,
    pub timestamp: SgBigInt,
    pub transaction: SgTransaction,
    pub raindex: SgRaindex,
    pub sender: SgBytes,
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize, Tsify)]
#[cynic(graphql_type = "TradeEvent")]
pub struct SgTradeEvent {
    pub id: SgBytes,
    #[serde(rename = "__typename")]
    pub __typename: String,
    pub transaction: SgTransaction,
    pub sender: SgBytes,
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[cynic(graphql_type = "TradeEvent")]
pub struct SgTradeEventTypename {
    #[serde(rename = "__typename")]
    pub __typename: String,
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[cynic(graphql_type = "Trade")]
pub struct SgTradeRef {
    pub trade_event: SgTradeEventTypename,
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[cynic(graphql_type = "Trade")]
pub struct SgTrade {
    pub id: SgBytes,
    pub trade_event: SgTradeEvent,
    pub output_vault_balance_change: SgTradeVaultBalanceChange,
    pub order: SgTradeStructPartialOrder,
    pub input_vault_balance_change: SgTradeVaultBalanceChange,
    pub timestamp: SgBigInt,
    pub raindex: SgRaindex,
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize, Tsify)]
#[cynic(graphql_type = "Trade")]
pub struct SgOrderStructPartialTrade {
    pub id: SgBytes,
}

#[cfg_attr(target_family = "wasm", tsify::declare)]
pub type SgTokenAddress = SgBytes;

#[derive(cynic::QueryFragment, Debug, Serialize, Clone, PartialEq, Eq, Hash, Tsify)]
#[cynic(graphql_type = "ERC20")]
pub struct SgErc20 {
    pub id: SgBytes,
    pub address: SgTokenAddress,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub name: Option<String>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub symbol: Option<String>,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub decimals: Option<SgBigInt>,
}
#[derive(Debug, Serialize, Deserialize, Clone, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct SgErc20WithSubgraphName {
    pub token: SgErc20,
    pub subgraph_name: String,
}

#[derive(cynic::QueryFragment, Debug, Serialize, Clone, Tsify)]
#[serde(rename_all = "camelCase")]
#[cynic(graphql_type = "Transaction")]
pub struct SgTransaction {
    pub id: SgBytes,
    pub from: SgBytes,
    pub block_number: SgBigInt,
    pub timestamp: SgBigInt,
}

#[derive(cynic::QueryFragment, Debug, Serialize, Clone, Tsify)]
#[cynic(graphql_type = "AddOrder")]
pub struct SgAddOrder {
    pub transaction: SgTransaction,
}
impl_wasm_traits!(SgAddOrder);

#[derive(cynic::QueryFragment, Debug, Serialize, Clone, Tsify)]
#[cynic(graphql_type = "RemoveOrder")]
pub struct SgRemoveOrder {
    pub transaction: SgTransaction,
}
impl_wasm_traits!(SgRemoveOrder);

#[derive(cynic::QueryFragment, Debug, Serialize, Clone, Tsify)]
#[cynic(graphql_type = "AddOrder")]
pub struct SgAddOrderWithOrder {
    pub transaction: SgTransaction,
    pub order: SgOrder,
}
impl_wasm_traits!(SgAddOrderWithOrder);

#[derive(cynic::QueryFragment, Debug, Serialize, Clone, Tsify)]
#[cynic(graphql_type = "RemoveOrder")]
pub struct SgRemoveOrderWithOrder {
    pub transaction: SgTransaction,
    pub order: SgOrder,
}

#[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq, Hash, Tsify)]
#[cynic(graphql_type = "BigInt")]
pub struct SgBigInt(pub String);

#[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq, Hash, Tsify)]
#[cynic(graphql_type = "Bytes")]
pub struct SgBytes(pub String);

#[derive(cynic::Enum, Clone, Copy, Debug)]
#[cfg_attr(target_family = "wasm", derive(Tsify), tsify(namespace))]
#[cynic(graphql_type = "OrderDirection")]
pub enum SgOrderDirection {
    #[cynic(rename = "asc")]
    #[cfg_attr(target_family = "wasm", serde(rename = "asc"))]
    Asc,
    #[cynic(rename = "desc")]
    #[cfg_attr(target_family = "wasm", serde(rename = "desc"))]
    Desc,
}

#[derive(cynic::Enum, Clone, Copy, Debug)]
#[cynic(graphql_type = "Order_orderBy")]
#[cfg_attr(target_family = "wasm", derive(Tsify), tsify(namespace))]
pub enum SgOrderOrderBy {
    #[cynic(rename = "id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "id"))]
    Id,
    #[cynic(rename = "raindex")]
    #[cfg_attr(target_family = "wasm", serde(rename = "raindex"))]
    Raindex,
    #[cynic(rename = "raindex__id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "raindex__id"))]
    RaindexId,
    #[cynic(rename = "active")]
    #[cfg_attr(target_family = "wasm", serde(rename = "active"))]
    Active,
    #[cynic(rename = "orderHash")]
    #[cfg_attr(target_family = "wasm", serde(rename = "orderHash"))]
    OrderHash,
    #[cynic(rename = "owner")]
    #[cfg_attr(target_family = "wasm", serde(rename = "owner"))]
    Owner,
    #[cynic(rename = "inputs")]
    #[cfg_attr(target_family = "wasm", serde(rename = "inputs"))]
    Inputs,
    #[cynic(rename = "outputs")]
    #[cfg_attr(target_family = "wasm", serde(rename = "outputs"))]
    Outputs,
    #[cynic(rename = "nonce")]
    #[cfg_attr(target_family = "wasm", serde(rename = "nonce"))]
    Nonce,
    #[cynic(rename = "orderBytes")]
    #[cfg_attr(target_family = "wasm", serde(rename = "orderBytes"))]
    OrderBytes,
    #[cynic(rename = "addEvents")]
    #[cfg_attr(target_family = "wasm", serde(rename = "addEvents"))]
    AddEvents,
    #[cynic(rename = "removeEvents")]
    #[cfg_attr(target_family = "wasm", serde(rename = "removeEvents"))]
    RemoveEvents,
    #[cynic(rename = "trades")]
    #[cfg_attr(target_family = "wasm", serde(rename = "trades"))]
    Trades,
    #[cynic(rename = "meta")]
    #[cfg_attr(target_family = "wasm", serde(rename = "meta"))]
    Meta,
    #[cynic(rename = "timestampAdded")]
    #[cfg_attr(target_family = "wasm", serde(rename = "timestampAdded"))]
    TimestampAdded,
}

#[derive(cynic::Enum, Clone, Copy, Debug)]
#[cynic(graphql_type = "AddOrder_orderBy")]
#[cfg_attr(target_family = "wasm", derive(Tsify), tsify(namespace))]
pub enum SgAddOrderOrderBy {
    #[cynic(rename = "id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "id"))]
    Id,
    #[cynic(rename = "order")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order"))]
    Order,
    #[cynic(rename = "order__id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__id"))]
    OrderId,
    #[cynic(rename = "order__active")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__active"))]
    OrderActive,
    #[cynic(rename = "order__orderHash")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__orderHash"))]
    OrderOrderHash,
    #[cynic(rename = "order__owner")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__owner"))]
    OrderOwner,
    #[cynic(rename = "order__nonce")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__nonce"))]
    OrderNonce,
    #[cynic(rename = "order__orderBytes")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__orderBytes"))]
    OrderOrderBytes,
    #[cynic(rename = "order__meta")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__meta"))]
    OrderMeta,
    #[cynic(rename = "order__timestampAdded")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__timestampAdded"))]
    OrderTimestampAdded,
    #[cynic(rename = "raindex")]
    #[cfg_attr(target_family = "wasm", serde(rename = "raindex"))]
    Raindex,
    #[cynic(rename = "raindex__id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "raindex__id"))]
    RaindexId,
    #[cynic(rename = "transaction")]
    #[cfg_attr(target_family = "wasm", serde(rename = "transaction"))]
    Transaction,
    #[cynic(rename = "transaction__id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "transaction__id"))]
    TransactionId,
    #[cynic(rename = "transaction__timestamp")]
    #[cfg_attr(target_family = "wasm", serde(rename = "transaction__timestamp"))]
    TransactionTimestamp,
    #[cynic(rename = "transaction__blockNumber")]
    #[cfg_attr(target_family = "wasm", serde(rename = "transaction__blockNumber"))]
    TransactionBlockNumber,
    #[cynic(rename = "transaction__from")]
    #[cfg_attr(target_family = "wasm", serde(rename = "transaction__from"))]
    TransactionFrom,
    #[cynic(rename = "sender")]
    #[cfg_attr(target_family = "wasm", serde(rename = "sender"))]
    Sender,
}

#[derive(cynic::Enum, Clone, Copy, Debug)]
#[cynic(graphql_type = "Trade_orderBy")]
#[cfg_attr(target_family = "wasm", derive(Tsify), tsify(namespace))]
pub enum SgTradeOrderBy {
    #[cynic(rename = "id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "id"))]
    Id,
    #[cynic(rename = "raindex")]
    #[cfg_attr(target_family = "wasm", serde(rename = "raindex"))]
    Raindex,
    #[cynic(rename = "raindex__id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "raindex__id"))]
    RaindexId,
    #[cynic(rename = "order")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order"))]
    Order,
    #[cynic(rename = "order__id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__id"))]
    OrderId,
    #[cynic(rename = "order__active")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__active"))]
    OrderActive,
    #[cynic(rename = "order__orderHash")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__orderHash"))]
    OrderOrderHash,
    #[cynic(rename = "order__owner")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__owner"))]
    OrderOwner,
    #[cynic(rename = "order__nonce")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__nonce"))]
    OrderNonce,
    #[cynic(rename = "order__orderBytes")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__orderBytes"))]
    OrderOrderBytes,
    #[cynic(rename = "order__meta")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__meta"))]
    OrderMeta,
    #[cynic(rename = "order__timestampAdded")]
    #[cfg_attr(target_family = "wasm", serde(rename = "order__timestampAdded"))]
    OrderTimestampAdded,
    #[cynic(rename = "inputVaultBalanceChange")]
    #[cfg_attr(target_family = "wasm", serde(rename = "inputVaultBalanceChange"))]
    InputVaultBalanceChange,
    #[cynic(rename = "inputVaultBalanceChange__id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "inputVaultBalanceChange__id"))]
    InputVaultBalanceChangeId,
    #[cynic(rename = "inputVaultBalanceChange__amount")]
    #[cfg_attr(
        target_family = "wasm",
        serde(rename = "inputVaultBalanceChange__amount")
    )]
    InputVaultBalanceChangeAmount,
    #[cynic(rename = "inputVaultBalanceChange__oldVaultBalance")]
    #[cfg_attr(
        target_family = "wasm",
        serde(rename = "inputVaultBalanceChange__oldVaultBalance")
    )]
    InputVaultBalanceChangeOldVaultBalance,
    #[cynic(rename = "inputVaultBalanceChange__newVaultBalance")]
    #[cfg_attr(
        target_family = "wasm",
        serde(rename = "inputVaultBalanceChange__newVaultBalance")
    )]
    InputVaultBalanceChangeNewVaultBalance,
    #[cynic(rename = "inputVaultBalanceChange__timestamp")]
    #[cfg_attr(
        target_family = "wasm",
        serde(rename = "inputVaultBalanceChange__timestamp")
    )]
    InputVaultBalanceChangeTimestamp,
    #[cynic(rename = "outputVaultBalanceChange")]
    #[cfg_attr(target_family = "wasm", serde(rename = "outputVaultBalanceChange"))]
    OutputVaultBalanceChange,
    #[cynic(rename = "outputVaultBalanceChange__id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "outputVaultBalanceChange__id"))]
    OutputVaultBalanceChangeId,
    #[cynic(rename = "outputVaultBalanceChange__amount")]
    #[cfg_attr(
        target_family = "wasm",
        serde(rename = "outputVaultBalanceChange__amount")
    )]
    OutputVaultBalanceChangeAmount,
    #[cynic(rename = "outputVaultBalanceChange__oldVaultBalance")]
    #[cfg_attr(
        target_family = "wasm",
        serde(rename = "outputVaultBalanceChange__oldVaultBalance")
    )]
    OutputVaultBalanceChangeOldVaultBalance,
    #[cynic(rename = "outputVaultBalanceChange__newVaultBalance")]
    #[cfg_attr(
        target_family = "wasm",
        serde(rename = "outputVaultBalanceChange__newVaultBalance")
    )]
    OutputVaultBalanceChangeNewVaultBalance,
    #[cynic(rename = "outputVaultBalanceChange__timestamp")]
    #[cfg_attr(
        target_family = "wasm",
        serde(rename = "outputVaultBalanceChange__timestamp")
    )]
    OutputVaultBalanceChangeTimestamp,
    #[cynic(rename = "tradeEvent")]
    #[cfg_attr(target_family = "wasm", serde(rename = "tradeEvent"))]
    TradeEvent,
    #[cynic(rename = "tradeEvent__id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "tradeEvent__id"))]
    TradeEventId,
    #[cynic(rename = "tradeEvent__sender")]
    #[cfg_attr(target_family = "wasm", serde(rename = "tradeEvent__sender"))]
    TradeEventSender,
    #[cynic(rename = "timestamp")]
    #[cfg_attr(target_family = "wasm", serde(rename = "timestamp"))]
    Timestamp,
}

#[derive(cynic::Enum, Clone, Copy, Debug)]
#[cynic(graphql_type = "Vault_orderBy")]
#[cfg_attr(target_family = "wasm", derive(Tsify), tsify(namespace))]
pub enum SgVaultOrderBy {
    #[cynic(rename = "id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "id"))]
    Id,
    #[cynic(rename = "raindex")]
    #[cfg_attr(target_family = "wasm", serde(rename = "raindex"))]
    Raindex,
    #[cynic(rename = "raindex__id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "raindex__id"))]
    RaindexId,
    #[cynic(rename = "token")]
    #[cfg_attr(target_family = "wasm", serde(rename = "token"))]
    Token,
    #[cynic(rename = "token__id")]
    #[cfg_attr(target_family = "wasm", serde(rename = "token__id"))]
    TokenId,
    #[cynic(rename = "token__address")]
    #[cfg_attr(target_family = "wasm", serde(rename = "token__address"))]
    TokenAddress,
    #[cynic(rename = "token__name")]
    #[cfg_attr(target_family = "wasm", serde(rename = "token__name"))]
    TokenName,
    #[cynic(rename = "token__symbol")]
    #[cfg_attr(target_family = "wasm", serde(rename = "token__symbol"))]
    TokenSymbol,
    #[cynic(rename = "token__decimals")]
    #[cfg_attr(target_family = "wasm", serde(rename = "token__decimals"))]
    TokenDecimals,
    #[cynic(rename = "owner")]
    #[cfg_attr(target_family = "wasm", serde(rename = "owner"))]
    Owner,
    #[cynic(rename = "vaultId")]
    #[cfg_attr(target_family = "wasm", serde(rename = "vaultId"))]
    VaultId,
    #[cynic(rename = "ordersAsInput")]
    #[cfg_attr(target_family = "wasm", serde(rename = "ordersAsInput"))]
    OrdersAsInput,
    #[cynic(rename = "ordersAsOutput")]
    #[cfg_attr(target_family = "wasm", serde(rename = "ordersAsOutput"))]
    OrdersAsOutput,
    #[cynic(rename = "balance")]
    #[cfg_attr(target_family = "wasm", serde(rename = "balance"))]
    Balance,
    #[cynic(rename = "balanceChanges")]
    #[cfg_attr(target_family = "wasm", serde(rename = "balanceChanges"))]
    BalanceChanges,
}

#[derive(cynic::QueryFragment, Debug, Clone, Serialize)]
#[cynic(graphql_type = "Query")]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
pub struct SgTokensListAllQuery {
    #[cynic(rename = "erc20S")]
    pub tokens: Vec<SgErc20>,
}

#[cfg(target_family = "wasm")]
mod impls {
    use super::*;

    impl_wasm_traits!(SgOrderAsIO);
    impl_wasm_traits!(SgVaultBalanceChangeVault);
    impl_wasm_traits!(SgVaultBalanceChangeType);
    impl_wasm_traits!(SgWithdrawal);
    impl_wasm_traits!(SgTradeVaultBalanceChange);
    impl_wasm_traits!(SgDeposit);
    impl_wasm_traits!(SgClearBounty);
    impl_wasm_traits!(SgOrderStructPartialTrade);
    impl_wasm_traits!(SgErc20);
    impl_wasm_traits!(SgTransaction);
    impl_wasm_traits!(SgBigInt);
    impl_wasm_traits!(SgBytes);
    impl_wasm_traits!(SgTrade);
    impl_wasm_traits!(SgTradeStructPartialOrder);
    impl_wasm_traits!(SgTradeEvent);
    impl_wasm_traits!(SgTradeEventTypename);
    impl_wasm_traits!(SgTradeRef);
}

#[cfg(test)]
mod sg_vault_balance_change_type_tests {
    use super::*;

    fn vault_ref() -> SgVaultBalanceChangeVault {
        SgVaultBalanceChangeVault {
            id: SgBytes("0xvault".to_string()),
            vault_id: SgBytes("vault-id".to_string()),
            token: SgErc20 {
                id: SgBytes("0xtoken".to_string()),
                address: SgBytes("0xtoken".to_string()),
                name: Some("Token".to_string()),
                symbol: Some("TKN".to_string()),
                decimals: Some(SgBigInt("18".to_string())),
            },
        }
    }

    fn raindex_ref() -> SgRaindex {
        SgRaindex {
            id: SgBytes("0xraindex".to_string()),
        }
    }

    // Each variant is built so that, within a variant, every accessor-exposed
    // field carries a DISTINCT value (so an arm that returns the wrong field is
    // caught), and so that the same accessor returns a DIFFERENT value across
    // variants (so an arm that delegates to the wrong variant's struct is
    // caught). The prefix encodes the variant; the suffix encodes the field.
    fn transaction_for(prefix: &str) -> SgTransaction {
        SgTransaction {
            id: SgBytes(format!("{prefix}-tx-id")),
            from: SgBytes(format!("{prefix}-tx-from")),
            block_number: SgBigInt(format!("{prefix}-tx-block")),
            timestamp: SgBigInt(format!("{prefix}-tx-ts")),
        }
    }

    fn withdrawal() -> SgVaultBalanceChangeType {
        SgVaultBalanceChangeType::Withdrawal(SgWithdrawal {
            id: SgBytes("withdrawal-id".to_string()),
            __typename: "Withdrawal".to_string(),
            amount: SgBytes("withdrawal-amount".to_string()),
            new_vault_balance: SgBytes("withdrawal-newbal".to_string()),
            old_vault_balance: SgBytes("withdrawal-oldbal".to_string()),
            vault: vault_ref(),
            timestamp: SgBigInt("withdrawal-ts".to_string()),
            transaction: transaction_for("withdrawal"),
            raindex: raindex_ref(),
        })
    }

    fn trade_change() -> SgVaultBalanceChangeType {
        SgVaultBalanceChangeType::TradeVaultBalanceChange(SgTradeVaultBalanceChange {
            id: SgBytes("trade-id".to_string()),
            __typename: "TradeVaultBalanceChange".to_string(),
            amount: SgBytes("trade-amount".to_string()),
            new_vault_balance: SgBytes("trade-newbal".to_string()),
            old_vault_balance: SgBytes("trade-oldbal".to_string()),
            vault: vault_ref(),
            timestamp: SgBigInt("trade-ts".to_string()),
            transaction: transaction_for("trade"),
            raindex: raindex_ref(),
            trade: SgTradeRef {
                trade_event: SgTradeEventTypename {
                    __typename: "TradeEvent".to_string(),
                },
            },
        })
    }

    fn deposit() -> SgVaultBalanceChangeType {
        SgVaultBalanceChangeType::Deposit(SgDeposit {
            id: SgBytes("deposit-id".to_string()),
            __typename: "Deposit".to_string(),
            amount: SgBytes("deposit-amount".to_string()),
            new_vault_balance: SgBytes("deposit-newbal".to_string()),
            old_vault_balance: SgBytes("deposit-oldbal".to_string()),
            vault: vault_ref(),
            timestamp: SgBigInt("deposit-ts".to_string()),
            transaction: transaction_for("deposit"),
            raindex: raindex_ref(),
        })
    }

    fn clear_bounty() -> SgVaultBalanceChangeType {
        SgVaultBalanceChangeType::ClearBounty(SgClearBounty {
            id: SgBytes("clearbounty-id".to_string()),
            __typename: "ClearBounty".to_string(),
            amount: SgBytes("clearbounty-amount".to_string()),
            new_vault_balance: SgBytes("clearbounty-newbal".to_string()),
            old_vault_balance: SgBytes("clearbounty-oldbal".to_string()),
            vault: vault_ref(),
            timestamp: SgBigInt("clearbounty-ts".to_string()),
            transaction: transaction_for("clearbounty"),
            raindex: raindex_ref(),
            sender: SgBytes("clearbounty-sender".to_string()),
        })
    }

    #[test]
    fn test_typename_all_variants() {
        // The __typename field is populated independently of the variant name,
        // so this also pins each arm to its own variant's stored field.
        assert_eq!(withdrawal().typename(), "Withdrawal");
        assert_eq!(trade_change().typename(), "TradeVaultBalanceChange");
        assert_eq!(deposit().typename(), "Deposit");
        assert_eq!(clear_bounty().typename(), "ClearBounty");
        // Unknown is a unit variant: the string is a literal, not a field.
        assert_eq!(SgVaultBalanceChangeType::Unknown.typename(), "Unknown");
    }

    #[test]
    fn test_timestamp_all_variants() {
        assert_eq!(withdrawal().timestamp().unwrap().0, "withdrawal-ts");
        assert_eq!(trade_change().timestamp().unwrap().0, "trade-ts");
        assert_eq!(deposit().timestamp().unwrap().0, "deposit-ts");
        assert_eq!(clear_bounty().timestamp().unwrap().0, "clearbounty-ts");
        assert!(SgVaultBalanceChangeType::Unknown.timestamp().is_none());
    }

    #[test]
    fn test_amount_all_variants() {
        assert_eq!(withdrawal().amount().unwrap().0, "withdrawal-amount");
        assert_eq!(trade_change().amount().unwrap().0, "trade-amount");
        assert_eq!(deposit().amount().unwrap().0, "deposit-amount");
        assert_eq!(clear_bounty().amount().unwrap().0, "clearbounty-amount");
        assert!(SgVaultBalanceChangeType::Unknown.amount().is_none());
    }

    #[test]
    fn test_new_vault_balance_all_variants() {
        assert_eq!(
            withdrawal().new_vault_balance().unwrap().0,
            "withdrawal-newbal"
        );
        assert_eq!(
            trade_change().new_vault_balance().unwrap().0,
            "trade-newbal"
        );
        assert_eq!(deposit().new_vault_balance().unwrap().0, "deposit-newbal");
        assert_eq!(
            clear_bounty().new_vault_balance().unwrap().0,
            "clearbounty-newbal"
        );
        assert!(SgVaultBalanceChangeType::Unknown
            .new_vault_balance()
            .is_none());
    }

    #[test]
    fn test_transaction_all_variants() {
        assert_eq!(withdrawal().transaction().unwrap().id.0, "withdrawal-tx-id");
        assert_eq!(trade_change().transaction().unwrap().id.0, "trade-tx-id");
        assert_eq!(deposit().transaction().unwrap().id.0, "deposit-tx-id");
        assert_eq!(
            clear_bounty().transaction().unwrap().id.0,
            "clearbounty-tx-id"
        );
        assert!(SgVaultBalanceChangeType::Unknown.transaction().is_none());
    }
}
