#[cfg(not(target_family = "wasm"))]
use crate::transaction::TransactionArgs;
use crate::transaction::WritableTransactionExecuteError;
#[cfg(not(target_family = "wasm"))]
use crate::write_tx::{execute_write_tx, WriteTransactionStatus};
use alloy::primitives::{Address, B256};
use alloy::sol_types::SolCall;
use serde::{Deserialize, Serialize};

use rain_math_float::Float;
use raindex_bindings::IRaindexV7::withdraw5Call;

#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct WithdrawArgs {
    pub token: Address,
    pub vault_id: B256,
    pub target_amount: Float,
}

impl From<WithdrawArgs> for withdraw5Call {
    fn from(val: WithdrawArgs) -> Self {
        withdraw5Call {
            token: val.token,
            vaultId: val.vault_id,
            targetAmount: val.target_amount.get_inner(),
            tasks: vec![],
        }
    }
}

impl WithdrawArgs {
    #[cfg(not(target_family = "wasm"))]
    pub async fn execute<S: Fn(WriteTransactionStatus)>(
        &self,
        transaction_args: TransactionArgs,
        transaction_status_changed: S,
    ) -> Result<(), WritableTransactionExecuteError> {
        let (ledger_client, _) = transaction_args.clone().try_into_ledger_client().await?;

        let withdraw_call: withdraw5Call = self.clone().into();
        let tx_request = transaction_args
            .try_into_transaction_request(withdraw_call, transaction_args.raindex_address)?;

        execute_write_tx(ledger_client, tx_request, 4, transaction_status_changed).await?;

        Ok(())
    }

    pub async fn get_withdraw_calldata(&self) -> Result<Vec<u8>, WritableTransactionExecuteError> {
        let withdraw_call: withdraw5Call = self.clone().into();
        Ok(withdraw_call.abi_encode())
    }
}
