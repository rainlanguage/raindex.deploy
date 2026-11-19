use super::pipeline::runner::ProducerRunReport;
use crate::commands::local_db::pipeline::runner::ProducerRunner;
use anyhow::Result;
use clap::Parser;
use raindex_common::local_db::pipeline::runner::TargetFailure;
use std::io::{self, Write};
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone, Parser)]
#[command(about = "Run the producer pipeline across all raindexes in settings.yaml")]
pub struct RunPipeline {
    #[clap(
        long,
        help = "Full YAML document that configures the local DB pipeline",
        value_name = "YAML"
    )]
    pub settings_yaml: String,

    #[clap(
        long,
        help = "HyperRPC API token used to fetch logs during the run",
        value_name = "TOKEN"
    )]
    pub api_token: String,

    #[clap(
        long,
        help = "Output directory where per-raindex SQLite databases and dumps are written",
        value_name = "PATH",
        default_value = "./local-db"
    )]
    pub out_root: PathBuf,

    #[clap(
        long,
        help = "Base URL for published dumps (e.g., https://example.com/releases)",
        value_name = "URL",
        value_parser = clap::value_parser!(Url)
    )]
    pub release_base_url: Url,

    #[clap(
        long,
        help = "Emit per-stage status updates from the pipeline as it runs"
    )]
    pub debug_status: bool,
}

impl RunPipeline {
    pub async fn execute(self) -> Result<()> {
        println!("Starting producer pipeline run");

        let RunPipeline {
            settings_yaml,
            api_token,
            out_root,
            release_base_url,
            debug_status,
        } = self;

        std::fs::create_dir_all(&out_root)?;

        let runner = ProducerRunner::new(
            settings_yaml,
            out_root,
            release_base_url,
            api_token,
            debug_status.into(),
        )?;
        let report = runner.run().await?;

        render_report(&report);

        if report.failures().is_empty() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "one or more producer jobs failed ({} failure(s))",
                report.failures().len()
            ))
        }
    }
}

fn render_report(report: &ProducerRunReport) {
    let mut stdout = io::stdout();
    let _ = render_report_to(report, &mut stdout);
}

fn render_report_to<W: Write>(report: &ProducerRunReport, writer: &mut W) -> io::Result<()> {
    if report.successes().is_empty() {
        writeln!(writer, "No producer jobs completed successfully.")?;
    } else {
        writeln!(
            writer,
            "Producer pipeline completed {} successful job(s):",
            report.successes().len()
        )?;
        for outcome in report.successes() {
            let raindex_id = &outcome.outcome.raindex_id;
            match report.export_for(raindex_id) {
                Some(export) => {
                    writeln!(
                        writer,
                        "- chain {} raindex {:#x}: start {} → target {} | logs {} | events {} | dump {} (end block {}, hash {}, time {})",
                        raindex_id.chain_id,
                        raindex_id.raindex_address,
                        outcome.outcome.start_block,
                        outcome.outcome.target_block,
                        outcome.outcome.fetched_logs,
                        outcome.outcome.decoded_events,
                        export.dump_path.display(),
                        export.end_block,
                        export.end_block_hash,
                        export.end_block_time_ms,
                    )?;
                }
                None => {
                    writeln!(
                        writer,
                        "- chain {} raindex {:#x}: start {} → target {} | logs {} | events {} | dump <none>",
                        raindex_id.chain_id,
                        raindex_id.raindex_address,
                        outcome.outcome.start_block,
                        outcome.outcome.target_block,
                        outcome.outcome.fetched_logs,
                        outcome.outcome.decoded_events,
                    )?;
                }
            }
        }
    }

    if report.failures().is_empty() {
        writeln!(writer, "All producer jobs completed successfully.")?;
    } else {
        writeln!(
            writer,
            "{} job(s) failed during the producer run:",
            report.failures().len()
        )?;
        for failure in report.failures() {
            render_failure_to(failure, writer)?;
        }
    }

    Ok(())
}

fn render_failure_to<W: Write>(failure: &TargetFailure, writer: &mut W) -> io::Result<()> {
    let raindex_id = &failure.raindex_id;
    let address = raindex_id.raindex_address;
    let chain_id = raindex_id.chain_id;
    let stage = failure.stage;
    let message = failure.error.to_readable_msg();
    let key = failure
        .raindex_key
        .as_deref()
        .unwrap_or("<unknown-raindex>");

    if chain_id == 0 && address.is_zero() {
        writeln!(writer, "- job {} failed at {:?}: {}", key, stage, message)
    } else {
        writeln!(
            writer,
            "- chain {} raindex {:#x} ({}) failed at {:?}: {}",
            chain_id, address, key, stage, message
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::local_db::pipeline::runner::export::ExportMetadata;

    use super::*;
    use alloy::primitives::{address, Address};
    use raindex_common::local_db::pipeline::engine::SyncInputs;
    use raindex_common::local_db::pipeline::runner::utils::RunnerTarget;
    use raindex_common::local_db::pipeline::runner::{TargetFailure, TargetStage, TargetSuccess};
    use raindex_common::local_db::pipeline::{
        FinalityConfig, SyncConfig, SyncOutcome, WindowOverrides,
    };
    use raindex_common::local_db::{FetchConfig, LocalDbError, RaindexIdentifier};
    use std::collections::HashMap;

    #[test]
    fn default_out_root_is_local_db() {
        let command = RunPipeline::parse_from([
            "sync",
            "--settings-yaml",
            "test",
            "--api-token",
            "token",
            "--release-base-url",
            "https://example.com/releases",
        ]);
        assert_eq!(command.out_root, PathBuf::from("./local-db"));
    }

    fn sample_success_and_export(chain_id: u32) -> (TargetSuccess, ExportMetadata) {
        let raindex_id = RaindexIdentifier::new(
            chain_id,
            address!("0000000000000000000000000000000000000a11"),
        );
        let fetch = FetchConfig::new(10, 5, 2, 1, 0, 0).unwrap();
        let sync_config = SyncConfig {
            deployment_block: 100,
            fetch,
            finality: FinalityConfig { depth: 12 },
            window_overrides: WindowOverrides::default(),
            max_blocks_per_cycle: None,
        };
        let inputs = SyncInputs {
            raindex_id: raindex_id.clone(),
            metadata_rpcs: Vec::new(),
            cfg: sync_config,
            dump_str: None,
            defer_analyze: false,
            block_number_threshold: 10000,
            manifest_end_block: 1,
        };
        let runner_target = RunnerTarget {
            raindex_key: "test".to_string(),
            manifest_url: "https://example.com/manifest.yaml".parse().unwrap(),
            network_key: "anvil".to_string(),
            inputs,
        };

        let outcome = SyncOutcome {
            raindex_id: runner_target.inputs.raindex_id.clone(),
            start_block: 200,
            target_block: 400,
            fetched_logs: 123,
            decoded_events: 456,
            caught_up: true,
        };
        let export = ExportMetadata {
            dump_path: PathBuf::from(format!(
                "./local-db/{}/{}-{}.sql.gz",
                chain_id, chain_id, runner_target.inputs.raindex_id.raindex_address
            )),
            end_block: 400,
            end_block_hash: "0xdeadbeef".to_string(),
            end_block_time_ms: 1_700_000_000,
        };

        (TargetSuccess { outcome }, export)
    }

    #[test]
    fn render_report_to_writes_success_summary() {
        let (success, export) = sample_success_and_export(42161);
        let mut exports = HashMap::new();
        exports.insert(success.outcome.raindex_id.clone(), Some(export));
        let report = ProducerRunReport {
            successes: vec![success],
            failures: vec![],
            exports,
        };
        let mut buffer = Vec::new();
        render_report_to(&report, &mut buffer).expect("render succeeds");

        let output = String::from_utf8(buffer).expect("utf8");
        assert!(output.contains("1 successful job"));
        assert!(output.contains("chain 42161"));
        assert!(output.contains("start 200 → target 400"));
        assert!(output.contains("logs 123"));
        assert!(output.contains("events 456"));
        assert!(output.contains("All producer jobs completed successfully."));
    }

    #[test]
    fn render_report_to_handles_missing_dump() {
        let (success, _) = sample_success_and_export(10);
        let mut exports = HashMap::new();
        exports.insert(success.outcome.raindex_id.clone(), None);
        let report = ProducerRunReport {
            successes: vec![success],
            failures: vec![],
            exports,
        };

        let mut buffer = Vec::new();
        render_report_to(&report, &mut buffer).expect("render succeeds");

        let output = String::from_utf8(buffer).expect("utf8");
        assert!(output.contains("dump <none>"));
    }

    #[test]
    fn render_report_to_lists_failures() {
        let raindex_address = address!("0000000000000000000000000000000000000fA1");
        let failure = TargetFailure {
            raindex_id: RaindexIdentifier::new(1, raindex_address),
            raindex_key: Some("book".into()),
            stage: TargetStage::EngineRun,
            error: LocalDbError::CustomError("oh no".into()),
        };
        let report = ProducerRunReport {
            successes: Vec::new(),
            failures: vec![failure],
            exports: HashMap::new(),
        };

        let mut buffer = Vec::new();
        render_report_to(&report, &mut buffer).expect("render succeeds");

        let output = String::from_utf8(buffer).expect("utf8");
        assert!(output.contains("No producer jobs completed successfully."));
        assert!(output.contains("1 job(s) failed"));
        assert!(output.contains(&format!("chain {} raindex {:#x}", 1, raindex_address)));
        assert!(output.contains("oh no"));
    }

    #[test]
    fn render_failure_to_handles_unknowns() {
        let failure = TargetFailure {
            raindex_id: RaindexIdentifier::new(0, Address::ZERO),
            raindex_key: None,
            stage: TargetStage::EngineRun,
            error: LocalDbError::CustomError("boom".into()),
        };
        let mut buffer = Vec::new();
        render_failure_to(&failure, &mut buffer).expect("render succeeds");
        let output = String::from_utf8(buffer).expect("utf8");
        assert!(output.contains("job <unknown-raindex> failed"));
        assert!(output.contains("EngineRun"));
    }

    // The terse "job <key> failed" line is used only for the fully-unknown raindex
    // (chain id zero AND a zero address). A failure identified by EITHER a non-zero
    // chain id OR a non-zero address renders the detailed
    // "chain ... raindex ... (key)" line. These two cases cover the mixed-identity
    // combinations where one component is zero and the other is non-zero.
    #[test]
    fn render_failure_to_zero_address_nonzero_chain_uses_detailed_line() {
        let failure = TargetFailure {
            raindex_id: RaindexIdentifier::new(42161, Address::ZERO),
            raindex_key: Some("book".into()),
            stage: TargetStage::EngineRun,
            error: LocalDbError::CustomError("boom".into()),
        };
        let mut buffer = Vec::new();
        render_failure_to(&failure, &mut buffer).expect("render succeeds");
        let output = String::from_utf8(buffer).expect("utf8");

        assert_eq!(
            output,
            format!(
                "- chain {} raindex {:#x} ({}) failed at {:?}: {}\n",
                42161,
                Address::ZERO,
                "book",
                TargetStage::EngineRun,
                "boom",
            )
        );
        assert!(
            !output.starts_with("- job "),
            "non-zero chain id must not collapse to the terse unknown-job line: {output}"
        );
    }

    #[test]
    fn render_failure_to_nonzero_address_zero_chain_uses_detailed_line() {
        let raindex_address = address!("0000000000000000000000000000000000000fA1");
        let failure = TargetFailure {
            raindex_id: RaindexIdentifier::new(0, raindex_address),
            raindex_key: Some("book".into()),
            stage: TargetStage::EngineRun,
            error: LocalDbError::CustomError("boom".into()),
        };
        let mut buffer = Vec::new();
        render_failure_to(&failure, &mut buffer).expect("render succeeds");
        let output = String::from_utf8(buffer).expect("utf8");

        assert_eq!(
            output,
            format!(
                "- chain {} raindex {:#x} ({}) failed at {:?}: {}\n",
                0,
                raindex_address,
                "book",
                TargetStage::EngineRun,
                "boom",
            )
        );
        assert!(
            !output.starts_with("- job "),
            "non-zero address must not collapse to the terse unknown-job line: {output}"
        );
    }
}
