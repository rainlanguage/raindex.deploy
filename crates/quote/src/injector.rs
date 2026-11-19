use alloy::primitives::Address;
use async_trait::async_trait;
use raindex_bindings::IRaindexV7::{OrderV4, SignedContextV2};

/// Caller-supplied source of additional `SignedContextV2` entries for each
/// order candidate. Invoked during candidate building, after any oracle-URL
/// contexts have been fetched. Returned contexts are appended (not replacing
/// oracle contexts). Strategies that verify signed context by index must
/// therefore know the composition order: `[oracle..., injected...]`.
///
/// Implementors may inspect any field of `order` (tokens, owner, nonce,
/// evaluable bytecode), the IO indices selected for this take, and the
/// `counterparty` that will execute the take, to produce per-order
/// signatures or other signed attestations.
#[async_trait]
pub trait SignedContextInjector: Send + Sync {
    async fn contexts_for(
        &self,
        order: &OrderV4,
        input_io_index: u32,
        output_io_index: u32,
        counterparty: Address,
    ) -> Vec<SignedContextV2>;
}

/// No-op injector used as the default when no caller-supplied injector is given.
pub struct NoopInjector;

#[async_trait]
impl SignedContextInjector for NoopInjector {
    async fn contexts_for(
        &self,
        _order: &OrderV4,
        _input_io_index: u32,
        _output_io_index: u32,
        _counterparty: Address,
    ) -> Vec<SignedContextV2> {
        vec![]
    }
}
