#[cfg(not(target_family = "wasm"))]
pub mod native;
#[cfg(target_family = "wasm")]
pub mod wasm;

#[cfg(not(target_family = "wasm"))]
pub use native::*;
#[cfg(target_family = "wasm")]
pub use wasm::*;

use crate::local_db::pipeline::runner::RunOutcome;
use crate::local_db::LocalDbError;

/// Longest wait between sync cycles while a network keeps failing.
pub(crate) const MAX_FAILURE_BACKOFF_MS: u64 = 5 * 60 * 1000;

/// Step used to sleep between cycles so a stop request is noticed promptly
/// even during a long backoff.
pub(crate) const STOP_POLL_MS: u64 = 250;

/// How one network cycle ended, as far as pacing and readiness care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CycleResult {
    /// Every target reached its requested block (or the client is not the
    /// leader): the chain is ready, wait the normal interval.
    CaughtUp,
    /// At least one target succeeded but stopped short of its requested
    /// block because of the per-cycle cap: run the next cycle at once and do
    /// not mark the chain ready yet.
    Behind,
    /// `run_once` errored or any target failed: back off. A failing target
    /// keeps its watermark, so retrying at the normal pace (or at once while
    /// another target catches up) would re-read its window again and again;
    /// backing off the whole network bounds that even when other targets on
    /// it are healthy.
    Failed,
}

impl CycleResult {
    pub(crate) fn of(result: &Result<RunOutcome, LocalDbError>) -> Self {
        match result {
            Err(_) => Self::Failed,
            Ok(RunOutcome::NotLeader) => Self::CaughtUp,
            Ok(RunOutcome::Report(report)) => {
                if !report.failures.is_empty() {
                    Self::Failed
                } else if report.successes.iter().any(|s| !s.outcome.caught_up) {
                    Self::Behind
                } else {
                    Self::CaughtUp
                }
            }
        }
    }
}

/// Paces one network's sync loop. A failed cycle keeps its old watermark,
/// so retrying at the normal interval re-reads the same blocks over and
/// over; this backs off while any target keeps failing and resets once a
/// cycle completes without failures.
#[derive(Debug, Default)]
pub(crate) struct CycleBackoff {
    consecutive_failures: u32,
}

impl CycleBackoff {
    /// Records a cycle and returns the delay before the next one.
    pub(crate) fn next_delay_ms(&mut self, interval_ms: u64, result: CycleResult) -> u64 {
        match result {
            CycleResult::Failed => {
                self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                next_cycle_delay_ms(interval_ms, self.consecutive_failures)
            }
            CycleResult::Behind => {
                self.consecutive_failures = 0;
                0
            }
            CycleResult::CaughtUp => {
                self.consecutive_failures = 0;
                interval_ms
            }
        }
    }

    pub(crate) fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
}

/// Delay before the next sync cycle.
///
/// After a success this is the configured interval. Each consecutive failure
/// doubles it, up to [`MAX_FAILURE_BACKOFF_MS`] (or the interval itself if
/// that is longer). A cycle that fails before persisting keeps its old
/// watermark, so retrying at the normal pace re-reads the same blocks every
/// few seconds; backing off bounds that RPC load while the cause is fixed.
pub(crate) fn next_cycle_delay_ms(interval_ms: u64, consecutive_failures: u32) -> u64 {
    if consecutive_failures == 0 {
        return interval_ms;
    }
    let ceiling = MAX_FAILURE_BACKOFF_MS.max(interval_ms);
    let factor = 1u64
        .checked_shl(consecutive_failures.min(32))
        .unwrap_or(u64::MAX);
    interval_ms.saturating_mul(factor).min(ceiling)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_db::pipeline::runner::{RunReport, TargetFailure, TargetStage, TargetSuccess};
    use crate::local_db::pipeline::SyncOutcome;
    use crate::local_db::RaindexIdentifier;
    use alloy::primitives::Address;

    fn success(caught_up: bool) -> TargetSuccess {
        TargetSuccess {
            outcome: SyncOutcome {
                raindex_id: RaindexIdentifier::new(1, Address::ZERO),
                start_block: 1,
                target_block: 2,
                fetched_logs: 0,
                caught_up,
                decoded_events: 0,
            },
        }
    }

    fn failure() -> TargetFailure {
        TargetFailure {
            raindex_id: RaindexIdentifier::new(1, Address::ZERO),
            raindex_key: None,
            stage: TargetStage::EngineRun,
            error: LocalDbError::CustomError("boom".to_string()),
        }
    }

    fn report(
        successes: Vec<TargetSuccess>,
        failures: Vec<TargetFailure>,
    ) -> Result<RunOutcome, LocalDbError> {
        Ok(RunOutcome::Report(RunReport {
            successes,
            failures,
        }))
    }

    #[test]
    fn classifies_cycles() {
        assert_eq!(
            CycleResult::of(&Err(LocalDbError::CustomError("x".into()))),
            CycleResult::Failed
        );
        assert_eq!(
            CycleResult::of(&Ok(RunOutcome::NotLeader)),
            CycleResult::CaughtUp
        );
        assert_eq!(
            CycleResult::of(&report(vec![], vec![])),
            CycleResult::CaughtUp
        );
        assert_eq!(
            CycleResult::of(&report(vec![success(true)], vec![])),
            CycleResult::CaughtUp
        );
        assert_eq!(
            CycleResult::of(&report(vec![success(false)], vec![])),
            CycleResult::Behind
        );
        assert_eq!(
            CycleResult::of(&report(vec![], vec![failure()])),
            CycleResult::Failed
        );
        // A failing target backs the network off even when another succeeds,
        // so a partial failure cannot re-read its window every cycle.
        assert_eq!(
            CycleResult::of(&report(vec![success(true)], vec![failure()])),
            CycleResult::Failed
        );
        assert_eq!(
            CycleResult::of(&report(vec![success(false)], vec![failure()])),
            CycleResult::Failed
        );
    }

    #[test]
    fn backoff_doubles_on_failures_and_resets_on_progress() {
        let mut backoff = CycleBackoff::default();
        assert_eq!(backoff.next_delay_ms(5_000, CycleResult::Failed), 10_000);
        assert_eq!(backoff.next_delay_ms(5_000, CycleResult::Failed), 20_000);
        assert_eq!(backoff.consecutive_failures(), 2);
        assert_eq!(backoff.next_delay_ms(5_000, CycleResult::CaughtUp), 5_000);
        assert_eq!(backoff.consecutive_failures(), 0);
        assert_eq!(backoff.next_delay_ms(5_000, CycleResult::Failed), 10_000);
        assert_eq!(backoff.next_delay_ms(5_000, CycleResult::Behind), 0);
        assert_eq!(backoff.consecutive_failures(), 0);
    }

    #[test]
    fn success_uses_the_configured_interval() {
        assert_eq!(next_cycle_delay_ms(5_000, 0), 5_000);
    }

    #[test]
    fn failures_double_the_delay() {
        assert_eq!(next_cycle_delay_ms(5_000, 1), 10_000);
        assert_eq!(next_cycle_delay_ms(5_000, 2), 20_000);
        assert_eq!(next_cycle_delay_ms(5_000, 3), 40_000);
    }

    #[test]
    fn failures_stop_at_the_ceiling() {
        assert_eq!(next_cycle_delay_ms(5_000, 6), MAX_FAILURE_BACKOFF_MS);
        assert_eq!(next_cycle_delay_ms(5_000, 64), MAX_FAILURE_BACKOFF_MS);
        assert_eq!(next_cycle_delay_ms(5_000, u32::MAX), MAX_FAILURE_BACKOFF_MS);
    }

    #[test]
    fn interval_longer_than_ceiling_is_kept() {
        assert_eq!(next_cycle_delay_ms(600_000, 3), 600_000);
    }

    #[test]
    fn zero_interval_stays_zero() {
        assert_eq!(next_cycle_delay_ms(0, 5), 0);
    }
}
