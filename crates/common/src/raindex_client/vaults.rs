use super::ClientRef;
use super::*;
use crate::local_db::query::fetch_order_vaults_volume::LocalDbVaultVolume;
use crate::local_db::query::fetch_vaults::LocalDbVault;
use crate::local_db::{
    query::fetch_vault_balance_changes::LocalDbVaultBalanceChange, RaindexIdentifier,
};
use crate::raindex_client::local_db::vaults::LocalDbVaults;
use crate::raindex_client::QuerySource;
use crate::types::VaultBalanceChangeKind;
use crate::{
    allowance::read_allowance,
    deposit::DepositArgs,
    erc20::ERC20,
    raindex_client::{
        orders::RaindexOrderAsIO, transactions::RaindexTransaction, vaults_list::RaindexVaultsList,
    },
    transaction::TransactionArgs,
    withdraw::WithdrawArgs,
};
use alloy::sol_types::SolCall;
use alloy::{
    hex,
    primitives::{Address, Bytes, B256, U256},
};
use async_trait::async_trait;
use rain_math_float::Float;
use raindex_bindings::{IRaindexV7::deposit5Call, IERC20::approveCall};
use raindex_subgraph_client::{
    performance::vol::{VaultVolume, VolumeDetails},
    types::{
        common::{
            SgBigInt, SgBytes, SgErc20, SgOrderAsIO, SgRaindex, SgTradeVaultBalanceChange, SgVault,
            SgVaultBalanceChangeType, SgVaultBalanceChangeUnwrapped, SgVaultsListFilterArgs,
        },
        Id,
    },
    MultiRaindexSubgraphClient, RaindexSubgraphClient, RaindexSubgraphClientError,
    SgPaginationArgs,
};
use std::{collections::BTreeMap, str::FromStr};
use wasm_bindgen_utils::impl_wasm_traits;
#[cfg(target_family = "wasm")]
use wasm_bindgen_utils::prelude::js_sys::BigInt;

const DEFAULT_PAGE_SIZE: u16 = 100;

fn sort_vaults_for_pagination(vaults: &mut [RaindexVault]) {
    vaults.sort_by(|a, b| {
        a.chain_id
            .cmp(&b.chain_id)
            .then_with(|| a.raindex.cmp(&b.raindex))
            .then_with(|| a.owner.cmp(&b.owner))
            .then_with(|| a.token.address.cmp(&b.token.address))
            .then_with(|| a.vault_id.cmp(&b.vault_id))
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn page_vaults(vaults: Vec<RaindexVault>, page: u16, page_size: u16) -> Vec<RaindexVault> {
    let offset = ((page - 1) as usize) * page_size as usize;
    vaults
        .into_iter()
        .skip(offset)
        .take(page_size as usize)
        .collect()
}

fn add_vaults_to_totals(
    totals: &mut BTreeMap<(u32, Address), RaindexVaultTotal>,
    vaults: Vec<RaindexVault>,
    zero: Float,
) -> Result<(), RaindexError> {
    for vault in vaults {
        if !vault.balance.gt(zero)? {
            continue;
        }
        let key = (vault.chain_id, vault.token.address);
        let total = totals.entry(key).or_insert_with(|| RaindexVaultTotal {
            chain_id: vault.chain_id,
            token: vault.token.clone(),
            balance: zero,
            balance_hex: zero.as_hex(),
            formatted_balance: "0".to_string(),
        });
        total.balance = (total.balance + vault.balance)?;
        total.balance_hex = total.balance.as_hex();
        total.formatted_balance = total.balance.format()?;
    }

    Ok(())
}

pub(crate) struct SubgraphVaults<'a> {
    client: &'a RaindexClient,
}
impl<'a> SubgraphVaults<'a> {
    pub(crate) fn new(client: &'a RaindexClient) -> Self {
        Self { client }
    }
}

#[cfg_attr(target_family = "wasm", async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
pub(crate) trait VaultsDataSource {
    async fn list(
        &self,
        chain_ids: Option<Vec<u32>>,
        filters: &GetVaultsFilters,
        page: Option<u16>,
        page_size: Option<u16>,
    ) -> Result<Vec<RaindexVault>, RaindexError>;

    async fn count(
        &self,
        chain_ids: Option<Vec<u32>>,
        filters: &GetVaultsFilters,
    ) -> Result<u32, RaindexError>;

    async fn get_by_id(
        &self,
        raindex_id: &RaindexIdentifier,
        vault_id: &Bytes,
    ) -> Result<Option<RaindexVault>, RaindexError>;

    async fn balance_changes_list(
        &self,
        vault: &RaindexVault,
        page: Option<u16>,
        filter_types: Option<&[VaultBalanceChangeFilter]>,
    ) -> Result<Vec<RaindexVaultBalanceChange>, RaindexError>;

    async fn tokens_list(
        &self,
        chain_ids: Option<Vec<u32>>,
    ) -> Result<Vec<RaindexVaultToken>, RaindexError>;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct RaindexVaultsListResult {
    vaults: RaindexVaultsList,
    page: u16,
    page_size: u16,
    total_items: u32,
    has_more: bool,
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl RaindexVaultsListResult {
    #[wasm_bindgen(getter)]
    pub fn items(&self) -> Vec<RaindexVault> {
        self.vaults.items()
    }

    #[wasm_bindgen(getter, unchecked_return_type = "RaindexVaultsList")]
    pub fn vaults(&self) -> RaindexVaultsList {
        self.vaults.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn page(&self) -> u16 {
        self.page
    }

    #[wasm_bindgen(getter, js_name = "pageSize")]
    pub fn page_size(&self) -> u16 {
        self.page_size
    }

    #[wasm_bindgen(getter, js_name = "totalItems")]
    pub fn total_items(&self) -> u32 {
        self.total_items
    }

    #[wasm_bindgen(getter, js_name = "hasMore")]
    pub fn has_more(&self) -> bool {
        self.has_more
    }
}

#[cfg(not(target_family = "wasm"))]
impl RaindexVaultsListResult {
    pub fn items(&self) -> Vec<RaindexVault> {
        self.vaults.items()
    }

    pub fn vaults(&self) -> RaindexVaultsList {
        self.vaults.clone()
    }

    pub fn page(&self) -> u16 {
        self.page
    }

    pub fn page_size(&self) -> u16 {
        self.page_size
    }

    pub fn total_items(&self) -> u32 {
        self.total_items
    }

    pub fn has_more(&self) -> bool {
        self.has_more
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct RaindexVaultTotal {
    chain_id: u32,
    token: RaindexVaultToken,
    balance: Float,
    balance_hex: String,
    formatted_balance: String,
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl RaindexVaultTotal {
    #[wasm_bindgen(getter = chainId)]
    pub fn chain_id(&self) -> u32 {
        self.chain_id
    }

    #[wasm_bindgen(getter)]
    pub fn token(&self) -> RaindexVaultToken {
        self.token.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> Float {
        self.balance
    }

    #[wasm_bindgen(getter = balanceHex, unchecked_return_type = "Hex")]
    pub fn balance_hex(&self) -> String {
        self.balance_hex.clone()
    }

    #[wasm_bindgen(getter = formattedBalance)]
    pub fn formatted_balance(&self) -> String {
        self.formatted_balance.clone()
    }
}

#[cfg(not(target_family = "wasm"))]
impl RaindexVaultTotal {
    pub fn chain_id(&self) -> u32 {
        self.chain_id
    }

    pub fn token(&self) -> RaindexVaultToken {
        self.token.clone()
    }

    pub fn balance(&self) -> Float {
        self.balance
    }

    pub fn balance_hex(&self) -> String {
        self.balance_hex.clone()
    }

    pub fn formatted_balance(&self) -> String {
        self.formatted_balance.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum RaindexVaultType {
    Input,
    Output,
    InputOutput,
}
impl_wasm_traits!(RaindexVaultType);

/// Represents a vault with balance and token information within a given raindex.
///
/// A vault is a fundamental component that holds tokens and participates in order execution.
/// Each vault has a unique identifier, current balance, associated token metadata, and
/// belongs to a specific raindex contract on the blockchain.
///
/// Vaults can serve different roles in relation to orders - they can provide tokens (input),
/// receive tokens (output), or both (input/output), depending on the trading algorithm.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct RaindexVault {
    raindex_client: ClientRef,
    chain_id: u32,
    vault_type: Option<RaindexVaultType>,
    id: Bytes,
    owner: Address,
    vault_id: U256,
    balance: Float,
    formatted_balance: String,
    token: RaindexVaultToken,
    raindex: Address,
    orders_as_inputs: Vec<RaindexOrderAsIO>,
    orders_as_outputs: Vec<RaindexOrderAsIO>,
}

impl RaindexVault {
    pub(crate) fn vault_id_string(&self) -> String {
        self.vault_id.to_string()
    }
    /// The raw `vaultId` as a `U256`, available on every target (the public
    /// `vault_id` getter returns a `BigInt` on wasm and a `U256` off-wasm).
    pub(crate) fn raw_vault_id(&self) -> U256 {
        self.vault_id
    }
    /// The vault token's address, available on every target (the public
    /// `token().address()` getter returns a `String` on wasm).
    pub(crate) fn token_address(&self) -> Address {
        self.token.address
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl RaindexVault {
    fn u256_to_bigint(value: U256) -> Result<BigInt, RaindexError> {
        BigInt::from_str(&value.to_string())
            .map_err(|e| RaindexError::JsError(e.to_string().into()))
    }

    #[wasm_bindgen(getter = chainId)]
    pub fn chain_id(&self) -> u32 {
        self.chain_id
    }
    #[wasm_bindgen(getter = vaultType)]
    pub fn vault_type(&self) -> Option<RaindexVaultType> {
        self.vault_type.clone()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Hex")]
    pub fn id(&self) -> String {
        self.id.to_string()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Address")]
    pub fn owner(&self) -> String {
        self.owner.to_string()
    }
    pub(crate) fn vault_id_hex(&self) -> String {
        hex::encode_prefixed(self.vault_id.to_be_bytes::<32>())
    }
    #[wasm_bindgen(getter = vaultId)]
    pub fn vault_id(&self) -> Result<BigInt, RaindexError> {
        Self::u256_to_bigint(self.vault_id)
    }
    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> Float {
        self.balance
    }
    #[wasm_bindgen(getter = formattedBalance)]
    pub fn formatted_balance(&self) -> String {
        self.formatted_balance.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn token(&self) -> RaindexVaultToken {
        self.token.clone()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Address")]
    pub fn raindex(&self) -> String {
        self.raindex.to_string()
    }
    #[wasm_bindgen(getter = ordersAsInput)]
    pub fn orders_as_inputs(&self) -> Vec<RaindexOrderAsIO> {
        self.orders_as_inputs.clone()
    }
    #[wasm_bindgen(getter = ordersAsOutput)]
    pub fn orders_as_outputs(&self) -> Vec<RaindexOrderAsIO> {
        self.orders_as_outputs.clone()
    }
}

#[cfg(not(target_family = "wasm"))]
impl RaindexVault {
    pub fn chain_id(&self) -> u32 {
        self.chain_id
    }
    pub fn vault_type(&self) -> Option<RaindexVaultType> {
        self.vault_type.clone()
    }
    pub fn id(&self) -> Bytes {
        self.id.clone()
    }
    pub fn owner(&self) -> Address {
        self.owner
    }
    pub fn vault_id(&self) -> U256 {
        self.vault_id
    }
    pub fn balance(&self) -> Float {
        self.balance
    }
    pub fn formatted_balance(&self) -> String {
        self.formatted_balance.clone()
    }
    pub fn token(&self) -> RaindexVaultToken {
        self.token.clone()
    }
    pub fn raindex(&self) -> Address {
        self.raindex
    }
    pub fn orders_as_inputs(&self) -> Vec<RaindexOrderAsIO> {
        self.orders_as_inputs.clone()
    }
    pub fn orders_as_outputs(&self) -> Vec<RaindexOrderAsIO> {
        self.orders_as_outputs.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct AccountBalance {
    balance: Float,
    formatted_balance: String,
}
impl AccountBalance {
    pub fn new(balance: Float, formatted_balance: String) -> Self {
        Self {
            balance,
            formatted_balance,
        }
    }
}
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl AccountBalance {
    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> Float {
        self.balance
    }
    #[wasm_bindgen(getter = formattedBalance)]
    pub fn formatted_balance(&self) -> String {
        self.formatted_balance.clone()
    }
}
#[cfg(not(target_family = "wasm"))]
impl AccountBalance {
    pub fn balance(&self) -> Float {
        self.balance
    }
    pub fn formatted_balance(&self) -> String {
        self.formatted_balance.clone()
    }
}

/// Token metadata associated with a vault in the Raindex system.
///
/// Contains comprehensive information about the ERC20 token held within a vault,
/// including contract address, human-readable identifiers, and decimal precision.
/// Some fields may be optional as they depend on the token's implementation and
/// whether the metadata has been successfully retrieved from the blockchain.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct RaindexVaultToken {
    chain_id: u32,
    id: String,
    address: Address,
    name: Option<String>,
    symbol: Option<String>,
    decimals: u8,
}

impl RaindexVaultToken {
    pub(crate) fn raw_address(&self) -> Address {
        self.address
    }

    pub(crate) fn raw_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub(crate) fn raw_symbol(&self) -> Option<&str> {
        self.symbol.as_deref()
    }

    pub(crate) fn raw_decimals(&self) -> u8 {
        self.decimals
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl RaindexVaultToken {
    #[wasm_bindgen(getter = chainId)]
    pub fn chain_id(&self) -> u32 {
        self.chain_id
    }
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.id.clone()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Address")]
    pub fn address(&self) -> String {
        self.address.to_string()
    }
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> Option<String> {
        self.name.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn symbol(&self) -> Option<String> {
        self.symbol.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn decimals(&self) -> u8 {
        self.decimals
    }
}

#[cfg(not(target_family = "wasm"))]
impl RaindexVaultToken {
    pub fn chain_id(&self) -> u32 {
        self.chain_id
    }
    pub fn id(&self) -> String {
        self.id.clone()
    }
    pub fn address(&self) -> Address {
        self.address
    }
    pub fn name(&self) -> Option<String> {
        self.name.clone()
    }
    pub fn symbol(&self) -> Option<String> {
        self.symbol.clone()
    }
    pub fn decimals(&self) -> u8 {
        self.decimals
    }
}

#[wasm_export]
impl RaindexVault {
    #[wasm_export(skip)]
    pub fn get_raindex_subgraph_client(&self) -> Result<RaindexSubgraphClient, RaindexError> {
        self.raindex_client
            .get_raindex_subgraph_client(self.raindex)
    }

    /// Fetches balance change history for a vault
    ///
    /// Retrieves chronological list of deposits, withdrawals, and trades affecting
    /// a vault's balance. Optionally filter by balance change type.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// // Fetch all balance changes
    /// const result = await vault.getBalanceChanges();
    /// if (result.error) {
    ///   console.error("Error fetching history:", result.error.readableMsg);
    ///   return;
    /// }
    /// const changes = result.value;
    ///
    /// // Fetch only deposits and withdrawals
    /// const filteredResult = await vault.getBalanceChanges(1, ["deposit", "withdrawal"]);
    /// ```
    #[wasm_export(
        js_name = "getBalanceChanges",
        return_description = "Array of balance change events",
        unchecked_return_type = "RaindexVaultBalanceChange[]",
        preserve_js_class
    )]
    pub async fn get_balance_changes(
        &self,
        #[wasm_export(param_description = "Optional page number (default to 1)")] page: Option<u16>,
        #[wasm_export(
            param_description = "Optional filter types array (deposit, withdrawal, takeOrder, clear, clearBounty)"
        )]
        filter_types: Option<Vec<VaultBalanceChangeFilter>>,
    ) -> Result<Vec<RaindexVaultBalanceChange>, RaindexError> {
        match self.raindex_client.query_source(self.chain_id) {
            QuerySource::LocalDb(local_db) => {
                let local_source =
                    LocalDbVaults::new(&local_db, ClientRef::clone(&self.raindex_client));
                local_source
                    .balance_changes_list(self, page, filter_types.as_deref())
                    .await
            }
            QuerySource::Subgraph => {
                let subgraph_source = SubgraphVaults::new(&self.raindex_client);
                subgraph_source
                    .balance_changes_list(self, page, filter_types.as_deref())
                    .await
            }
        }
    }

    fn validate_amount(&self, amount: &Float) -> Result<(), RaindexError> {
        let zero_float = Float::parse("0".to_string())?;
        if amount.is_zero()? {
            return Err(RaindexError::ZeroAmount);
        }
        if amount.lt(zero_float)? {
            return Err(RaindexError::NegativeAmount);
        }
        Ok(())
    }

    /// Builds the [`DepositArgs`] for `amount` from this vault's deposit context
    /// (token, vault id, decimals). It deliberately takes no transaction context
    /// (raindex address, RPCs): constructing a deposit's arguments is independent
    /// of how the resulting transaction is submitted, so deposit callers don't
    /// have to construct a [`TransactionArgs`].
    fn get_deposit_args(&self, amount: &Float) -> DepositArgs {
        DepositArgs {
            token: self.token.address,
            vault_id: B256::from(self.vault_id),
            amount: *amount,
            decimals: self.token.decimals,
        }
    }

    /// Builds the [`TransactionArgs`] from this vault's transaction context
    /// (raindex address and chain RPCs). It deliberately takes no deposit context
    /// (amount, vault id, decimals): the transaction's arguments are independent
    /// of any deposit, so allowance/approval callers don't have to construct a
    /// [`DepositArgs`].
    fn get_transaction_args(&self) -> Result<TransactionArgs, RaindexError> {
        let rpcs = self.raindex_client.get_rpc_urls_for_chain(self.chain_id)?;

        Ok(TransactionArgs {
            raindex_address: self.raindex,
            rpcs: rpcs.iter().map(|rpc| rpc.to_string()).collect(),
            ..Default::default()
        })
    }

    /// Reads the current ERC20 allowance the raindex contract holds for this
    /// vault's owner and token. It needs only the vault's token, owner, raindex
    /// spender and RPCs (via [`Self::get_transaction_args`]) - no deposit context
    /// (amount, vault id, decimals).
    async fn read_allowance(&self) -> Result<U256, RaindexError> {
        let transaction_args = self.get_transaction_args()?;
        Ok(read_allowance(
            &transaction_args.rpcs,
            self.token.address,
            self.owner,
            transaction_args.raindex_address,
        )
        .await?)
    }

    /// Builds the ERC20 approval calldata for `amount`, returning `None` when the raindex
    /// contract already has a sufficient allowance and therefore no approval is needed.
    ///
    /// Used by [`RaindexVault::get_calldatas`] so the on-chain allowance is only read once.
    /// It reads the allowance via [`Self::read_allowance`] (deposit-free), so it takes no
    /// [`DepositArgs`].
    async fn build_approval_calldata(&self, amount: &Float) -> Result<Option<Bytes>, RaindexError> {
        let allowance = self.read_allowance().await?;
        let allowance_float = Float::from_fixed_decimal(allowance, self.token.decimals)?;

        if allowance_float.gte(*amount)? {
            return Ok(None);
        }

        let calldata = approveCall {
            spender: self.raindex,
            amount: amount.to_fixed_decimal(self.token.decimals)?,
        }
        .abi_encode();

        Ok(Some(Bytes::copy_from_slice(&calldata)))
    }

    /// Generates every transaction calldata associated with a vault in a single call
    ///
    /// Produces the approval (when needed), deposit and withdraw calldata for the vault
    /// in one method so callers don't have to invoke them separately. The on-chain
    /// allowance is read only once.
    ///
    /// The returned `approval` is `undefined` when the raindex contract already has a
    /// sufficient allowance to spend the requested amount, in which case no approval
    /// transaction is needed.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await vault.getCalldatas("10.5");
    /// if (result.error) {
    ///   console.error("Cannot generate calldatas:", result.error.readableMsg);
    ///   return;
    /// }
    /// const { approval, deposit, withdraw } = result.value;
    /// // `approval` is undefined when no approval is needed
    /// ```
    #[wasm_export(
        js_name = "getCalldatas",
        return_description = "Approval (when needed), deposit and withdraw calldata for the amount",
        unchecked_return_type = "RaindexVaultCalldatas"
    )]
    pub async fn get_calldatas(
        &self,
        #[wasm_export(param_description = "Amount in Float value")] amount: &Float,
    ) -> Result<RaindexVaultCalldatas, RaindexError> {
        self.validate_amount(amount)?;

        let approval = self.build_approval_calldata(amount).await?;

        let deposit_args = self.get_deposit_args(amount);
        let deposit = Bytes::copy_from_slice(&deposit5Call::try_from(deposit_args)?.abi_encode());

        let withdraw = self.build_withdraw_calldata(amount).await?;

        Ok(RaindexVaultCalldatas {
            approval,
            deposit,
            withdraw,
        })
    }

    /// Gets the current ERC20 allowance for a vault
    ///
    /// Determines how much the raindex contract is currently approved to spend
    /// on behalf of the vault owner.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await vault.getAllowance();
    /// if (result.error) {
    ///   console.error("Cannot check allowance:", result.error.readableMsg);
    ///   return;
    /// }
    /// const allowance = result.value;
    /// // Do something with the allowance
    /// ```
    #[wasm_export(
        js_name = "getAllowance",
        return_description = "Current allowance amount in token's smallest unit (e.g., \"1000000000000000000\" for 1 token with 18 decimals)"
    )]
    pub async fn get_allowance(&self) -> Result<RaindexVaultAllowance, RaindexError> {
        let allowance = self.read_allowance().await?;
        Ok(RaindexVaultAllowance(allowance))
    }

    /// Normalizes an amount to the largest token-decimal amount that can be withdrawn.
    ///
    /// Converts the provided [`Float`] through this vault token's fixed-decimal
    /// precision, truncating any sub-token-base-unit dust, then returns it as a
    /// [`Float`] that can be safely encoded for withdraw calldata.
    #[wasm_export(
        js_name = "tokenSafeWithdrawAmount",
        return_description = "Float amount normalized to this vault token's withdrawable precision",
        unchecked_return_type = "Float",
        preserve_js_class
    )]
    pub fn token_safe_withdraw_amount(
        &self,
        #[wasm_export(param_description = "Amount in Float value")] amount: &Float,
    ) -> Result<Float, RaindexError> {
        let (fixed_amount, _) = amount.to_fixed_decimal_lossy(self.token.decimals)?;
        Ok(Float::from_fixed_decimal(
            fixed_amount,
            self.token.decimals,
        )?)
    }

    /// Fetches the balance of the owner for this vault
    ///
    /// Retrieves the current balance of the vault owner.
    /// The returned balance is an object containing both raw and formatted values.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await vault.getOwnerBalance();
    /// if (result.error) {
    ///  console.error("Error fetching balance:", result.error.readableMsg);
    /// return;
    /// }
    /// const accountBalance = result.value;
    /// console.log("Raw balance:", accountBalance.balance);
    /// console.log("Formatted balance:", accountBalance.formattedBalance);
    /// ```
    #[wasm_export(
        js_name = "getOwnerBalance",
        return_description = "Owner balance in both raw and human-readable format",
        unchecked_return_type = "AccountBalance",
        preserve_js_class
    )]
    pub async fn get_owner_balance_wasm_binding(&self) -> Result<AccountBalance, RaindexError> {
        self.account_balance(self.owner).await
    }

    /// Fetches the ERC20 balance of `account` for this vault's token.
    ///
    /// Same as [`RaindexVault::get_owner_balance_wasm_binding`], but for an
    /// arbitrary address rather than the vault owner. Used by takers to cap a
    /// take-order MAX against their own wallet balance.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await vault.getAccountBalance(takerAddress);
    /// if (result.error) {
    ///   console.error("Error fetching balance:", result.error.readableMsg);
    ///   return;
    /// }
    /// const accountBalance = result.value;
    /// console.log("Raw balance:", accountBalance.balance);
    /// console.log("Formatted balance:", accountBalance.formattedBalance);
    /// ```
    #[wasm_export(
        js_name = "getAccountBalance",
        return_description = "Account balance in both raw and human-readable format",
        unchecked_return_type = "AccountBalance",
        preserve_js_class
    )]
    pub async fn get_account_balance_wasm_binding(
        &self,
        #[wasm_export(param_description = "Account address to check balance for")] account: String,
    ) -> Result<AccountBalance, RaindexError> {
        self.account_balance(Address::from_str(&account)?).await
    }
}
impl RaindexVault {
    pub async fn get_owner_balance(&self, owner: Address) -> Result<U256, RaindexError> {
        let rpcs = self.raindex_client.get_rpc_urls_for_chain(self.chain_id)?;
        let erc20 = ERC20::new(rpcs, self.token.address);
        Ok(erc20.get_account_balance(owner).await?)
    }

    async fn account_balance(&self, account: Address) -> Result<AccountBalance, RaindexError> {
        let balance = self.get_owner_balance(account).await?;
        let decimals = self.token.decimals;
        let float_balance = Float::from_fixed_decimal(balance, decimals)?;
        Ok(AccountBalance {
            balance: float_balance,
            formatted_balance: float_balance.format()?,
        })
    }

    /// Builds the withdraw calldata for `amount`.
    ///
    /// Used by [`RaindexVault::get_calldatas`] and by
    /// [`RaindexVaultsList::get_withdraw_calldata`] to build the per-vault withdraw
    /// calldata that is multicalled together.
    pub async fn build_withdraw_calldata(&self, amount: &Float) -> Result<Bytes, RaindexError> {
        self.validate_amount(amount)?;
        Ok(Bytes::copy_from_slice(
            &WithdrawArgs {
                token: self.token.address,
                vault_id: B256::from(self.vault_id),
                target_amount: *amount,
            }
            .get_withdraw_calldata()
            .await?,
        ))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum RaindexVaultBalanceChangeType {
    Deposit,
    Withdrawal,
    TakeOrder,
    Clear,
    ClearBounty,
    Unknown,
}
impl_wasm_traits!(RaindexVaultBalanceChangeType);

impl From<VaultBalanceChangeKind> for RaindexVaultBalanceChangeType {
    fn from(kind: VaultBalanceChangeKind) -> Self {
        match kind {
            VaultBalanceChangeKind::Deposit => Self::Deposit,
            VaultBalanceChangeKind::Withdrawal => Self::Withdrawal,
            VaultBalanceChangeKind::TakeOrder => Self::TakeOrder,
            VaultBalanceChangeKind::Clear => Self::Clear,
            VaultBalanceChangeKind::ClearBounty => Self::ClearBounty,
            VaultBalanceChangeKind::Unknown => Self::Unknown,
        }
    }
}

impl TryFrom<String> for RaindexVaultBalanceChangeType {
    type Error = RaindexError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let kind = VaultBalanceChangeKind::from_local_db_change_type(&value);
        if matches!(kind, VaultBalanceChangeKind::Unknown) {
            let kind_from_sg = VaultBalanceChangeKind::from_subgraph_typename(&value);
            if matches!(kind_from_sg, VaultBalanceChangeKind::Unknown) && value != "Unknown" {
                return Err(RaindexError::InvalidVaultBalanceChangeType(value));
            }
            return Ok(kind_from_sg.into());
        }
        Ok(kind.into())
    }
}

impl RaindexVaultBalanceChangeType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Deposit => "Deposit",
            Self::Withdrawal => "Withdrawal",
            Self::TakeOrder => "Take order",
            Self::Clear => "Clear",
            Self::ClearBounty => "Clear Bounty",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum VaultBalanceChangeFilter {
    Deposit,
    Withdrawal,
    TakeOrder,
    Clear,
    ClearBounty,
}
impl_wasm_traits!(VaultBalanceChangeFilter);

impl VaultBalanceChangeFilter {
    pub fn to_kind(&self) -> VaultBalanceChangeKind {
        match self {
            Self::Deposit => VaultBalanceChangeKind::Deposit,
            Self::Withdrawal => VaultBalanceChangeKind::Withdrawal,
            Self::TakeOrder => VaultBalanceChangeKind::TakeOrder,
            Self::Clear => VaultBalanceChangeKind::Clear,
            Self::ClearBounty => VaultBalanceChangeKind::ClearBounty,
        }
    }

    pub fn to_local_db_types(&self) -> &'static [&'static str] {
        self.to_kind().to_local_db_change_types()
    }

    pub fn to_raindex_type(&self) -> RaindexVaultBalanceChangeType {
        match self {
            Self::Deposit => RaindexVaultBalanceChangeType::Deposit,
            Self::Withdrawal => RaindexVaultBalanceChangeType::Withdrawal,
            Self::TakeOrder => RaindexVaultBalanceChangeType::TakeOrder,
            Self::Clear => RaindexVaultBalanceChangeType::Clear,
            Self::ClearBounty => RaindexVaultBalanceChangeType::ClearBounty,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct RaindexVaultBalanceChange {
    r#type: RaindexVaultBalanceChangeType,
    vault_id: U256,
    token: RaindexVaultToken,
    amount: Float,
    formatted_amount: String,
    new_balance: Float,
    formatted_new_balance: String,
    old_balance: Float,
    formatted_old_balance: String,
    timestamp: U256,
    transaction: RaindexTransaction,
    raindex: Address,
}

#[cfg(not(target_family = "wasm"))]
impl RaindexVaultBalanceChange {
    pub(crate) fn raw_token_address(&self) -> Address {
        self.token.raw_address()
    }

    pub(crate) fn raw_amount(&self) -> Float {
        self.amount
    }
}
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl RaindexVaultBalanceChange {
    #[wasm_bindgen(getter = type)]
    pub fn type_getter(&self) -> RaindexVaultBalanceChangeType {
        self.r#type.clone()
    }
    #[wasm_bindgen(getter = typeDisplayName)]
    pub fn type_display_name(&self) -> String {
        self.r#type.display_name().to_string()
    }
    #[wasm_bindgen(getter = vaultId)]
    pub fn vault_id(&self) -> Result<BigInt, RaindexError> {
        BigInt::from_str(&self.vault_id.to_string())
            .map_err(|e| RaindexError::JsError(e.to_string().into()))
    }
    #[wasm_bindgen(getter)]
    pub fn token(&self) -> RaindexVaultToken {
        self.token.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn amount(&self) -> Float {
        self.amount
    }
    #[wasm_bindgen(getter = formattedAmount)]
    pub fn formatted_amount(&self) -> String {
        self.formatted_amount.clone()
    }
    #[wasm_bindgen(getter = newBalance)]
    pub fn new_balance(&self) -> Float {
        self.new_balance
    }
    #[wasm_bindgen(getter = formattedNewBalance)]
    pub fn formatted_new_balance(&self) -> String {
        self.formatted_new_balance.clone()
    }
    #[wasm_bindgen(getter = oldBalance)]
    pub fn old_balance(&self) -> Float {
        self.old_balance
    }
    #[wasm_bindgen(getter = formattedOldBalance)]
    pub fn formatted_old_balance(&self) -> String {
        self.formatted_old_balance.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn timestamp(&self) -> Result<BigInt, RaindexError> {
        BigInt::from_str(&self.timestamp.to_string())
            .map_err(|e| RaindexError::JsError(e.to_string().into()))
    }
    #[wasm_bindgen(getter)]
    pub fn transaction(&self) -> RaindexTransaction {
        self.transaction.clone()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Address")]
    pub fn raindex(&self) -> String {
        self.raindex.to_string()
    }
}
#[cfg(not(target_family = "wasm"))]
impl RaindexVaultBalanceChange {
    pub fn r#type(&self) -> RaindexVaultBalanceChangeType {
        self.r#type.clone()
    }
    pub fn type_display_name(&self) -> &'static str {
        self.r#type.display_name()
    }
    pub fn vault_id(&self) -> U256 {
        self.vault_id
    }
    pub fn token(&self) -> RaindexVaultToken {
        self.token.clone()
    }
    pub fn amount(&self) -> Float {
        self.amount
    }
    pub fn formatted_amount(&self) -> String {
        self.formatted_amount.clone()
    }
    pub fn new_balance(&self) -> Float {
        self.new_balance
    }
    pub fn formatted_new_balance(&self) -> String {
        self.formatted_new_balance.clone()
    }
    pub fn old_balance(&self) -> Float {
        self.old_balance
    }
    pub fn formatted_old_balance(&self) -> String {
        self.formatted_old_balance.clone()
    }
    pub fn timestamp(&self) -> U256 {
        self.timestamp
    }
    pub fn transaction(&self) -> RaindexTransaction {
        self.transaction.clone()
    }
    pub fn raindex(&self) -> Address {
        self.raindex
    }
}

#[derive(Clone)]
pub(crate) struct LocalTradeTokenInfo {
    pub address: Address,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub decimals: Option<u8>,
}

#[derive(Clone)]
pub(crate) struct LocalTradeBalanceInfo {
    pub delta: String,
    pub running_balance: Option<String>,
    pub trade_kind: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Tsify)]
pub struct RaindexVaultAllowance(#[tsify(type = "string")] U256);
impl_wasm_traits!(RaindexVaultAllowance);

/// Bundle of every transaction calldata associated with a vault for a given amount.
///
/// Returned by [`RaindexVault::get_calldatas`] so callers can generate the deposit,
/// withdraw and (when required) approval calldata for a vault in a single call instead
/// of invoking the three separate calldata functions individually.
///
/// `approval` is `None` when the raindex contract already has a sufficient allowance to
/// spend the requested amount, in which case no approval transaction is needed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct RaindexVaultCalldatas {
    #[tsify(optional, type = "Hex")]
    pub approval: Option<Bytes>,
    #[tsify(type = "Hex")]
    pub deposit: Bytes,
    #[tsify(type = "Hex")]
    pub withdraw: Bytes,
}
impl_wasm_traits!(RaindexVaultCalldatas);

impl RaindexVaultBalanceChange {
    pub fn try_from_sg_balance_change(
        chain_id: u32,
        balance_change: SgVaultBalanceChangeUnwrapped,
    ) -> Result<Self, RaindexError> {
        let token = RaindexVaultToken::try_from_sg_erc20(chain_id, balance_change.vault.token)?;

        let amount = Float::from_hex(&balance_change.amount.0)?;
        let new_balance = Float::from_hex(&balance_change.new_vault_balance.0)?;
        let old_balance = Float::from_hex(&balance_change.old_vault_balance.0)?;

        let formatted_amount = amount.format()?;
        let formatted_new_balance = new_balance.format()?;
        let formatted_old_balance = old_balance.format()?;

        Ok(Self {
            r#type: balance_change.__typename.try_into()?,
            vault_id: U256::from_str(&balance_change.vault.vault_id.0)?,
            token,
            amount,
            formatted_amount,
            new_balance,
            formatted_new_balance,
            old_balance,
            formatted_old_balance,
            timestamp: U256::from_str(&balance_change.timestamp.0)?,
            transaction: RaindexTransaction::try_from(balance_change.transaction)?,
            raindex: Address::from_str(&balance_change.raindex.id.0)?,
        })
    }
}

impl RaindexVaultBalanceChange {
    pub fn try_from_sg_trade_balance_change(
        chain_id: u32,
        balance_change: SgTradeVaultBalanceChange,
    ) -> Result<Self, RaindexError> {
        let token = RaindexVaultToken::try_from_sg_erc20(chain_id, balance_change.vault.token)?;

        let amount = Float::from_hex(&balance_change.amount.0)?;
        let new_balance = Float::from_hex(&balance_change.new_vault_balance.0)?;
        let old_balance = Float::from_hex(&balance_change.old_vault_balance.0)?;

        let formatted_amount = amount.format()?;
        let formatted_new_balance = new_balance.format()?;
        let formatted_old_balance = old_balance.format()?;

        let change_type: RaindexVaultBalanceChangeType =
            VaultBalanceChangeKind::from_subgraph_typename(
                &balance_change.trade.trade_event.__typename,
            )
            .into();

        Ok(Self {
            r#type: change_type,
            vault_id: U256::from_str(&balance_change.vault.vault_id.0)?,
            token,
            amount,
            formatted_amount,
            new_balance,
            formatted_new_balance,
            old_balance,
            formatted_old_balance,
            timestamp: U256::from_str(&balance_change.timestamp.0)?,
            transaction: RaindexTransaction::try_from(balance_change.transaction)?,
            raindex: Address::from_str(&balance_change.raindex.id.0)?,
        })
    }
}

impl RaindexVaultBalanceChange {
    pub fn try_from_sg_balance_change_type(
        chain_id: u32,
        balance_change: SgVaultBalanceChangeType,
    ) -> Result<Self, RaindexError> {
        match balance_change {
            SgVaultBalanceChangeType::Deposit(deposit) => {
                let token = RaindexVaultToken::try_from_sg_erc20(chain_id, deposit.vault.token)?;
                let amount = Float::from_hex(&deposit.amount.0)?;
                let new_balance = Float::from_hex(&deposit.new_vault_balance.0)?;
                let old_balance = Float::from_hex(&deposit.old_vault_balance.0)?;

                Ok(Self {
                    r#type: RaindexVaultBalanceChangeType::Deposit,
                    vault_id: U256::from_str(&deposit.vault.vault_id.0)?,
                    token,
                    amount,
                    formatted_amount: amount.format()?,
                    new_balance,
                    formatted_new_balance: new_balance.format()?,
                    old_balance,
                    formatted_old_balance: old_balance.format()?,
                    timestamp: U256::from_str(&deposit.timestamp.0)?,
                    transaction: RaindexTransaction::try_from(deposit.transaction)?,
                    raindex: Address::from_str(&deposit.raindex.id.0)?,
                })
            }
            SgVaultBalanceChangeType::Withdrawal(withdrawal) => {
                let token = RaindexVaultToken::try_from_sg_erc20(chain_id, withdrawal.vault.token)?;
                let amount = Float::from_hex(&withdrawal.amount.0)?;
                let new_balance = Float::from_hex(&withdrawal.new_vault_balance.0)?;
                let old_balance = Float::from_hex(&withdrawal.old_vault_balance.0)?;

                Ok(Self {
                    r#type: RaindexVaultBalanceChangeType::Withdrawal,
                    vault_id: U256::from_str(&withdrawal.vault.vault_id.0)?,
                    token,
                    amount,
                    formatted_amount: amount.format()?,
                    new_balance,
                    formatted_new_balance: new_balance.format()?,
                    old_balance,
                    formatted_old_balance: old_balance.format()?,
                    timestamp: U256::from_str(&withdrawal.timestamp.0)?,
                    transaction: RaindexTransaction::try_from(withdrawal.transaction)?,
                    raindex: Address::from_str(&withdrawal.raindex.id.0)?,
                })
            }
            SgVaultBalanceChangeType::TradeVaultBalanceChange(trade_change) => {
                Self::try_from_sg_trade_balance_change(chain_id, trade_change)
            }
            SgVaultBalanceChangeType::ClearBounty(bounty) => {
                let token = RaindexVaultToken::try_from_sg_erc20(chain_id, bounty.vault.token)?;
                let amount = Float::from_hex(&bounty.amount.0)?;
                let new_balance = Float::from_hex(&bounty.new_vault_balance.0)?;
                let old_balance = Float::from_hex(&bounty.old_vault_balance.0)?;

                Ok(Self {
                    r#type: RaindexVaultBalanceChangeType::ClearBounty,
                    vault_id: U256::from_str(&bounty.vault.vault_id.0)?,
                    token,
                    amount,
                    formatted_amount: amount.format()?,
                    new_balance,
                    formatted_new_balance: new_balance.format()?,
                    old_balance,
                    formatted_old_balance: old_balance.format()?,
                    timestamp: U256::from_str(&bounty.timestamp.0)?,
                    transaction: RaindexTransaction::try_from(bounty.transaction)?,
                    raindex: Address::from_str(&bounty.raindex.id.0)?,
                })
            }
            SgVaultBalanceChangeType::Unknown => Err(RaindexError::InvalidVaultBalanceChangeType(
                "Unknown".to_string(),
            )),
        }
    }
}

impl RaindexVaultBalanceChange {
    pub fn try_from_local_db(
        vault: &RaindexVault,
        change: LocalDbVaultBalanceChange,
    ) -> Result<Self, RaindexError> {
        let amount = Float::from_hex(&change.delta)?;
        let new_balance = Float::from_hex(&change.running_balance)?;
        let old_balance = (new_balance - amount)?;

        let formatted_amount = amount.format()?;
        let formatted_new_balance = new_balance.format()?;
        let formatted_old_balance = old_balance.format()?;

        let transaction = RaindexTransaction::from_local_parts(
            change.transaction_hash,
            change.transaction_sender,
            change.block_number,
            change.block_timestamp,
        )?;

        let change_type = RaindexVaultBalanceChangeType::try_from(change.change_type)?;

        Ok(Self {
            r#type: change_type,
            vault_id: vault.vault_id,
            token: vault.token.clone(),
            amount,
            formatted_amount,
            new_balance,
            formatted_new_balance,
            old_balance,
            formatted_old_balance,
            timestamp: U256::from(change.block_timestamp),
            transaction,
            raindex: vault.raindex,
        })
    }

    pub(crate) fn try_from_local_trade_side(
        chain_id: u32,
        raindex_addr: Address,
        transaction: &RaindexTransaction,
        vault_id: U256,
        token: LocalTradeTokenInfo,
        balance: LocalTradeBalanceInfo,
        block_timestamp: u64,
    ) -> Result<Self, RaindexError> {
        let amount = Float::from_hex(&balance.delta)?;
        let new_balance = match balance.running_balance.as_ref() {
            Some(balance) => Float::from_hex(balance)?,
            None => amount,
        };
        let old_balance = (new_balance - amount)?;

        let formatted_amount = amount.format()?;
        let formatted_new_balance = new_balance.format()?;
        let formatted_old_balance = old_balance.format()?;

        let LocalTradeTokenInfo {
            address,
            name,
            symbol,
            decimals,
        } = token;
        let decimals = decimals.unwrap_or(18);
        let token = RaindexVaultToken {
            chain_id,
            id: hex::encode_prefixed(address),
            address,
            name,
            symbol,
            decimals,
        };

        let change_type: RaindexVaultBalanceChangeType =
            VaultBalanceChangeKind::from_local_db_trade_kind(&balance.trade_kind).into();

        Ok(Self {
            r#type: change_type,
            vault_id,
            token,
            amount,
            formatted_amount,
            new_balance,
            formatted_new_balance,
            old_balance,
            formatted_old_balance,
            timestamp: U256::from(block_timestamp),
            transaction: transaction.clone(),
            raindex: raindex_addr,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct RaindexVaultVolume {
    id: U256,
    token: RaindexVaultToken,
    details: RaindexVaultVolumeDetails,
}
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl RaindexVaultVolume {
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> Result<BigInt, RaindexError> {
        BigInt::from_str(&self.id.to_string())
            .map_err(|e| RaindexError::JsError(e.to_string().into()))
    }
    #[wasm_bindgen(getter)]
    pub fn token(&self) -> RaindexVaultToken {
        self.token.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn details(&self) -> RaindexVaultVolumeDetails {
        self.details.clone()
    }
}
#[cfg(not(target_family = "wasm"))]
impl RaindexVaultVolume {
    pub fn id(&self) -> U256 {
        self.id
    }
    pub fn token(&self) -> RaindexVaultToken {
        self.token.clone()
    }
    pub fn details(&self) -> RaindexVaultVolumeDetails {
        self.details.clone()
    }
}
impl RaindexVaultVolume {
    pub fn try_from_vault_volume(
        chain_id: u32,
        vault_volume: VaultVolume,
    ) -> Result<Self, RaindexError> {
        let token = RaindexVaultToken::try_from_sg_erc20(chain_id, vault_volume.token)?;
        let details = RaindexVaultVolumeDetails::from_volume_details(vault_volume.vol_details)?;
        Ok(Self {
            id: U256::from_str(&vault_volume.id)?,
            token,
            details,
        })
    }

    pub fn try_from_local_db_vault_volume(
        chain_id: u32,
        volume: LocalDbVaultVolume,
    ) -> Result<Self, RaindexError> {
        let decimals = volume
            .token_decimals
            .ok_or(RaindexError::MissingErc20Decimals(volume.token.to_string()))?;

        let token = RaindexVaultToken {
            chain_id,
            id: volume.token.to_string(),
            address: volume.token,
            name: volume.token_name,
            symbol: volume.token_symbol,
            decimals,
        };

        let total_in = Float::from_hex(&volume.total_in)?;
        let total_out = Float::from_hex(&volume.total_out)?;
        let total_vol = (total_in + total_out)?;
        let net_vol = (total_in - total_out)?;

        let details = RaindexVaultVolumeDetails {
            total_in,
            formatted_total_in: total_in.format()?,
            total_out,
            formatted_total_out: total_out.format()?,
            total_vol,
            formatted_total_vol: total_vol.format()?,
            net_vol,
            formatted_net_vol: net_vol.format()?,
        };

        Ok(Self {
            id: volume.vault_id,
            token,
            details,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct RaindexVaultVolumeDetails {
    total_in: Float,
    formatted_total_in: String,
    total_out: Float,
    formatted_total_out: String,
    total_vol: Float,
    formatted_total_vol: String,
    net_vol: Float,
    formatted_net_vol: String,
}
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl RaindexVaultVolumeDetails {
    #[wasm_bindgen(getter = totalIn)]
    pub fn total_in(&self) -> Float {
        self.total_in
    }
    #[wasm_bindgen(getter = formattedTotalIn)]
    pub fn formatted_total_in(&self) -> String {
        self.formatted_total_in.clone()
    }
    #[wasm_bindgen(getter = totalOut)]
    pub fn total_out(&self) -> Float {
        self.total_out
    }
    #[wasm_bindgen(getter = formattedTotalOut)]
    pub fn formatted_total_out(&self) -> String {
        self.formatted_total_out.clone()
    }
    #[wasm_bindgen(getter = totalVol)]
    pub fn total_vol(&self) -> Float {
        self.total_vol
    }
    #[wasm_bindgen(getter = formattedTotalVol)]
    pub fn formatted_total_vol(&self) -> String {
        self.formatted_total_vol.clone()
    }
    #[wasm_bindgen(getter = netVol)]
    pub fn net_vol(&self) -> Float {
        self.net_vol
    }
    #[wasm_bindgen(getter = formattedNetVol)]
    pub fn formatted_net_vol(&self) -> String {
        self.formatted_net_vol.clone()
    }
}
#[cfg(not(target_family = "wasm"))]
impl RaindexVaultVolumeDetails {
    pub fn total_in(&self) -> Float {
        self.total_in
    }
    pub fn formatted_total_in(&self) -> String {
        self.formatted_total_in.clone()
    }
    pub fn total_out(&self) -> Float {
        self.total_out
    }
    pub fn formatted_total_out(&self) -> String {
        self.formatted_total_out.clone()
    }
    pub fn total_vol(&self) -> Float {
        self.total_vol
    }
    pub fn formatted_total_vol(&self) -> String {
        self.formatted_total_vol.clone()
    }
    pub fn net_vol(&self) -> Float {
        self.net_vol
    }
    pub fn formatted_net_vol(&self) -> String {
        self.formatted_net_vol.clone()
    }
}
impl RaindexVaultVolumeDetails {
    pub fn from_volume_details(volume_details: VolumeDetails) -> Result<Self, RaindexError> {
        Ok(Self {
            total_in: volume_details.total_in,
            formatted_total_in: volume_details.total_in.format()?,
            total_out: volume_details.total_out,
            formatted_total_out: volume_details.total_out.format()?,
            total_vol: volume_details.total_vol,
            formatted_total_vol: volume_details.total_vol.format()?,
            net_vol: volume_details.net_vol,
            formatted_net_vol: volume_details.net_vol.format()?,
        })
    }
}

#[wasm_export]
impl RaindexClient {
    /// Fetches a page of vault data from multiple networks with pagination metadata.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await client.getVaults(
    ///   {
    ///     owners: ["0x1234567890abcdef1234567890abcdef12345678"],
    ///     hide_zero_balance: true
    ///   },
    /// );
    /// if (result.error) {
    ///   console.error("Error fetching vaults:", result.error.readableMsg);
    ///   return;
    /// }
    /// const { items, totalItems, hasMore } = result.value;
    /// // Do something with the vaults
    /// ```
    #[wasm_export(
        js_name = "getVaults",
        return_description = "Vault list result with pagination metadata",
        unchecked_return_type = "RaindexVaultsListResult",
        preserve_js_class
    )]
    pub async fn get_vaults(
        &self,
        #[wasm_export(
            js_name = "chainIds",
            param_description = "Specific networks to query (optional)"
        )]
        chain_ids: Option<ChainIds>,
        #[wasm_export(
            param_description = "Optional filtering options including owners and hide_zero_balance"
        )]
        filters: Option<GetVaultsFilters>,
        #[wasm_export(param_description = "Optional page number (defaults to 1)")] page: Option<
            u16,
        >,
        #[wasm_export(
            js_name = "pageSize",
            param_description = "Number of vaults per page (optional, defaults to 100)"
        )]
        page_size: Option<u16>,
    ) -> Result<RaindexVaultsListResult, RaindexError> {
        let filters = filters.unwrap_or_default();
        let page_number = page.unwrap_or(1).max(1);
        let page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE).max(1);
        let ids = chain_ids.map(|ChainIds(ids)| ids);
        let (local_db, local_ids, sg_ids) = self.classify_chains(ids)?;
        let has_subgraph_sources = !sg_ids.is_empty();
        let has_local_source = local_db.is_some();
        let subgraph_source_count = if has_subgraph_sources {
            self.get_multi_subgraph_args(Some(sg_ids.clone()))?
                .values()
                .map(Vec::len)
                .sum::<usize>()
        } else {
            0
        };
        let use_source_pagination =
            !(has_local_source && has_subgraph_sources) && subgraph_source_count <= 1;

        let mut all_vaults = Vec::new();
        let mut total_items = 0u32;

        if let Some(db) = local_db {
            let local_source = LocalDbVaults::new(&db, ClientRef::new(self.clone()));
            total_items += local_source
                .count(Some(local_ids.clone()), &filters)
                .await?;
            all_vaults.extend(
                local_source
                    .list(
                        Some(local_ids),
                        &filters,
                        use_source_pagination.then_some(page_number),
                        use_source_pagination.then_some(page_size),
                    )
                    .await?,
            );
        }

        if !sg_ids.is_empty() {
            let subgraph_source = SubgraphVaults::new(self);
            total_items += subgraph_source
                .count(Some(sg_ids.clone()), &filters)
                .await?;
            all_vaults.extend(
                subgraph_source
                    .list(
                        Some(sg_ids),
                        &filters,
                        use_source_pagination.then_some(page_number),
                        use_source_pagination.then_some(page_size),
                    )
                    .await?,
            );
        }

        if !use_source_pagination {
            sort_vaults_for_pagination(&mut all_vaults);
            all_vaults = page_vaults(all_vaults, page_number, page_size);
        }
        let has_more = u32::from(page_number) * u32::from(page_size) < total_items;

        Ok(RaindexVaultsListResult {
            vaults: RaindexVaultsList::new(all_vaults),
            page: page_number,
            page_size,
            total_items,
            has_more,
        })
    }

    /// Aggregates non-zero vault balances by chain and token.
    #[wasm_export(
        js_name = "getVaultTotals",
        return_description = "Non-zero vault balance totals grouped by token",
        unchecked_return_type = "RaindexVaultTotal[]",
        preserve_js_class
    )]
    pub async fn get_vault_totals(
        &self,
        #[wasm_export(
            js_name = "chainIds",
            param_description = "Specific networks to query (optional)"
        )]
        chain_ids: Option<ChainIds>,
    ) -> Result<Vec<RaindexVaultTotal>, RaindexError> {
        let filters = GetVaultsFilters {
            owners: vec![],
            hide_zero_balance: true,
            tokens: None,
            raindex_addresses: None,
            only_active_orders: false,
        };
        let page_size = 1000u16;
        let zero = Float::zero()?;
        let mut totals: BTreeMap<(u32, Address), RaindexVaultTotal> = BTreeMap::new();
        let ids = chain_ids.map(|ChainIds(ids)| ids);
        let (local_db, local_ids, sg_ids) = self.classify_chains(ids)?;

        if let Some(db) = local_db {
            let local_source = LocalDbVaults::new(&db, ClientRef::new(self.clone()));
            let mut page = 1u16;

            loop {
                let vaults = local_source
                    .list(
                        Some(local_ids.clone()),
                        &filters,
                        Some(page),
                        Some(page_size),
                    )
                    .await?;
                let batch_len = vaults.len();
                add_vaults_to_totals(&mut totals, vaults, zero)?;

                if batch_len < page_size as usize {
                    break;
                }
                page = page.checked_add(1).ok_or_else(|| {
                    RaindexError::PreflightError(
                        "Vault totals local pagination exhausted u16 page range".to_string(),
                    )
                })?;
            }
        }

        if !sg_ids.is_empty() {
            let subgraph_source = SubgraphVaults::new(self);
            let mut page = 1u16;

            loop {
                let vaults = subgraph_source
                    .list(Some(sg_ids.clone()), &filters, Some(page), Some(page_size))
                    .await?;
                let batch_len = vaults.len();
                add_vaults_to_totals(&mut totals, vaults, zero)?;

                if batch_len < page_size as usize {
                    break;
                }
                page = page.checked_add(1).ok_or_else(|| {
                    RaindexError::PreflightError(
                        "Vault totals subgraph pagination exhausted u16 page range".to_string(),
                    )
                })?;
            }
        }

        Ok(totals.into_values().collect())
    }

    /// Fetches detailed information for a specific vault
    ///
    /// Retrieves complete vault information including token details, balance, etc.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await client.getVault(
    ///   137, // Polygon network
    ///   "0x1234567890abcdef1234567890abcdef12345678"
    /// );
    /// if (result.error) {
    ///   console.error("Vault not found:", result.error.readableMsg);
    ///   return;
    /// }
    /// const vault = result.value;
    /// // Do something with the vault
    /// ```
    #[wasm_export(
        js_name = "getVault",
        return_description = "Complete vault information",
        unchecked_return_type = "RaindexVault",
        preserve_js_class
    )]
    pub async fn get_vault_wasm_binding(
        &self,
        #[wasm_export(
            js_name = "chainId",
            param_description = "Chain ID of the network the vault is on"
        )]
        chain_id: u32,
        #[wasm_export(
            js_name = "raindexAddress",
            param_description = "Raindex contract address",
            unchecked_param_type = "Address"
        )]
        raindex_address: String,
        #[wasm_export(
            js_name = "vaultId",
            param_description = "Unique vault identifier",
            unchecked_param_type = "Hex"
        )]
        vault_id: String,
    ) -> Result<RaindexVault, RaindexError> {
        let raindex_address = Address::from_str(&raindex_address)?;
        let vault_id = Bytes::from_str(&vault_id)?;
        self.get_vault(&RaindexIdentifier::new(chain_id, raindex_address), vault_id)
            .await
    }

    /// Fetches all unique tokens that exist in vaults.
    ///
    /// Retrieves all unique ERC20 tokens that have associated vaults by querying
    /// all vaults and extracting their token information, removing duplicates.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// const result = await client.getAllVaultTokens();
    /// if (result.error) {
    ///   console.error("Error fetching tokens:", result.error.readableMsg);
    ///   return;
    /// }
    /// const tokens = result.value;
    /// console.log(`Found ${tokens.length} unique tokens`);
    /// console.log(`Token ${tokens[0].name} in ${tokens[0].chainId}`);
    /// ```
    #[wasm_export(
        js_name = "getAllVaultTokens",
        return_description = "Array of raindex vault token instances",
        unchecked_return_type = "RaindexVaultToken[]"
    )]
    pub async fn get_all_vault_tokens(
        &self,
        #[wasm_export(
            js_name = "chainIds",
            param_description = "Specific networks to query (optional)"
        )]
        chain_ids: Option<ChainIds>,
    ) -> Result<Vec<RaindexVaultToken>, RaindexError> {
        let ids = chain_ids.map(|ChainIds(ids)| ids);

        let (local_db, local_ids, sg_ids) = self.classify_chains(ids)?;

        let mut tokens: Vec<RaindexVaultToken> = Vec::new();

        if let Some(db) = local_db {
            let local_source = LocalDbVaults::new(&db, ClientRef::new(self.clone()));
            let local_tokens = local_source.tokens_list(Some(local_ids)).await?;
            tokens.extend(local_tokens);
        }

        if !sg_ids.is_empty() {
            let subgraph_source = SubgraphVaults::new(self);
            let sg_tokens = subgraph_source.tokens_list(Some(sg_ids)).await?;
            tokens.extend(sg_tokens);
        }

        Ok(tokens)
    }
}
impl RaindexClient {
    pub async fn get_vault(
        &self,
        raindex_id: &RaindexIdentifier,
        vault_id: Bytes,
    ) -> Result<RaindexVault, RaindexError> {
        let raindex_cfg = self.get_raindex_by_address(raindex_id.raindex_address)?;
        if raindex_cfg.network.chain_id != raindex_id.chain_id {
            return Err(RaindexError::RaindexNotFound(
                raindex_id.raindex_address.to_string(),
                raindex_id.chain_id,
            ));
        }

        match self.query_source(raindex_id.chain_id) {
            QuerySource::LocalDb(local_db) => {
                let local_source = LocalDbVaults::new(&local_db, ClientRef::new(self.clone()));
                local_source
                    .get_by_id(raindex_id, &vault_id)
                    .await?
                    .ok_or_else(|| {
                        RaindexError::VaultNotFound(
                            raindex_id.raindex_address.to_string(),
                            raindex_id.chain_id,
                            vault_id.to_string(),
                        )
                    })
            }
            QuerySource::Subgraph => SubgraphVaults::new(self)
                .get_by_id(raindex_id, &vault_id)
                .await?
                .ok_or_else(|| {
                    RaindexError::VaultNotFound(
                        raindex_id.raindex_address.to_string(),
                        raindex_id.chain_id,
                        vault_id.to_string(),
                    )
                }),
        }
    }
}

#[cfg_attr(target_family = "wasm", async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
impl VaultsDataSource for SubgraphVaults<'_> {
    async fn list(
        &self,
        chain_ids: Option<Vec<u32>>,
        filters: &GetVaultsFilters,
        page: Option<u16>,
        page_size: Option<u16>,
    ) -> Result<Vec<RaindexVault>, RaindexError> {
        let raindex_client = ClientRef::new(self.client.clone());
        let multi_subgraph_args = self.client.get_multi_subgraph_args(chain_ids)?;
        let client = MultiRaindexSubgraphClient::new(
            multi_subgraph_args.values().flatten().cloned().collect(),
        );

        let sg_filters = filters.clone().try_into()?;
        let vaults = if let Some(page) = page {
            client
                .vaults_list_strict(
                    sg_filters,
                    SgPaginationArgs {
                        page,
                        page_size: page_size.unwrap_or(DEFAULT_PAGE_SIZE),
                    },
                )
                .await?
        } else {
            let mut vaults = Vec::new();
            let mut page = 1u16;
            let page_size = DEFAULT_PAGE_SIZE;

            loop {
                let page_vaults = client
                    .vaults_list_strict(sg_filters.clone(), SgPaginationArgs { page, page_size })
                    .await?;
                let batch_len = page_vaults.len();
                vaults.extend(page_vaults);
                if batch_len < page_size as usize {
                    break;
                }
                page = page.checked_add(1).ok_or_else(|| {
                    RaindexError::PreflightError(
                        "Subgraph vault pagination exhausted u16 page range".to_string(),
                    )
                })?;
            }

            vaults
        };

        let vaults = vaults
            .iter()
            .map(|vault| {
                let chain_id = multi_subgraph_args
                    .iter()
                    .find(|(_, args)| args.iter().any(|arg| arg.name == vault.subgraph_name))
                    .map(|(chain_id, _)| *chain_id)
                    .ok_or_else(|| {
                        RaindexError::SubgraphNotFound(
                            vault.subgraph_name.clone(),
                            vault.vault.vault_id.0.clone(),
                        )
                    })?;
                let vault = RaindexVault::try_from_sg_vault(
                    raindex_client.clone(),
                    chain_id,
                    vault.vault.clone(),
                    None,
                )?;
                Ok(vault)
            })
            .collect::<Result<Vec<RaindexVault>, RaindexError>>()?;

        Ok(vaults)
    }

    async fn count(
        &self,
        chain_ids: Option<Vec<u32>>,
        filters: &GetVaultsFilters,
    ) -> Result<u32, RaindexError> {
        let multi_subgraph_args = self.client.get_multi_subgraph_args(chain_ids)?;
        let client = MultiRaindexSubgraphClient::new(
            multi_subgraph_args.values().flatten().cloned().collect(),
        );
        Ok(client.vaults_count(filters.clone().try_into()?).await?)
    }

    async fn get_by_id(
        &self,
        raindex_id: &RaindexIdentifier,
        vault_id: &Bytes,
    ) -> Result<Option<RaindexVault>, RaindexError> {
        let raindex_client = ClientRef::new(self.client.clone());
        let client = self
            .client
            .get_raindex_subgraph_client(raindex_id.raindex_address)?;
        let vault = match client.vault_detail(Id::new(vault_id.to_string())).await {
            Ok(vault) => vault,
            Err(RaindexSubgraphClientError::Empty) => return Ok(None),
            Err(err) => return Err(err.into()),
        };

        let vault =
            RaindexVault::try_from_sg_vault(raindex_client, raindex_id.chain_id, vault, None)?;
        Ok(Some(vault))
    }

    async fn balance_changes_list(
        &self,
        vault: &RaindexVault,
        page: Option<u16>,
        filter_types: Option<&[VaultBalanceChangeFilter]>,
    ) -> Result<Vec<RaindexVaultBalanceChange>, RaindexError> {
        let client = self.client.get_raindex_subgraph_client(vault.raindex)?;

        let filter_typenames: Option<Vec<&str>> = filter_types.map(|filters| {
            filters
                .iter()
                .flat_map(|f| f.to_kind().to_subgraph_typenames())
                .copied()
                .collect()
        });

        let balance_changes = client
            .vault_balance_changes_list(
                Id::new(vault.id.to_string()),
                SgPaginationArgs {
                    page: page.unwrap_or(1),
                    page_size: 1000,
                },
                filter_typenames.as_deref(),
            )
            .await?;

        balance_changes
            .into_iter()
            .map(|balance_change| {
                RaindexVaultBalanceChange::try_from_sg_balance_change_type(
                    vault.chain_id,
                    balance_change,
                )
            })
            .collect()
    }

    async fn tokens_list(
        &self,
        chain_ids: Option<Vec<u32>>,
    ) -> Result<Vec<RaindexVaultToken>, RaindexError> {
        let multi_subgraph_args = self.client.get_multi_subgraph_args(chain_ids)?;
        let client = MultiRaindexSubgraphClient::new(
            multi_subgraph_args.values().flatten().cloned().collect(),
        );

        let token_list = client.tokens_list().await?;
        token_list
            .iter()
            .map(|v| {
                let chain_id = multi_subgraph_args
                    .iter()
                    .find(|(_, args)| args.iter().any(|arg| arg.name == v.subgraph_name))
                    .map(|(chain_id, _)| *chain_id)
                    .ok_or(RaindexError::SubgraphNotFound(
                        v.subgraph_name.clone(),
                        v.token.address.0.clone(),
                    ))?;
                RaindexVaultToken::try_from_sg_erc20(chain_id, v.token.clone())
            })
            .collect()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Tsify, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetVaultsFilters {
    #[tsify(type = "Address[]")]
    pub owners: Vec<Address>,
    pub hide_zero_balance: bool,
    #[tsify(optional, type = "Address[]")]
    pub tokens: Option<Vec<Address>>,
    #[tsify(optional, type = "Address[]")]
    pub raindex_addresses: Option<Vec<Address>>,
    #[serde(default)]
    pub only_active_orders: bool,
}
impl_wasm_traits!(GetVaultsFilters);

impl TryFrom<GetVaultsFilters> for SgVaultsListFilterArgs {
    type Error = RaindexError;
    fn try_from(filters: GetVaultsFilters) -> Result<Self, Self::Error> {
        Ok(Self {
            owners: filters
                .owners
                .into_iter()
                .map(|owner| SgBytes(owner.to_string()))
                .collect(),
            hide_zero_balance: filters.hide_zero_balance,
            tokens: filters
                .tokens
                .map(|tokens| {
                    tokens
                        .into_iter()
                        .map(|token| token.to_string().to_lowercase())
                        .collect()
                })
                .unwrap_or_default(),
            raindexes: filters
                .raindex_addresses
                .map(|addrs| {
                    addrs
                        .into_iter()
                        .map(|addr| addr.to_string().to_lowercase())
                        .collect()
                })
                .unwrap_or_default(),
            only_active_orders: filters.only_active_orders,
        })
    }
}

impl RaindexVault {
    pub fn try_from_sg_vault(
        raindex_client: ClientRef,
        chain_id: u32,
        vault: SgVault,
        vault_type: Option<RaindexVaultType>,
    ) -> Result<Self, RaindexError> {
        let token = RaindexVaultToken::try_from_sg_erc20(chain_id, vault.token)?;

        let balance = Float::from_hex(&vault.balance.0)?;
        let formatted_balance = balance.format()?;

        Ok(Self {
            raindex_client,
            chain_id,
            vault_type,
            id: Bytes::from_str(&vault.id.0)?,
            owner: Address::from_str(&vault.owner.0)?,
            vault_id: U256::from_str(&vault.vault_id.0)?,
            balance,
            formatted_balance,
            token,
            raindex: Address::from_str(&vault.raindex.id.0)?,
            orders_as_inputs: vault
                .orders_as_input
                .iter()
                .map(|order| RaindexOrderAsIO::try_from(order.clone()))
                .collect::<Result<Vec<RaindexOrderAsIO>, RaindexError>>()?,
            orders_as_outputs: vault
                .orders_as_output
                .iter()
                .map(|order| RaindexOrderAsIO::try_from(order.clone()))
                .collect::<Result<Vec<RaindexOrderAsIO>, RaindexError>>()?,
        })
    }

    pub fn with_vault_type(&self, vault_type: RaindexVaultType) -> Self {
        Self {
            raindex_client: ClientRef::clone(&self.raindex_client),
            chain_id: self.chain_id,
            vault_type: Some(vault_type),
            id: self.id.clone(),
            owner: self.owner,
            vault_id: self.vault_id,
            balance: self.balance,
            formatted_balance: self.formatted_balance.clone(),
            token: self.token.clone(),
            raindex: self.raindex,
            orders_as_inputs: self.orders_as_inputs.clone(),
            orders_as_outputs: self.orders_as_outputs.clone(),
        }
    }

    pub fn into_sg_vault(self) -> Result<SgVault, RaindexError> {
        Ok(SgVault {
            id: SgBytes(self.id.to_string()),
            vault_id: SgBytes(self.vault_id.to_string()),
            balance: SgBytes(self.balance.as_hex()),
            owner: SgBytes(self.owner.to_string()),
            token: self.token.try_into()?,
            raindex: SgRaindex {
                id: SgBytes(self.raindex.to_string()),
            },
            orders_as_input: self
                .orders_as_inputs
                .into_iter()
                .map(|v| v.try_into())
                .collect::<Result<Vec<SgOrderAsIO>, RaindexError>>()?,
            orders_as_output: self
                .orders_as_outputs
                .into_iter()
                .map(|v| v.try_into())
                .collect::<Result<Vec<SgOrderAsIO>, RaindexError>>()?,
            balance_changes: vec![],
        })
    }

    pub fn try_from_local_db(
        raindex_client: ClientRef,
        vault: LocalDbVault,
        vault_type: Option<RaindexVaultType>,
    ) -> Result<Self, RaindexError> {
        let balance = Float::from_hex(&vault.balance)?;
        let formatted_balance = balance.format()?;

        let mut id = Vec::from(vault.raindex_address.as_slice());
        id.extend_from_slice(vault.owner.as_slice());
        id.extend_from_slice(vault.token.as_slice());
        id.extend_from_slice(&vault.vault_id.to_le_bytes::<32>());

        Ok(Self {
            raindex_client,
            chain_id: vault.chain_id,
            vault_type,
            id: Bytes::from(id),
            owner: vault.owner,
            vault_id: vault.vault_id,
            balance,
            formatted_balance,
            token: RaindexVaultToken {
                chain_id: vault.chain_id,
                id: vault.token.to_string(),
                address: vault.token,
                name: Some(vault.token_name),
                symbol: Some(vault.token_symbol),
                decimals: vault.token_decimals,
            },
            raindex: vault.raindex_address,
            orders_as_inputs: RaindexOrderAsIO::try_from_local_db_orders_csv(
                "inputOrders",
                &vault.input_orders,
            )?,
            orders_as_outputs: RaindexOrderAsIO::try_from_local_db_orders_csv(
                "outputOrders",
                &vault.output_orders,
            )?,
        })
    }
}

impl RaindexVaultToken {
    fn try_from_sg_erc20(chain_id: u32, erc20: SgErc20) -> Result<Self, RaindexError> {
        let address = Address::from_str(&erc20.address.0)?;
        let decimals = erc20
            .decimals
            .ok_or(RaindexError::MissingErc20Decimals(address.to_string()))?
            .0
            .parse::<u8>()?;
        Ok(Self {
            chain_id,
            id: erc20.id.0,
            address,
            name: erc20.name,
            symbol: erc20.symbol,
            decimals,
        })
    }

    pub(crate) fn from_local_db_token(
        token: crate::local_db::query::fetch_all_tokens::LocalDbToken,
    ) -> Self {
        Self {
            chain_id: token.chain_id,
            id: token.token_address.to_string(),
            address: token.token_address,
            name: Some(token.name),
            symbol: Some(token.symbol),
            decimals: token.decimals,
        }
    }
}
impl TryFrom<RaindexVaultToken> for SgErc20 {
    type Error = RaindexError;
    fn try_from(token: RaindexVaultToken) -> Result<Self, Self::Error> {
        Ok(Self {
            id: SgBytes(token.id),
            address: SgBytes(token.address.to_string()),
            name: token.name,
            symbol: token.symbol,
            decimals: Some(SgBigInt(token.decimals.to_string())),
        })
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[cfg(target_family = "wasm")]
    mod wasm_tests {
        use super::*;
        use crate::local_db::query::fetch_vault_balance_changes::LocalDbVaultBalanceChange;
        use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
        use crate::raindex_client::tests::{
            get_local_db_test_yaml, local_db_object_from_query_callback,
            new_test_client_with_local_db,
        };
        use alloy::primitives::{address, b256, Address, Bytes};
        use rain_math_float::Float;
        use serde_json;
        use std::cell::RefCell;
        use std::rc::Rc;
        use std::str::FromStr;
        use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
        use wasm_bindgen_test::wasm_bindgen_test;
        use wasm_bindgen_utils::prelude::WasmEncodedResult;
        use LocalDbVault;

        fn make_local_db_vaults_callback(vaults: Vec<LocalDbVault>) -> js_sys::Function {
            let vaults_count = vaults.len();
            let json = serde_json::to_string(&vaults).unwrap();
            let result = WasmEncodedResult::Success::<String> {
                value: json,
                error: None,
            };
            let payload = js_sys::JSON::stringify(&serde_wasm_bindgen::to_value(&result).unwrap())
                .unwrap()
                .as_string()
                .unwrap();

            let callback = Closure::wrap(Box::new(move |sql: String| -> JsValue {
                if sql.contains("vaults_count") {
                    serde_wasm_bindgen::to_value(&WasmEncodedResult::success(format!(
                        "[{{\"vaults_count\":{vaults_count}}}]"
                    )))
                    .unwrap()
                } else {
                    js_sys::JSON::parse(&payload).unwrap()
                }
            }) as Box<dyn Fn(String) -> JsValue>);

            callback.into_js_value().dyn_into().unwrap()
        }

        fn make_local_db_vaults_with_balance_changes_callback(
            vaults: Vec<LocalDbVault>,
            balance_changes: Vec<LocalDbVaultBalanceChange>,
        ) -> js_sys::Function {
            let vaults_json = serde_json::to_string(&vaults).unwrap();
            let vaults_result = WasmEncodedResult::Success::<String> {
                value: vaults_json,
                error: None,
            };
            let vaults_payload =
                js_sys::JSON::stringify(&serde_wasm_bindgen::to_value(&vaults_result).unwrap())
                    .unwrap()
                    .as_string()
                    .unwrap();

            let balance_json = serde_json::to_string(&balance_changes).unwrap();
            let balance_result = WasmEncodedResult::Success::<String> {
                value: balance_json,
                error: None,
            };
            let balance_payload =
                js_sys::JSON::stringify(&serde_wasm_bindgen::to_value(&balance_result).unwrap())
                    .unwrap()
                    .as_string()
                    .unwrap();

            let callback = Closure::wrap(Box::new(move |sql: String| -> JsValue {
                if sql.contains("runningBalance") {
                    js_sys::JSON::parse(&balance_payload).unwrap()
                } else {
                    js_sys::JSON::parse(&vaults_payload).unwrap()
                }
            }) as Box<dyn Fn(String) -> JsValue>);

            callback.into_js_value().dyn_into().unwrap()
        }

        fn make_local_vault(
            vault_id: &str,
            token: &str,
            owner: &str,
            balance: Float,
        ) -> LocalDbVault {
            LocalDbVault {
                chain_id: 42161,
                vault_id: U256::from_str(vault_id).unwrap(),
                token: Address::from_str(token).unwrap(),
                owner: Address::from_str(owner).unwrap(),
                raindex_address: address!("0x2f209e5b67A33B8fE96E28f24628dF6Da301c8eB"),
                token_name: "Token".to_string(),
                token_symbol: "TKN".to_string(),
                token_decimals: 18,
                balance: balance.as_hex(),
                input_orders: None,
                output_orders: None,
            }
        }

        #[wasm_bindgen_test]
        async fn test_get_vaults_local_db_path() {
            let owner = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            let token = "0x00000000000000000000000000000000000000aa";
            let vault =
                make_local_vault("0x01", token, owner, Float::parse("1".to_string()).unwrap());

            let callback = make_local_db_vaults_callback(vec![vault]);

            let client = new_test_client_with_local_db(
                vec![get_local_db_test_yaml()],
                local_db_object_from_query_callback(callback),
                vec![42161],
            );

            let vaults = client
                .get_vaults(Some(ChainIds(vec![42161])), None, None, None)
                .await
                .expect("local db vaults should load");

            let items = vaults.items();
            assert_eq!(items.len(), 1);
            let result_vault = &items[0];
            assert_eq!(result_vault.chain_id(), 42161);
            assert_eq!(result_vault.owner().to_lowercase(), owner.to_string());
            assert_eq!(
                result_vault.raindex().to_lowercase(),
                "0x2f209e5b67a33b8fe96e28f24628df6da301c8eb".to_string()
            );
            assert_eq!(result_vault.formatted_balance(), "1".to_string());
            let token_meta = result_vault.token();
            assert_eq!(token_meta.address().to_lowercase(), token.to_string());
        }

        #[wasm_bindgen_test]
        async fn test_get_vault_local_db_path() {
            let owner = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            let token = "0x00000000000000000000000000000000000000aa";
            let local_vault =
                make_local_vault("0x02", token, owner, Float::parse("5".to_string()).unwrap());

            let callback = make_local_db_vaults_callback(vec![local_vault.clone()]);

            let client = new_test_client_with_local_db(
                vec![get_local_db_test_yaml()],
                local_db_object_from_query_callback(callback),
                vec![42161],
            );

            let rc_client = Rc::new(client.clone());
            let derived_vault =
                RaindexVault::try_from_local_db(Rc::clone(&rc_client), local_vault, None)
                    .expect("local vault should convert");

            let vault_id_hex = derived_vault.id();
            let vault_id_bytes = Bytes::from_str(&vault_id_hex).expect("valid vault id");

            let raindex_addr =
                Address::from_str("0x2f209e5b67A33B8fE96E28f24628dF6Da301c8eB").unwrap();
            let retrieved = client
                .get_vault(&RaindexIdentifier::new(42161, raindex_addr), vault_id_bytes)
                .await
                .expect("local vault retrieval should succeed");

            assert_eq!(retrieved.chain_id(), 42161);
            assert_eq!(retrieved.owner().to_lowercase(), owner.to_string());
            assert_eq!(retrieved.formatted_balance(), "5".to_string());
            assert_eq!(
                retrieved.token().address().to_lowercase(),
                token.to_string()
            );
            assert_eq!(retrieved.id(), vault_id_hex);
        }

        #[wasm_bindgen_test]
        async fn test_get_balance_changes_local_db_path() {
            let owner = address!("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
            let token = address!("0x00000000000000000000000000000000000000aa");
            let owner_str = owner.to_string();
            let token_str = token.to_string();
            let local_vault = make_local_vault(
                "0x02",
                &token_str,
                &owner_str,
                Float::parse("5".to_string()).unwrap(),
            );

            let amount = Float::parse("1".to_string()).unwrap();
            let running_balance = Float::parse("5".to_string()).unwrap();
            let transaction_sender = address!("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");

            let balance_change = LocalDbVaultBalanceChange {
                transaction_hash: b256!(
                    "0x00000000000000000000000000000000000000000000000000000000deadbeef"
                ),
                log_index: 1,
                block_number: 1234,
                block_timestamp: 5678,
                owner,
                transaction_sender,
                change_type: "DEPOSIT".to_string(),
                token,
                vault_id: local_vault.vault_id.clone(),
                delta: amount.as_hex(),
                running_balance: running_balance.as_hex(),
            };

            let callback = make_local_db_vaults_with_balance_changes_callback(
                vec![local_vault.clone()],
                vec![balance_change],
            );

            let client = new_test_client_with_local_db(
                vec![get_local_db_test_yaml()],
                local_db_object_from_query_callback(callback),
                vec![42161],
            );

            let rc_client = Rc::new(client.clone());
            let derived_vault =
                RaindexVault::try_from_local_db(Rc::clone(&rc_client), local_vault, None)
                    .expect("local vault should convert");

            let vault_id_bytes = Bytes::from_str(&derived_vault.id()).expect("valid vault id");

            let raindex_addr =
                Address::from_str("0x2f209e5b67A33B8fE96E28f24628dF6Da301c8eB").unwrap();
            let vault = client
                .get_vault(&RaindexIdentifier::new(42161, raindex_addr), vault_id_bytes)
                .await
                .expect("local vault retrieval should succeed");

            let changes = vault
                .get_balance_changes(None, None)
                .await
                .expect("balance changes should load from local db");

            assert_eq!(changes.len(), 1);
            let change = &changes[0];
            assert_eq!(change.type_getter(), RaindexVaultBalanceChangeType::Deposit);
            assert_eq!(change.formatted_amount(), "1");
            assert_eq!(change.formatted_new_balance(), "5");
            assert_eq!(change.formatted_old_balance(), "4");
            assert_eq!(
                change.transaction().id(),
                "0x00000000000000000000000000000000000000000000000000000000deadbeef"
            );
            assert_eq!(
                change.transaction().from().to_lowercase(),
                transaction_sender.to_string().to_lowercase()
            );
        }

        #[wasm_bindgen_test]
        async fn test_get_vaults_local_db_filters() {
            use wasm_bindgen::JsCast;
            use wasm_bindgen_utils::prelude::JsValue;
            let owner_kept = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            let token_kept = "0x00000000000000000000000000000000000000aa";

            let keep_vault = make_local_vault(
                "0x01",
                token_kept,
                owner_kept,
                Float::parse("2".to_string()).unwrap(),
            );
            let captured_sql = Rc::new(RefCell::new((String::new(), JsValue::UNDEFINED)));
            let json = serde_json::to_string(&vec![keep_vault]).unwrap();
            let callback = create_sql_capturing_callback(&json, captured_sql.clone());
            let callback = js_sys::Function::new_with_args(
                "callback",
                "return (sql, params) => sql.includes('vaults_count') \
                 ? { value: '[{\"vaults_count\":1}]', error: null } \
                 : callback(sql, params);",
            )
            .call1(&JsValue::UNDEFINED, &callback)
            .unwrap()
            .unchecked_into();

            let client = new_test_client_with_local_db(
                vec![get_local_db_test_yaml()],
                local_db_object_from_query_callback(callback),
                vec![42161],
            );

            let filters = GetVaultsFilters {
                owners: vec![Address::from_str(owner_kept).unwrap()],
                hide_zero_balance: true,
                tokens: Some(vec![Address::from_str(token_kept).unwrap()]),
                raindex_addresses: None,
                only_active_orders: false,
            };

            let vaults = client
                .get_vaults(Some(ChainIds(vec![42161])), Some(filters), None, None)
                .await
                .expect("filtered vaults should load");

            let items = vaults.items();
            assert_eq!(items.len(), 1);
            let vault = &items[0];
            assert_eq!(vault.owner().to_lowercase(), owner_kept.to_string());
            let token_meta = vault.token();
            assert_eq!(token_meta.address().to_lowercase(), token_kept.to_string());
            assert_eq!(vault.formatted_balance(), "2".to_string());

            let sql = captured_sql.borrow();
            // SQL should contain parameterized IN-clauses and hide-zero filter body
            assert!(sql.0.contains("o.owner IN ("));
            assert!(sql.0.contains("o.token IN ("));
            assert!(sql.0.contains("AND NOT FLOAT_IS_ZERO("));

            // Params should include chain id, owner and token values bound in order
            let params_js = sql.1.clone();
            assert!(
                js_sys::Array::is_array(&params_js),
                "expected array params from callback"
            );
            let params_array = js_sys::Array::from(&params_js);
            assert!(
                params_array.length() >= 3,
                "expected at least three params (chain id, owner, token)"
            );

            // Expect first param to be chain id (U64 encoded as BigInt)
            let chain_id = params_array.get(0);
            let chain_id_bigint = chain_id
                .dyn_into::<js_sys::BigInt>()
                .expect("chain id should be BigInt");
            let chain_id_str = chain_id_bigint.to_string(10).unwrap().as_string().unwrap();
            assert_eq!(chain_id_str, "42161");

            // Expect owner and token to be present among text params
            let mut has_owner = false;
            let mut has_token = false;
            for value in params_array.iter() {
                if let Some(text) = value.as_string() {
                    if text == owner_kept {
                        has_owner = true;
                    }
                    if text == token_kept {
                        has_token = true;
                    }
                }
            }
            assert!(has_owner, "owner missing in params");
            assert!(has_token, "token missing in params");
        }
    }

    #[cfg(not(target_family = "wasm"))]
    mod non_wasm {
        use super::*;
        use crate::local_db::query::{
            fetch_vaults::LocalDbVaultsCountRow, FromDbJson, LocalDbQueryError,
            LocalDbQueryExecutor, SqlStatement, SqlStatementBatch,
        };
        use crate::raindex_client::local_db::LocalDb;
        use crate::raindex_client::tests::{
            get_test_yaml, new_with_local_db, CHAIN_ID_1_RAINDEX_ADDRESS,
        };
        use alloy::hex::encode_prefixed;
        use alloy::primitives::{address, b256};
        use alloy::sol_types::SolCall;
        use httpmock::MockServer;
        use raindex_bindings::{
            IRaindexV7::{deposit5Call, withdraw5Call},
            IERC20::{allowanceCall, approveCall},
        };
        use raindex_subgraph_client::utils::float::*;
        use serde_json::{json, Value};
        use std::sync::Arc;
        use LocalDbVault;

        #[derive(Clone)]
        struct StaticVaultDbExec {
            vaults: Vec<LocalDbVault>,
        }

        #[async_trait::async_trait]
        impl LocalDbQueryExecutor for StaticVaultDbExec {
            async fn execute_batch(&self, _: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
                Ok(())
            }

            async fn query_json<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
            where
                T: FromDbJson,
            {
                let value = if stmt.sql.contains("vaults_count") {
                    serde_json::to_value(vec![LocalDbVaultsCountRow {
                        vaults_count: self.vaults.len() as u32,
                    }])
                } else {
                    serde_json::to_value(&self.vaults)
                }
                .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))?;

                serde_json::from_value(value)
                    .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))
            }

            async fn query_text(&self, _: &SqlStatement) -> Result<String, LocalDbQueryError> {
                Ok(String::new())
            }

            async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
                Ok(())
            }
        }

        fn make_native_local_vault(vault_id: u64, token: &str) -> LocalDbVault {
            LocalDbVault {
                chain_id: 1,
                vault_id: U256::from(vault_id),
                token: Address::from_str(token).unwrap(),
                owner: address!("0x0000000000000000000000000000000000000000"),
                raindex_address: Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                token_name: "Local Token".to_string(),
                token_symbol: "LTKN".to_string(),
                token_decimals: 18,
                balance: F1.as_hex(),
                input_orders: None,
                output_orders: None,
            }
        }

        #[test]
        fn test_try_from_local_trade_side_with_running_balance() {
            let chain_id = 42161;
            let raindex_addr = address!("0x0000000000000000000000000000000000000001");
            let transaction = RaindexTransaction::from_local_parts(
                b256!("0x00000000000000000000000000000000000000000000000000000000deadbeef"),
                address!("0x0000000000000000000000000000000000000002"),
                123,
                456,
            )
            .unwrap();

            let amount = Float::parse("1".to_string()).unwrap();
            let amount_hex = amount.as_hex();
            let new_balance = Float::parse("5".to_string()).unwrap();
            let new_balance_hex = new_balance.as_hex();
            let expected_old_balance = Float::parse("4".to_string()).unwrap();

            let change = RaindexVaultBalanceChange::try_from_local_trade_side(
                chain_id,
                raindex_addr,
                &transaction,
                U256::from(16),
                LocalTradeTokenInfo {
                    address: address!("0x0000000000000000000000000000000000000003"),
                    name: Some("Token In".to_string()),
                    symbol: Some("TIN".to_string()),
                    decimals: Some(6),
                },
                LocalTradeBalanceInfo {
                    delta: amount_hex.clone(),
                    running_balance: Some(new_balance_hex.clone()),
                    trade_kind: "take".to_string(),
                },
                789,
            )
            .unwrap();

            assert_eq!(change.r#type(), RaindexVaultBalanceChangeType::TakeOrder);
            assert_eq!(change.vault_id(), U256::from_str("0x10").unwrap());
            assert!(change.amount().eq(amount).unwrap());
            assert!(change.new_balance().eq(new_balance).unwrap());
            assert_eq!(change.formatted_amount(), amount.format().unwrap());
            assert_eq!(
                change.formatted_new_balance(),
                new_balance.format().unwrap()
            );
            assert_eq!(
                change.formatted_old_balance(),
                expected_old_balance.format().unwrap()
            );
            assert_eq!(change.timestamp(), U256::from(789));
            assert_eq!(change.raindex(), raindex_addr);
            assert_eq!(change.transaction().id(), transaction.id());

            let token = change.token();
            assert_eq!(token.chain_id(), chain_id);
            assert_eq!(
                token.address(),
                Address::from_str("0x0000000000000000000000000000000000000003").unwrap()
            );
            assert_eq!(token.decimals(), 6);
            assert_eq!(token.name(), Some("Token In".to_string()));
            assert_eq!(token.symbol(), Some("TIN".to_string()));
            assert_eq!(
                token.id(),
                "0x0000000000000000000000000000000000000003".to_string()
            );
        }

        #[test]
        fn test_try_from_local_trade_side_defaults() {
            let chain_id = 1;
            let raindex_addr = address!("0x0000000000000000000000000000000000000004");
            let transaction = RaindexTransaction::from_local_parts(
                b256!("0x00000000000000000000000000000000000000000000000000000000feedface"),
                address!("0x0000000000000000000000000000000000000005"),
                111,
                222,
            )
            .unwrap();

            let amount = Float::parse("2".to_string()).unwrap();
            let amount_hex = amount.as_hex();
            let zero = Float::parse("0".to_string()).unwrap();

            let change = RaindexVaultBalanceChange::try_from_local_trade_side(
                chain_id,
                raindex_addr,
                &transaction,
                U256::from(2),
                LocalTradeTokenInfo {
                    address: address!("0x0000000000000000000000000000000000000006"),
                    name: None,
                    symbol: None,
                    decimals: None,
                },
                LocalTradeBalanceInfo {
                    delta: amount_hex.clone(),
                    running_balance: None,
                    trade_kind: "clear".to_string(),
                },
                333,
            )
            .unwrap();

            assert_eq!(change.r#type(), RaindexVaultBalanceChangeType::Clear);

            assert!(change.amount().eq(amount).unwrap());
            assert!(change.new_balance().eq(amount).unwrap());
            assert!(change.old_balance().eq(zero).unwrap());
            assert_eq!(change.formatted_amount(), amount.format().unwrap());
            assert_eq!(change.formatted_new_balance(), amount.format().unwrap());
            assert_eq!(change.formatted_old_balance(), zero.format().unwrap());

            let token = change.token();
            assert_eq!(token.decimals(), 18);
            assert!(token.name().is_none());
            assert!(token.symbol().is_none());
            assert_eq!(
                token.address(),
                Address::from_str("0x0000000000000000000000000000000000000006").unwrap()
            );
            assert_eq!(
                token.id(),
                "0x0000000000000000000000000000000000000006".to_string()
            );
        }

        #[tokio::test]
        async fn test_try_from_local_db_maps_token_metadata() {
            // Build a minimal client; it won't be used in mapping
            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    "http://sg1",
                    "http://sg2",
                    "http://rpc1",
                    "http://rpc2",
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let local_vault = LocalDbVault {
                chain_id: 1,
                vault_id: U256::from(1),
                token: address!("0x0000000000000000000000000000000000000000"),
                owner: address!("0x0000000000000000000000000000000000000000"),
                raindex_address: Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                token_name: "Test Token".to_string(),
                token_symbol: "TST".to_string(),
                token_decimals: 6,
                balance: Float::parse("0".to_string()).unwrap().as_hex(),
                input_orders: None,
                output_orders: None,
            };

            let rv = RaindexVault::try_from_local_db(
                Arc::new(raindex_client),
                local_vault,
                Some(RaindexVaultType::Input),
            )
            .unwrap();

            assert_eq!(rv.token.name(), Some("Test Token".to_string()));
            assert_eq!(rv.token.symbol(), Some("TST".to_string()));
            assert_eq!(rv.token.decimals(), 6);
        }

        fn get_vault1_json() -> Value {
            json!({
              "id": "0x0123",
              "owner": "0x0000000000000000000000000000000000000000",
              "vaultId": "0x0123",
              "balance": F1,
              "token": {
                "id": "token1",
                "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                "name": "Token 1",
                "symbol": "TKN1",
                "decimals": "18"
              },
              "raindex": {
                "id": CHAIN_ID_1_RAINDEX_ADDRESS
              },
              "ordersAsOutput": [],
              "ordersAsInput": [],
              "balanceChanges": []
            })
        }

        fn get_vault2_json() -> Value {
            json!({
                "id": "0x0234",
                "owner": "0x0000000000000000000000000000000000000000",
                "vaultId": "0x0234",
                "balance": F2,
                "token": {
                    "id": "token2",
                    "address": "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                    "name": "Token 2",
                    "symbol": "TKN2",
                    "decimals": "18"
                },
                "raindex": {
                    "id": "0x0000000000000000000000000000000000000000"
                },
                "ordersAsOutput": [],
                "ordersAsInput": [],
                "balanceChanges": []
            })
        }

        #[tokio::test]
        async fn test_get_vaults() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault2_json()]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    // not used
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let result = raindex_client
                .get_vaults(None, None, None, None)
                .await
                .unwrap()
                .items();
            assert_eq!(result.len(), 2);

            let vault1 = result[0].clone();
            assert_eq!(vault1.chain_id, 1);
            assert_eq!(vault1.id, Bytes::from_str("0x0123").unwrap());
            assert_eq!(
                vault1.owner,
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(vault1.vault_id, U256::from_str("0x0123").unwrap());
            assert!(vault1.balance.eq(F1).unwrap());
            assert_eq!(vault1.formatted_balance, "1");
            assert_eq!(vault1.token.id, "token1");
            assert_eq!(
                vault1.raindex,
                Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap()
            );

            let vault2 = result[1].clone();
            assert_eq!(vault2.chain_id, 137);
            assert_eq!(vault2.id, Bytes::from_str("0x0234").unwrap());
            assert_eq!(
                vault2.owner,
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(vault2.vault_id, U256::from_str("0x0234").unwrap());
            assert!(vault2.balance.eq(F2).unwrap());
            assert_eq!(vault2.formatted_balance, "2");
            assert_eq!(vault2.token.id, "token2");
            assert_eq!(
                vault2.raindex,
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
        }

        #[tokio::test]
        async fn test_get_vaults_returns_metadata_and_uses_page_size() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":200");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json(), get_vault2_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"skip\":1")
                    .body_contains("\"first\":1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault2_json()]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let result = raindex_client
                .get_vaults(Some(ChainIds(vec![1])), None, Some(2), Some(1))
                .await
                .unwrap();

            assert_eq!(result.page(), 2);
            assert_eq!(result.page_size(), 1);
            assert_eq!(result.total_items(), 2);
            assert!(!result.has_more());
            let items = result.vaults().items();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].id, Bytes::from_str("0x0234").unwrap());
        }

        #[tokio::test]
        async fn test_get_vaults_has_more() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":200");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json(), get_vault2_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json()]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let result = raindex_client
                .get_vaults(Some(ChainIds(vec![1])), None, Some(1), Some(1))
                .await
                .unwrap();

            assert_eq!(result.total_items(), 2);
            assert!(result.has_more());
            assert_eq!(result.vaults().items().len(), 1);
        }

        #[tokio::test]
        async fn test_get_vaults_multiple_subgraphs_slices_after_merge() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":200");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":200");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault2_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":100");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":100");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault2_json()]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let result = raindex_client
                .get_vaults(None, None, Some(1), Some(1))
                .await
                .unwrap();

            assert_eq!(result.total_items(), 2);
            assert!(result.has_more());
            assert_eq!(result.vaults().items().len(), 1);
        }

        #[tokio::test]
        async fn test_get_vaults_subgraph_list_errors_if_any_source_errors() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":200");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":200");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": []
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":100");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":100");
                then.status(500);
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let result = raindex_client
                .get_vaults(None, None, Some(1), Some(1))
                .await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_get_vaults_mixed_sources_slices_after_merge() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg2")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":200");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault2_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2")
                    .body_contains("\"skip\":0")
                    .body_contains("\"first\":100");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault2_json()]
                    }
                }));
            });

            let local_db = LocalDb::new(StaticVaultDbExec {
                vaults: vec![
                    make_native_local_vault(1, "0x0000000000000000000000000000000000000001"),
                    make_native_local_vault(2, "0x0000000000000000000000000000000000000002"),
                ],
            });
            let raindex_client = new_with_local_db(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                local_db,
                vec![1],
            )
            .await;

            let result = raindex_client
                .get_vaults(None, None, Some(1), Some(2))
                .await
                .unwrap();

            assert_eq!(result.page(), 1);
            assert_eq!(result.page_size(), 2);
            assert_eq!(result.total_items(), 3);
            assert!(result.has_more());
            let items = result.vaults().items();
            assert_eq!(items.len(), 2);
            assert!(items.iter().all(|vault| vault.chain_id() == 1));
        }

        #[tokio::test]
        async fn test_get_vault_totals_aggregates_and_skips_zero_balances() {
            let sg_server = MockServer::start_async().await;
            let mut zero_vault = get_vault2_json();
            zero_vault["balance"] = Value::String(F0.as_hex());

            let count_mock = sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"balance_not\"")
                    .body_contains("\"first\":200");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json(), zero_vault.clone()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"balance_not\"")
                    .body_contains("\"first\":1000");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json(), zero_vault]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let totals = raindex_client
                .get_vault_totals(Some(ChainIds(vec![1])))
                .await
                .unwrap();

            assert_eq!(totals.len(), 1);
            assert_eq!(count_mock.hits(), 0);
            assert_eq!(
                totals[0].token().address(),
                Address::from_str("0x1d80c49bbbcd1c0911346656b529df9e5c2f783d").unwrap()
            );
            assert!(totals[0].balance().eq(F1).unwrap());
            assert_eq!(totals[0].balance_hex(), F1.as_hex());
            assert_eq!(totals[0].formatted_balance(), "1");
        }

        #[tokio::test]
        async fn test_get_vault() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vault": get_vault1_json()
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    // not used
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let vault = raindex_client
                .get_vault(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    Bytes::from_str("0x10").unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(vault.chain_id, 1);
            assert_eq!(vault.id, Bytes::from_str("0x0123").unwrap());
            assert_eq!(
                vault.owner,
                Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
            );
            assert_eq!(vault.vault_id, U256::from_str("0x0123").unwrap());

            assert!(
                vault.balance.eq(F1).unwrap(),
                "unexpected balance: {}",
                vault.balance.format().unwrap()
            );
            assert_eq!(vault.formatted_balance, "1");

            assert_eq!(vault.token.id, "token1");
            assert_eq!(
                vault.raindex,
                Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap()
            );
        }

        #[tokio::test]
        async fn test_get_vault_missing_decimals() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vault": json!({
                            "id": "0x0123",
                            "owner": "0x0000000000000000000000000000000000000000",
                            "vaultId": "0x10",
                            "balance": "69862789",
                            "token": {
                                "id": "token1",
                                "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                "name": "Token 1",
                                "symbol": "TKN1",
                                "decimals": null // Missing decimals
                            },
                            "raindex": {
                                "id": CHAIN_ID_1_RAINDEX_ADDRESS
                            },
                            "ordersAsOutput": [],
                            "ordersAsInput": [],
                            "balanceChanges": []
                        })
                    }
                }));
            });
            // 6 decimals token info
            sg_server.mock(|when, then| {
                when.method("POST")
                    .path("/rpc1")
                    .body_contains("0x313ce567");
                then.body(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "result": "0x0000000000000000000000000000000000000000000000000000000000000006"
                    })
                    .to_string(),
                );
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    // not used
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let err = raindex_client
                .get_vault(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    Bytes::from_str("0x0123").unwrap(),
                )
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                RaindexError::MissingErc20Decimals(token)
                if token == "0x1D80c49BbBCd1C0911346656B529DF9E5c2F783d"
            ));
        }

        #[tokio::test]
        async fn test_get_vault_balance_changes() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1").body_contains("SgVaultDetailQuery");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vault": get_vault1_json()
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("SgVaultBalanceChangesListQuery")
                    .body_contains("\"skip\":0");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaultBalanceChanges": [
                            {
                                "id": "0xdeposit001",
                                "__typename": "Deposit",
                                "amount": F5,
                                "newVaultBalance": F5,
                                "oldVaultBalance": F0,
                                "vault": {
                                    "id": "0x166aeed725f0f3ef9fe62f2a9054035756d55e5560b17afa1ae439e9cd362902",
                                    "vaultId": "1",
                                    "token": {
                                        "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                        "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                        "name": "Wrapped Flare",
                                        "symbol": "WFLR",
                                        "decimals": "18"
                                    }
                                },
                                "timestamp": "1734054063",
                                "transaction": {
                                    "id": "0x85857b5c6d0b277f9e971b6b45cab98720f90b8f24d65df020776d675b71fc22",
                                    "from": "0x7177b9d00bb5dbcaaf069cc63190902763783b09",
                                    "blockNumber": "34407047",
                                    "timestamp": "1734054063"
                                },
                                "raindex": {
                                    "id": "0xcee8cd002f151a536394e564b84076c41bbbcd4d"
                                }
                            }
                        ]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("SgVaultBalanceChangesListQuery")
                    .body_contains("\"skip\":200");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaultBalanceChanges": []
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    // not used
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let vault = raindex_client
                .get_vault(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    Bytes::from_str("0x0123").unwrap(),
                )
                .await
                .unwrap();
            let result = vault.get_balance_changes(None, None).await.unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].r#type, RaindexVaultBalanceChangeType::Deposit);
            assert_eq!(result[0].vault_id, U256::from_str("1").unwrap());
            assert_eq!(
                result[0].token.id,
                "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d"
            );
            assert_eq!(
                result[0].token.address,
                Address::from_str("0x1d80c49bbbcd1c0911346656b529df9e5c2f783d").unwrap()
            );
            assert_eq!(result[0].token.name, Some("Wrapped Flare".to_string()));
            assert_eq!(result[0].token.symbol, Some("WFLR".to_string()));
            assert_eq!(result[0].token.decimals, 18);
            assert!(result[0].amount.eq(F5).unwrap());
            assert_eq!(result[0].formatted_amount, "5");
            assert!(result[0].new_balance.eq(F5).unwrap());
            assert_eq!(result[0].formatted_new_balance, "5");
            assert!(result[0].old_balance.eq(F0).unwrap());
            assert_eq!(result[0].formatted_old_balance, "0");
            assert_eq!(result[0].timestamp, U256::from_str("1734054063").unwrap());
            assert_eq!(
                result[0].transaction.id(),
                b256!("0x85857b5c6d0b277f9e971b6b45cab98720f90b8f24d65df020776d675b71fc22")
            );
            assert_eq!(
                result[0].transaction.from(),
                Address::from_str("0x7177b9d00bB5dbcaaF069CC63190902763783b09").unwrap()
            );
            assert_eq!(result[0].transaction.block_number(), U256::from(34407047));
            assert_eq!(result[0].transaction.timestamp(), U256::from(1734054063));
            assert_eq!(
                result[0].raindex,
                Address::from_str("0xcee8cd002f151a536394e564b84076c41bbbcd4d").unwrap()
            );
        }

        #[tokio::test]
        async fn test_formatted_balance_with_different_decimals() {
            let vault_6_decimals_json = json!({
                "id": "0x0456",
                "owner": "0x0000000000000000000000000000000000000000",
                "vaultId": "0x30",
                "balance": F1_5,
                "token": {
                    "id": "token_usdc",
                    "address": "0xa0b86a33e6c3a0e4e8c7b6c6b0c2f6a3b7e8d9e0",
                    "name": "USD Coin",
                    "symbol": "USDC",
                    "decimals": "6"
                },
                "raindex": {
                    "id": CHAIN_ID_1_RAINDEX_ADDRESS
                },
                "ordersAsOutput": [],
                "ordersAsInput": [],
                "balanceChanges": []
            });

            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vault": vault_6_decimals_json
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let vault = raindex_client
                .get_vault(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    Bytes::from_str("0x0456").unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(vault.formatted_balance, "1.5");
            assert!(vault.balance.eq(F1_5).unwrap());
        }

        #[tokio::test]
        async fn test_formatted_balance_change_with_negative_amount() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1").body_contains("SgVaultDetailQuery");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vault": get_vault1_json()
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("SgVaultBalanceChangesListQuery")
                    .body_contains("\"skip\":0");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaultBalanceChanges": [
                            {
                                "id": "0xwithdrawal001",
                                "__typename": "Withdrawal",
                                "amount": NEG2,
                                "newVaultBalance": F3,
                                "oldVaultBalance": F5,
                                "vault": {
                                    "id": "0x166aeed725f0f3ef9fe62f2a9054035756d55e5560b17afa1ae439e9cd362902",
                                    "vaultId": "1",
                                    "token": {
                                        "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                        "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                        "name": "Wrapped Ether",
                                        "symbol": "WETH",
                                        "decimals": "18"
                                    }
                                },
                                "timestamp": "1734054063",
                                "transaction": {
                                    "id": "0x85857b5c6d0b277f9e971b6b45cab98720f90b8f24d65df020776d675b71fc22",
                                    "from": "0x7177b9d00bb5dbcaaf069cc63190902763783b09",
                                    "blockNumber": "34407047",
                                    "timestamp": "1734054063"
                                },
                                "raindex": {
                                    "id": "0xcee8cd002f151a536394e564b84076c41bbbcd4d"
                                }
                            }
                        ]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("SgVaultBalanceChangesListQuery")
                    .body_contains("\"skip\":200");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaultBalanceChanges": []
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let vault = raindex_client
                .get_vault(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    Bytes::from_str("0x0123").unwrap(),
                )
                .await
                .unwrap();
            let result = vault.get_balance_changes(None, None).await.unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].r#type, RaindexVaultBalanceChangeType::Withdrawal);

            assert!(result[0].amount.eq(NEG2).unwrap());
            assert_eq!(result[0].formatted_amount, "-2");

            assert!(result[0].old_balance.eq(F5).unwrap());
            assert_eq!(result[0].formatted_old_balance, "5");

            assert!(result[0].new_balance.eq(F3).unwrap());
            assert_eq!(result[0].formatted_new_balance, "3");
        }

        #[tokio::test]
        async fn test_missing_decimals_formatted_balance() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1").body_contains("SgVaultDetailQuery");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vault": get_vault1_json()
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("SgVaultBalanceChangesListQuery")
                    .body_contains("\"skip\":0");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaultBalanceChanges": [
                            {
                                "id": "0xwithdrawal002",
                                "__typename": "Withdrawal",
                                "amount": "-25354",
                                "newVaultBalance": "3378982",
                                "oldVaultBalance": "50008796",
                                "vault": {
                                    "id": "0x166aeed725f0f3ef9fe62f2a9054035756d55e5560b17afa1ae439e9cd362902",
                                    "vaultId": "1",
                                    "token": {
                                        "id": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                        "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                        "name": "Wrapped Ether",
                                        "symbol": "WETH",
                                        "decimals": null
                                    }
                                },
                                "timestamp": "1734054063",
                                "transaction": {
                                    "id": "0x85857b5c6d0b277f9e971b6b45cab98720f90b8f24d65df020776d675b71fc22",
                                    "from": "0x7177b9d00bb5dbcaaf069cc63190902763783b09",
                                    "blockNumber": "34407047",
                                    "timestamp": "1734054063"
                                },
                                "raindex": {
                                    "id": "0xcee8cd002f151a536394e564b84076c41bbbcd4d"
                                }
                            }
                        ]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("SgVaultBalanceChangesListQuery")
                    .body_contains("\"skip\":200");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaultBalanceChanges": []
                    }
                }));
            });
            // 6 decimals token info
            sg_server.mock(|when, then| {
                when.method("POST")
                    .path("/rpc1")
                    .body_contains("0x313ce567");
                then.body(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "result": "0x0000000000000000000000000000000000000000000000000000000000000006"
                    })
                    .to_string(),
                );
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let vault = raindex_client
                .get_vault(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    Bytes::from_str("0x0123").unwrap(),
                )
                .await
                .unwrap();
            let err = vault.get_balance_changes(None, None).await.unwrap_err();
            assert!(matches!(
                err,
                RaindexError::MissingErc20Decimals(token)
                if token == "0x1D80c49BbBCd1C0911346656B529DF9E5c2F783d"
            ));
        }

        #[tokio::test]
        async fn test_get_vault_calldatas() {
            let rpc_server = MockServer::start_async().await;
            rpc_server.mock(|when, then| {
                when.path("/rpc1");
                then.status(200).json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x0000000000000000000000000000000000000000000000056BC75E2D63100000",
                }));
            });

            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vault": get_vault1_json()
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &rpc_server.url("/rpc1"),
                    &rpc_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let vault = raindex_client
                .get_vault(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    Bytes::from_str("0x0123").unwrap(),
                )
                .await
                .unwrap();

            let token = Address::from_str("0x1d80c49bbbcd1c0911346656b529df9e5c2f783d").unwrap();
            let vault_id = B256::from(U256::from_str("0x0123").unwrap());

            // The on-chain allowance is 100 tokens, so an amount of 600 needs approval.
            let amount = Float::parse("600".to_string()).unwrap();
            let result = vault.get_calldatas(&amount).await.unwrap();
            assert_eq!(
                result.approval,
                Some(Bytes::copy_from_slice(
                    &approveCall {
                        spender: Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                        amount: U256::from(600000000000000000000u128),
                    }
                    .abi_encode(),
                ))
            );
            assert_eq!(
                result.deposit,
                Bytes::copy_from_slice(
                    &deposit5Call {
                        token,
                        vaultId: vault_id,
                        depositAmount: amount.get_inner(),
                        tasks: vec![],
                    }
                    .abi_encode()
                )
            );
            assert_eq!(
                result.withdraw,
                Bytes::copy_from_slice(
                    &withdraw5Call {
                        token,
                        vaultId: vault_id,
                        targetAmount: amount.get_inner(),
                        tasks: vec![],
                    }
                    .abi_encode()
                )
            );

            // An amount of 90 is already covered by the 100 token allowance, so no approval
            // calldata is produced (`approval` is `None` rather than an error).
            let amount = Float::parse("90".to_string()).unwrap();
            let result = vault.get_calldatas(&amount).await.unwrap();
            assert_eq!(result.approval, None);
            assert_eq!(
                result.deposit,
                Bytes::copy_from_slice(
                    &deposit5Call {
                        token,
                        vaultId: vault_id,
                        depositAmount: amount.get_inner(),
                        tasks: vec![],
                    }
                    .abi_encode()
                )
            );
            assert_eq!(
                result.withdraw,
                Bytes::copy_from_slice(
                    &withdraw5Call {
                        token,
                        vaultId: vault_id,
                        targetAmount: amount.get_inner(),
                        tasks: vec![],
                    }
                    .abi_encode()
                )
            );

            // Zero and negative amounts are rejected.
            let err = vault
                .get_calldatas(&Float::parse("0".to_string()).unwrap())
                .await
                .unwrap_err();
            assert_eq!(err.to_string(), RaindexError::ZeroAmount.to_string());

            let err = vault
                .get_calldatas(&Float::parse("-1".to_string()).unwrap())
                .await
                .unwrap_err();
            assert_eq!(err.to_string(), RaindexError::NegativeAmount.to_string());
        }

        #[tokio::test]
        async fn test_check_vault_allowance() {
            let rpc_server = MockServer::start_async().await;
            rpc_server.mock(|when, then| {
                when.path("/rpc1");
                then.status(200).json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x0000000000000000000000000000000000000000000000000000000000000001",
                }));
            });

            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vault": get_vault1_json()
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &rpc_server.url("/rpc1"),
                    &rpc_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let vault = raindex_client
                .get_vault(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    Bytes::from_str("0x0123").unwrap(),
                )
                .await
                .unwrap();
            let result = vault.get_allowance().await.unwrap();
            assert_eq!(result.0, U256::from(1));
        }

        // Helper: builds a `RaindexClient` + vault1 with a mocked subgraph and an
        // allowance RPC that only responds to a well-formed `allowance(owner,
        // spender)` `eth_call` for the vault token, returning `allowance_hex`.
        // The RPC mock matches on the exact ABI-encoded calldata, so if the
        // decoupled allowance path ever queried the wrong token, owner, or
        // spender the mock would not match and the read would fail.
        async fn vault1_with_allowance(
            allowance_hex: &str,
        ) -> (MockServer, MockServer, RaindexVault) {
            let owner = Address::from_str("0x0000000000000000000000000000000000000000").unwrap();
            let spender = Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap();
            let allowance_calldata = encode_prefixed(allowanceCall { owner, spender }.abi_encode());

            let rpc_server = MockServer::start_async().await;
            rpc_server.mock(|when, then| {
                when.path("/rpc1")
                    // Token (the ERC20 contract being read).
                    .body_contains("0x1d80c49bbbcd1c0911346656b529df9e5c2f783d")
                    // allowance(owner, spender) calldata.
                    .body_contains(&allowance_calldata);
                then.status(200).json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": allowance_hex,
                }));
            });

            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vault": get_vault1_json()
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &rpc_server.url("/rpc1"),
                    &rpc_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let vault = raindex_client
                .get_vault(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    Bytes::from_str("0x0123").unwrap(),
                )
                .await
                .unwrap();
            (rpc_server, sg_server, vault)
        }

        // `get_allowance` (via `read_allowance` / `get_transaction_args`) must
        // surface the *exact* on-chain allowance for distinct mocked values.
        // A refactor bug that returned a constant, the deposit amount, or a
        // truncated/zeroed value would survive a "1" assertion but fails here.
        #[tokio::test]
        async fn test_get_allowance_returns_distinct_values() {
            // allowance = 0
            let (_rpc, _sg, vault) = vault1_with_allowance(
                "0x0000000000000000000000000000000000000000000000000000000000000000",
            )
            .await;
            assert_eq!(vault.get_allowance().await.unwrap().0, U256::ZERO);

            // allowance = 250 * 1e18 (a partial, non-trivial amount)
            let (_rpc, _sg, vault) = vault1_with_allowance(
                "0x00000000000000000000000000000000000000000000000d8d726b7177a80000",
            )
            .await;
            assert_eq!(
                vault.get_allowance().await.unwrap().0,
                U256::from(250000000000000000000u128)
            );

            // allowance = u256::MAX (an "infinite"/large approval)
            let (_rpc, _sg, vault) = vault1_with_allowance(
                "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            )
            .await;
            assert_eq!(vault.get_allowance().await.unwrap().0, U256::MAX);
        }

        // The approval half of `get_calldatas` must produce an
        // `approve(spender, amount)` calldata for the raindex spender and the
        // requested amount whenever the current allowance is strictly below the
        // amount, regardless of the existing allowance level (0 vs. a partial
        // amount). Decodes the exact spender + amount, so a wrong spender or
        // amount encoding fails. The decoupled `build_approval_calldata` reads the
        // allowance with no `DepositArgs`, so this exercises the deposit-free path.
        #[tokio::test]
        async fn test_get_approval_calldata_insufficient_allowance() {
            // allowance = 0 -> approval needed for full requested amount.
            let (_rpc, _sg, vault) = vault1_with_allowance(
                "0x0000000000000000000000000000000000000000000000000000000000000000",
            )
            .await;
            let result = vault
                .get_calldatas(&Float::parse("600".to_string()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                result.approval,
                Some(Bytes::copy_from_slice(
                    &approveCall {
                        spender: Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                        amount: U256::from(600000000000000000000u128),
                    }
                    .abi_encode(),
                ))
            );

            // allowance = 250 * 1e18 (partial, still below 600) -> approval for
            // the full requested amount (the contract approves the target, not
            // the delta).
            let (_rpc, _sg, vault) = vault1_with_allowance(
                "0x00000000000000000000000000000000000000000000000d8d726b7177a80000",
            )
            .await;
            let result = vault
                .get_calldatas(&Float::parse("600".to_string()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                result.approval,
                Some(Bytes::copy_from_slice(
                    &approveCall {
                        spender: Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                        amount: U256::from(600000000000000000000u128),
                    }
                    .abi_encode(),
                ))
            );
        }

        // When the current allowance is >= the requested amount, the approval
        // half of `get_calldatas` must be `None` (no approval transaction needed).
        // Covers both the strictly-greater and the exactly-equal boundary.
        #[tokio::test]
        async fn test_get_approval_calldata_sufficient_allowance() {
            // allowance = 600 * 1e18, exactly equal to the requested amount.
            let (_rpc, _sg, vault) = vault1_with_allowance(
                "0x00000000000000000000000000000000000000000000002086ac351052600000",
            )
            .await;
            let result = vault
                .get_calldatas(&Float::parse("600".to_string()).unwrap())
                .await
                .unwrap();
            assert_eq!(result.approval, None);

            // allowance = 1000 * 1e18, strictly greater than the requested 600.
            let (_rpc, _sg, vault) = vault1_with_allowance(
                "0x00000000000000000000000000000000000000000000003635c9adc5dea00000",
            )
            .await;
            let result = vault
                .get_calldatas(&Float::parse("600".to_string()).unwrap())
                .await
                .unwrap();
            assert_eq!(result.approval, None);
        }

        // The whole point of the decoupling: the allowance/approval path reads
        // an ERC20 allowance using only the vault token + owner + raindex
        // spender, never the deposit amount/vault_id/decimals. This RPC mock
        // ONLY answers a request whose calldata is exactly
        // `allowance(owner, spender)` for the vault token; it deliberately
        // carries no deposit context. If `read_allowance` / `get_transaction_args`
        // sent the wrong token/owner/spender (or smuggled deposit fields into
        // the read), the mock would not match and the call would error out.
        #[tokio::test]
        async fn test_allowance_read_uses_token_owner_spender_only() {
            let owner = Address::from_str("0x0000000000000000000000000000000000000000").unwrap();
            let spender = Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap();
            let expected_calldata = encode_prefixed(allowanceCall { owner, spender }.abi_encode());

            let (_rpc, _sg, vault) = vault1_with_allowance(
                "0x000000000000000000000000000000000000000000000000000000000000002a",
            )
            .await;

            // get_allowance succeeds against the strict matcher -> proves the
            // read targeted the right token/owner/spender with no deposit data.
            assert_eq!(vault.get_allowance().await.unwrap().0, U256::from(42));

            // Sanity check the matched calldata shape: the ERC20 allowance
            // selector (0xdd62ed3e) followed by the 32-byte-padded owner and
            // raindex spender, and nothing amount/vault_id/decimals related.
            assert!(expected_calldata.starts_with("0xdd62ed3e"));
            assert!(expected_calldata.to_lowercase().contains(
                &CHAIN_ID_1_RAINDEX_ADDRESS
                    .trim_start_matches("0x")
                    .to_lowercase()
            ));

            // The approval path uses the same decoupled read: a very large
            // requested amount (far beyond any vault balance/deposit context)
            // still produces correct approval calldata purely from the
            // allowance read, with no deposit args required.
            let (_rpc, _sg, vault) = vault1_with_allowance(
                "0x0000000000000000000000000000000000000000000000000000000000000000",
            )
            .await;
            let big_amount = Float::parse("1000000".to_string()).unwrap();
            let result = vault.get_calldatas(&big_amount).await.unwrap();
            assert_eq!(
                result.approval,
                Some(Bytes::copy_from_slice(
                    &approveCall {
                        spender,
                        amount: big_amount.to_fixed_decimal(18).unwrap(),
                    }
                    .abi_encode(),
                ))
            );
        }

        // `get_calldatas` validates the amount before touching the network. A
        // zero amount short-circuits with `ZeroAmount` even when the allowance RPC
        // would otherwise answer, so the decoupled path keeps the existing
        // validation ordering.
        #[tokio::test]
        async fn test_get_approval_calldata_rejects_zero_amount() {
            let (_rpc, _sg, vault) = vault1_with_allowance(
                "0x0000000000000000000000000000000000000000000000000000000000000000",
            )
            .await;
            let err = vault
                .get_calldatas(&Float::parse("0".to_string()).unwrap())
                .await
                .unwrap_err();
            assert_eq!(err.to_string(), RaindexError::ZeroAmount.to_string());
        }

        #[tokio::test]
        async fn test_get_vaults_with_token_filter() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"token_in\":[\"0x1d80c49bbbcd1c0911346656b529df9e5c2f783d\"]");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2")
                    .body_contains("\"token_in\":[\"0x1d80c49bbbcd1c0911346656b529df9e5c2f783d\"]");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": []
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let filters = GetVaultsFilters {
                owners: vec![],
                hide_zero_balance: false,
                tokens: Some(vec![Address::from_str(
                    "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                )
                .unwrap()]),
                raindex_addresses: None,
                only_active_orders: false,
            };

            let result = raindex_client
                .get_vaults(None, Some(filters), None, None)
                .await
                .unwrap()
                .items();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].id, Bytes::from_str("0x0123").unwrap());
            assert_eq!(
                result[0].token.address,
                Address::from_str("0x1d80c49bbbcd1c0911346656b529df9e5c2f783d").unwrap()
            );
        }

        #[tokio::test]
        async fn test_get_vaults_with_multiple_token_filters() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1")
                    .body_contains("\"token_in\":[\"0x1d80c49bbbcd1c0911346656b529df9e5c2f783d\",\"0x12e605bc104e93b45e1ad99f9e555f659051c2bb\"]");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": [get_vault1_json(), get_vault2_json()]
                    }
                }));
            });
            sg_server.mock(|when, then| {
                when.path("/sg2");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vaults": []
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            let filters = GetVaultsFilters {
                owners: vec![],
                hide_zero_balance: false,
                tokens: Some(vec![
                    Address::from_str("0x1d80c49bbbcd1c0911346656b529df9e5c2f783d").unwrap(),
                    Address::from_str("0x12e605bc104e93b45e1ad99f9e555f659051c2bb").unwrap(),
                ]),
                raindex_addresses: None,
                only_active_orders: false,
            };

            let result = raindex_client
                .get_vaults(None, Some(filters), None, None)
                .await
                .unwrap()
                .items();

            assert_eq!(result.len(), 2);
        }

        #[tokio::test]
        async fn test_get_all_vault_tokens_without_filter() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "erc20S": [
                            {
                                "id": "token1",
                                "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                "name": "Token 1",
                                "symbol": "TKN1",
                                "decimals": "18"
                            }
                        ]
                    }
                }));
            });

            sg_server.mock(|when, then| {
                when.path("/sg2");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "erc20S": [
                            {
                                "id": "token2",
                                "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783f",
                                "name": "Token 2",
                                "symbol": "TKN2",
                                "decimals": "18"
                            }
                        ]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            // Test with specific chain filter (only chain 1)
            let result = raindex_client.get_all_vault_tokens(None).await.unwrap();

            assert_eq!(result.len(), 2);
        }

        #[tokio::test]
        async fn test_get_all_vault_tokens_with_chain_filter() {
            let sg_server = MockServer::start_async().await;
            sg_server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "erc20S": [
                            {
                                "id": "token1",
                                "address": "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                                "name": "Token 1",
                                "symbol": "TKN1",
                                "decimals": "18"
                            }
                        ]
                    }
                }));
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &sg_server.url("/sg1"),
                    &sg_server.url("/sg2"),
                    &sg_server.url("/rpc1"),
                    &sg_server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();

            // Test with specific chain filter (only chain 1)
            let result = raindex_client
                .get_all_vault_tokens(Some(ChainIds(vec![1])))
                .await
                .unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].id(), "token1");
            assert_eq!(result[0].chain_id(), 1);
        }

        #[tokio::test]
        async fn test_get_account_balance_from_vault() {
            let server = MockServer::start_async().await;
            server.mock(|when, then| {
                when.path("/sg1");
                then.status(200).json_body_obj(&json!({
                    "data": {
                        "vault": get_vault1_json()
                    }
                }));
            });
            server.mock(|when, then| {
                when.method("POST")
                    .path("/rpc1")
                    .body_contains("0x70a08231");
                then.body(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "result": "0x00000000000000000000000000000000000000000000000000000000000003e8"
                    })
                    .to_string(),
                );
            });

            let raindex_client = RaindexClient::new(
                vec![get_test_yaml(
                    &server.url("/sg1"),
                    &server.url("/sg2"),
                    &server.url("/rpc1"),
                    &server.url("/rpc2"),
                )],
                None,
                None,
            )
            .await
            .unwrap();
            let vault = raindex_client
                .get_vault(
                    &RaindexIdentifier::new(
                        1,
                        Address::from_str(CHAIN_ID_1_RAINDEX_ADDRESS).unwrap(),
                    ),
                    Bytes::from_str("0x0123").unwrap(),
                )
                .await
                .unwrap();

            let balance = vault.get_owner_balance(Address::random()).await.unwrap();
            assert_eq!(balance, U256::from(1000));
        }

        #[test]
        fn get_vaults_filters_to_sg_filter_args_maps_raindex_addresses() {
            use raindex_subgraph_client::types::common::SgVaultsListFilterArgs;

            let filters = GetVaultsFilters {
                owners: vec![],
                hide_zero_balance: false,
                tokens: None,
                raindex_addresses: Some(vec![
                    address!("0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
                    address!("0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"),
                ]),
                only_active_orders: false,
            };

            let sg_filter_args: SgVaultsListFilterArgs = filters.try_into().unwrap();

            assert_eq!(sg_filter_args.raindexes.len(), 2);
            assert_eq!(
                sg_filter_args.raindexes[0],
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            );
            assert_eq!(
                sg_filter_args.raindexes[1],
                "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            );
        }

        #[test]
        fn get_vaults_filters_to_sg_filter_args_empty_raindex_addresses() {
            use raindex_subgraph_client::types::common::SgVaultsListFilterArgs;

            let filters = GetVaultsFilters {
                owners: vec![],
                hide_zero_balance: false,
                tokens: None,
                raindex_addresses: None,
                only_active_orders: false,
            };

            let sg_filter_args: SgVaultsListFilterArgs = filters.try_into().unwrap();

            assert!(sg_filter_args.raindexes.is_empty());
        }

        #[test]
        fn get_vaults_filters_to_sg_filter_args_lowercases_mixed_case_addresses() {
            use raindex_subgraph_client::types::common::SgVaultsListFilterArgs;

            let filters = GetVaultsFilters {
                owners: vec![],
                hide_zero_balance: false,
                tokens: None,
                raindex_addresses: Some(vec![address!(
                    "0xDeaDbEEfDeaDbEEfDeaDbEEfDeaDbEEfDeaDbEEf"
                )]),
                only_active_orders: false,
            };

            let sg_filter_args: SgVaultsListFilterArgs = filters.try_into().unwrap();

            assert_eq!(sg_filter_args.raindexes.len(), 1);
            assert_eq!(
                sg_filter_args.raindexes[0],
                "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
            );
        }
    }
}
