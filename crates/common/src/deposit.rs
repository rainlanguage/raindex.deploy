use crate::allowance::read_allowance;
use crate::transaction::{TransactionArgs, TransactionArgsError, WritableTransactionExecuteError};
use alloy::primitives::{Address, B256, U256};
use rain_math_float::{Float, FloatError};
use raindex_bindings::IRaindexV7::deposit5Call;
#[cfg(not(target_family = "wasm"))]
use raindex_bindings::IERC20::approveCall;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(not(target_family = "wasm"))]
use crate::write_tx::{execute_write_tx, WriteTransactionStatus};

#[derive(Error, Debug)]
pub enum DepositError {
    #[error(transparent)]
    WritableTransactionExecuteError(#[from] WritableTransactionExecuteError),

    #[error(transparent)]
    TransactionArgsError(#[from] TransactionArgsError),

    #[error(transparent)]
    FloatError(#[from] FloatError),

    #[cfg(not(target_family = "wasm"))]
    #[error(transparent)]
    WriteTransactionError(#[from] crate::write_tx::WriteTransactionError),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DepositArgs {
    pub token: Address,
    pub vault_id: B256,
    pub amount: Float,
    pub decimals: u8,
}

impl TryFrom<DepositArgs> for deposit5Call {
    type Error = FloatError;

    fn try_from(val: DepositArgs) -> Result<Self, Self::Error> {
        Ok(deposit5Call {
            token: val.token,
            vaultId: val.vault_id,
            depositAmount: val.amount.get_inner(),
            tasks: vec![],
        })
    }
}

impl DepositArgs {
    pub async fn read_allowance(
        &self,
        owner: Address,
        transaction_args: TransactionArgs,
    ) -> Result<U256, DepositError> {
        let res = read_allowance(
            &transaction_args.rpcs,
            self.token,
            owner,
            transaction_args.raindex_address,
        )
        .await?;
        Ok(res)
    }

    #[cfg(not(target_family = "wasm"))]
    pub async fn execute_approve<S: Fn(WriteTransactionStatus)>(
        &self,
        transaction_args: TransactionArgs,
        transaction_status_changed: S,
    ) -> Result<(), DepositError> {
        let (ledger_client, address) = transaction_args.clone().try_into_ledger_client().await?;

        let current_allowance = self
            .read_allowance(address, transaction_args.clone())
            .await?;
        let current_allowance_float = Float::from_fixed_decimal(current_allowance, self.decimals)?;

        if !current_allowance_float.eq(self.amount)? {
            let approve_call = approveCall {
                spender: transaction_args.raindex_address,
                amount: self.amount.to_fixed_decimal(self.decimals)?,
            };
            let tx_request =
                transaction_args.try_into_transaction_request(approve_call, self.token)?;

            execute_write_tx(ledger_client, tx_request, 4, transaction_status_changed).await?;
        }

        Ok(())
    }

    #[cfg(not(target_family = "wasm"))]
    pub async fn execute_deposit<S: Fn(WriteTransactionStatus)>(
        &self,
        transaction_args: TransactionArgs,
        transaction_status_changed: S,
    ) -> Result<(), DepositError> {
        let (ledger_client, _) = transaction_args.clone().try_into_ledger_client().await?;

        let deposit_call: deposit5Call = self.clone().try_into()?;
        let tx_request = transaction_args
            .try_into_transaction_request(deposit_call, transaction_args.raindex_address)?;

        execute_write_tx(ledger_client, tx_request, 4, transaction_status_changed).await?;

        Ok(())
    }
}
