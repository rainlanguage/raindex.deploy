pub mod candidates;
pub mod config;
pub mod preflight;
pub mod simulation;

pub use candidates::{build_take_order_candidates_for_pair, TakeOrderCandidate};
pub use config::{
    build_take_orders_config_from_simulation, BuiltTakeOrdersConfig, ParsedTakeOrdersMode,
    TakeOrdersMode,
};
pub use preflight::{
    build_approval_calldata, check_taker_allowance, check_taker_balance,
    check_taker_balance_and_allowance, find_failing_order_index, simulate_take_orders,
    AllowanceOnlyResult, BalanceAndAllowanceResult, PreflightError,
};
pub use raindex_quote::injector::{NoopInjector, SignedContextInjector};
pub use simulation::{
    simulate_buy_over_candidates, simulate_candidates, simulate_spend_over_candidates,
    SelectedTakeOrderLeg, SimulationResult,
};
