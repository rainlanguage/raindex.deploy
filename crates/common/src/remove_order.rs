#[cfg(not(target_family = "wasm"))]
use crate::transaction::TransactionArgs;
use crate::transaction::TransactionArgsError;
#[cfg(not(target_family = "wasm"))]
use crate::write_tx::{execute_write_tx, WriteTransactionError, WriteTransactionStatus};
use alloy::primitives::hex::FromHexError;
use alloy::sol_types::SolCall;
use raindex_bindings::IRaindexV7::removeOrder4Call;
use raindex_subgraph_client::types::{common::SgOrder, order_detail_traits::OrderDetailError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RemoveOrderArgsError {
    #[cfg(not(target_family = "wasm"))]
    #[error(transparent)]
    WriteTransaction(#[from] WriteTransactionError),
    #[error(transparent)]
    TransactionArgs(#[from] TransactionArgsError),
    #[error(transparent)]
    FromHexError(#[from] FromHexError),
    #[error(transparent)]
    OrderDetailError(#[from] OrderDetailError),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RemoveOrderArgs {
    pub order: SgOrder,
}

impl From<SgOrder> for RemoveOrderArgs {
    fn from(order: SgOrder) -> Self {
        Self { order }
    }
}

impl TryInto<removeOrder4Call> for RemoveOrderArgs {
    type Error = OrderDetailError;

    fn try_into(self) -> Result<removeOrder4Call, OrderDetailError> {
        Ok(removeOrder4Call {
            order: self.order.try_into()?,
            tasks: vec![],
        })
    }
}

impl RemoveOrderArgs {
    #[cfg(not(target_family = "wasm"))]
    pub async fn execute<S: Fn(WriteTransactionStatus)>(
        self,
        transaction_args: TransactionArgs,
        transaction_status_changed: S,
    ) -> Result<(), RemoveOrderArgsError> {
        let (ledger_client, _) = transaction_args.clone().try_into_ledger_client().await?;

        let remove_order_call: removeOrder4Call = self.try_into()?;
        let tx_request = transaction_args
            .try_into_transaction_request(remove_order_call, transaction_args.raindex_address)?;

        execute_write_tx(ledger_client, tx_request, 4, transaction_status_changed).await?;

        Ok(())
    }

    pub async fn get_rm_order_calldata(self) -> Result<Vec<u8>, RemoveOrderArgsError> {
        let remove_order_call: removeOrder4Call = self.try_into()?;
        Ok(remove_order_call.abi_encode())
    }
}
