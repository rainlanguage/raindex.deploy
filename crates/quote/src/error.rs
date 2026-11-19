use alloy::{
    primitives::{hex::FromHexError, U256},
    providers::MulticallError,
};
use rain_error_decoding::{AbiDecodeFailedErrors, AbiDecodedErrorType};
use raindex_bindings::provider::ReadProviderError;
use raindex_subgraph_client::{
    types::order_detail_traits::OrderDetailError, RaindexSubgraphClientError,
};
use thiserror::Error;
use url::ParseError;
#[cfg(target_family = "wasm")]
use wasm_bindgen_utils::prelude::*;

#[derive(Debug, Error)]
pub enum FailedQuote {
    #[error("Order does not exist")]
    NonExistent,
    #[error(transparent)]
    RevertError(#[from] AbiDecodedErrorType),
    #[error("Corrupt return data: {0}")]
    CorruptReturnData(String),
    #[error(transparent)]
    RevertErrorDecodeFailed(#[from] AbiDecodeFailedErrors),
    #[cfg(target_family = "wasm")]
    #[error(transparent)]
    SerdeWasmBindgenError(#[from] serde_wasm_bindgen::Error),
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    UrlParseError(#[from] ParseError),
    #[error(transparent)]
    SubgraphClientError(#[from] RaindexSubgraphClientError),
    #[error(transparent)]
    FromHexError(#[from] FromHexError),
    #[error(transparent)]
    OrderDetailError(#[from] OrderDetailError),
    #[error(transparent)]
    AlloySolTypesError(#[from] alloy::sol_types::Error),
    #[cfg(target_family = "wasm")]
    #[error(transparent)]
    SerdeWasmBindgenError(#[from] serde_wasm_bindgen::Error),
    #[error("Invalid quote target: index {0} is out of bounds for this Order")]
    InvalidQuoteTarget(U256),
    #[error("Quote pair selections must align with orders: expected {expected}, got {actual}")]
    PairSelectionLengthMismatch { expected: usize, actual: usize },
    #[error(transparent)]
    ReadProviderError(#[from] ReadProviderError),
    #[error("Multicall failed: {0}")]
    MulticallError(#[from] MulticallError),
    /// Internal signal from `quote_chunk_once` that the raindex's own
    /// `multicall(bytes[])` reverted at the chunk level (OZ Multicall bubbles
    /// the first failing inner call's revert; it cannot isolate per-element).
    /// The quote RPC layer consumes this to drive bisection and attribute the
    /// failure to a specific `QuoteTarget`.
    #[error("Quote chunk reverted: {0}")]
    ChunkReverted(Box<FailedQuote>),
    #[error("RPC transport error: {0}")]
    TransportError(String),
}

#[cfg(target_family = "wasm")]
impl From<FailedQuote> for JsValue {
    fn from(value: FailedQuote) -> Self {
        JsError::new(&value.to_string()).into()
    }
}

#[cfg(target_family = "wasm")]
impl From<Error> for JsValue {
    fn from(value: Error) -> Self {
        JsError::new(&value.to_string()).into()
    }
}
