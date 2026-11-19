use crate::local_db::fetch::FetchConfig;
use crate::local_db::pipeline::engine::SyncInputs;
use crate::local_db::pipeline::{FinalityConfig, SyncConfig, WindowOverrides};
use crate::local_db::{LocalDbError, RaindexIdentifier};
use itertools::Itertools;
use raindex_app_settings::local_db_sync::LocalDbSyncCfg;
use raindex_app_settings::network::NetworkCfg;
use raindex_app_settings::raindex::RaindexCfg;
use raindex_app_settings::yaml::raindex::{RaindexYaml, RaindexYamlValidation};
use raindex_app_settings::yaml::YamlParsable;
use std::collections::HashMap;
use url::Url;

/// Parsed settings required by the runner orchestration layer.
#[derive(Debug, Clone)]
pub struct ParsedRunnerSettings {
    pub raindexes: HashMap<String, RaindexCfg>,
    pub syncs: HashMap<String, LocalDbSyncCfg>,
}

/// Scheduling metadata for a single engine invocation.
#[derive(Debug, Clone)]
pub struct RunnerTarget {
    pub raindex_key: String,
    pub manifest_url: Url,
    pub network_key: String,
    pub inputs: SyncInputs,
}

/// Returns the networks that have local DB sync configured.
pub(crate) fn configured_sync_networks(settings: &ParsedRunnerSettings) -> Vec<NetworkCfg> {
    let mut networks = settings
        .raindexes
        .values()
        .filter(|raindex| settings.syncs.contains_key(&raindex.network.key))
        .map(|raindex| (raindex.network.key.clone(), (*raindex.network).clone()))
        .collect::<HashMap<_, _>>()
        .into_values()
        .collect::<Vec<_>>();
    networks.sort_by(|a, b| a.key.cmp(&b.key));
    networks
}

/// Parses the provided YAML string into raindexes and per-network sync settings.
pub fn parse_runner_settings(settings_yaml: &str) -> Result<ParsedRunnerSettings, LocalDbError> {
    let raindex_yaml = RaindexYaml::new(
        vec![settings_yaml.to_owned()],
        RaindexYamlValidation::default(),
    )?;
    parse_runner_settings_from_yaml(&raindex_yaml)
}

pub fn parse_runner_settings_from_yaml(
    yaml: &RaindexYaml,
) -> Result<ParsedRunnerSettings, LocalDbError> {
    let raindexes = yaml.get_raindexes()?;
    let syncs = yaml.get_local_db_syncs()?;
    Ok(ParsedRunnerSettings { raindexes, syncs })
}

pub(crate) fn map_sync_to_engine(
    sync: &LocalDbSyncCfg,
) -> Result<(FetchConfig, FinalityConfig), LocalDbError> {
    let fetch = FetchConfig::new(
        sync.batch_size as u64,
        sync.max_concurrent_batches as usize,
        sync.max_concurrent_batches as usize,
        sync.retry_attempts as usize,
        sync.retry_delay_ms,
        sync.rate_limit_delay_ms,
    )?;
    let finality = FinalityConfig {
        depth: sync.finality_depth,
    };
    Ok((fetch, finality))
}

/// Builds runner targets for all configured raindexes.
pub fn build_runner_targets(
    raindexes: &HashMap<String, RaindexCfg>,
    syncs: &HashMap<String, LocalDbSyncCfg>,
) -> Result<Vec<RunnerTarget>, LocalDbError> {
    let mut targets = Vec::with_capacity(raindexes.len());
    for (key, raindex) in raindexes {
        let network_key = raindex.network.key.clone();
        let sync_cfg =
            syncs
                .get(&network_key)
                .ok_or_else(|| LocalDbError::MissingLocalDbSyncForNetwork {
                    network: network_key.clone(),
                })?;
        let (fetch, finality) = map_sync_to_engine(sync_cfg)?;
        let inputs = SyncInputs {
            raindex_id: RaindexIdentifier::new(raindex.network.chain_id, raindex.address),
            metadata_rpcs: raindex.network.rpcs.clone(),
            cfg: SyncConfig {
                deployment_block: raindex.deployment_block,
                fetch,
                finality,
                window_overrides: WindowOverrides::default(),
                max_blocks_per_cycle: None,
            },
            dump_str: None,
            defer_analyze: false,
            block_number_threshold: sync_cfg.bootstrap_block_threshold,
            manifest_end_block: 0,
        };

        let remote =
            raindex
                .local_db_remote
                .as_ref()
                .ok_or_else(|| LocalDbError::MissingLocalDbRemote {
                    raindex_key: key.clone(),
                })?;

        targets.push(RunnerTarget {
            raindex_key: key.clone(),
            manifest_url: remote.url.clone(),
            network_key,
            inputs,
        });
    }

    Ok(targets)
}

/// Convenience helper retained for existing tests/utilities.
pub fn build_sync_inputs_from_yaml(settings_yaml: &str) -> Result<Vec<SyncInputs>, LocalDbError> {
    let parsed = parse_runner_settings(settings_yaml)?;
    let targets = build_runner_targets(&parsed.raindexes, &parsed.syncs)?;
    Ok(targets.into_iter().map(|target| target.inputs).collect())
}

/// Groups runner targets by network key for per-network scheduling.
pub fn group_targets_by_network(targets: &[RunnerTarget]) -> HashMap<String, Vec<RunnerTarget>> {
    targets
        .iter()
        .cloned()
        .into_group_map_by(|target| target.network_key.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_db::fetch::FetchConfigError;
    use crate::local_db::LocalDbError;
    use alloy::primitives::address;
    use raindex_app_settings::local_db_sync::LocalDbSyncCfg;
    use raindex_app_settings::spec_version::SpecVersion;
    use raindex_app_settings::yaml::default_document;
    use url::Url;

    fn sample_settings_yaml() -> String {
        format!(
            r#"
version: {version}
networks:
  network-a:
    rpcs:
      - https://rpc.network/a
    chain-id: 1
  network-b:
    rpcs:
      - https://rpc.network/b
    chain-id: 2
subgraphs:
  network-a: https://subgraph.network/a
  network-b: https://subgraph.network/b
local-db-remotes:
  remote-a: https://remotes.example.com/a.yaml
  remote-b: https://remotes.example.com/b.yaml
local-db-sync:
  network-a:
    batch-size: 10
    max-concurrent-batches: 5
    retry-attempts: 3
    retry-delay-ms: 100
    rate-limit-delay-ms: 50
    finality-depth: 12
    bootstrap-block-threshold: 10000
    sync-interval-ms: 5000
  network-b:
    batch-size: 20
    max-concurrent-batches: 2
    retry-attempts: 4
    retry-delay-ms: 200
    rate-limit-delay-ms: 100
    finality-depth: 24
    bootstrap-block-threshold: 5000
    sync-interval-ms: 5000
raindexes:
  raindex-a:
    address: 0x00000000000000000000000000000000000000a1
    network: network-a
    subgraph: network-a
    local-db-remote: remote-a
    deployment-block: 111
  raindex-b:
    address: 0x00000000000000000000000000000000000000b2
    network: network-b
    subgraph: network-b
    local-db-remote: remote-b
    deployment-block: 222
  raindex-c:
    address: 0x00000000000000000000000000000000000000c3
    network: network-a
    subgraph: network-a
    local-db-remote: remote-a
    deployment-block: 333
"#,
            version = SpecVersion::current()
        )
    }

    fn minimal_invalid_yaml() -> &'static str {
        "networks: {"
    }

    fn missing_sync_yaml() -> String {
        format!(
            r#"
version: {version}
networks:
  mainnet:
    rpcs:
      - https://rpc.network/mainnet
    chain-id: 1
subgraphs:
  mainnet: https://subgraph.network/mainnet
local-db-remotes:
  remote: https://remotes.example.com/mainnet.yaml
raindexes:
  book:
    address: 0x0000000000000000000000000000000000000001
    network: mainnet
    subgraph: mainnet
    local-db-remote: remote
    deployment-block: 100
"#,
            version = SpecVersion::current()
        )
    }

    fn parsed_settings() -> ParsedRunnerSettings {
        parse_runner_settings(&sample_settings_yaml()).expect("valid sample YAML")
    }

    fn assert_fetch_config_error(sync: LocalDbSyncCfg, expected: FetchConfigError) {
        let err = map_sync_to_engine(&sync).unwrap_err();
        match err {
            LocalDbError::FetchConfigError(actual) => assert_eq!(actual, expected),
            other => panic!("expected fetch config error, got {other:?}"),
        }
    }

    #[test]
    fn parse_runner_settings_happy_path() {
        let parsed = parse_runner_settings(&sample_settings_yaml()).expect("parse succeeds");
        assert_eq!(parsed.raindexes.len(), 3);
        assert_eq!(parsed.syncs.len(), 2);
        assert!(parsed.raindexes.contains_key("raindex-a"));
        assert!(parsed.syncs.contains_key("network-a"));
    }

    #[test]
    fn configured_sync_networks_excludes_subgraph_only_networks() {
        let mut parsed = parsed_settings();
        parsed.syncs.remove("network-b");

        let networks = configured_sync_networks(&parsed);

        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].key, "network-a");
        assert_eq!(networks[0].chain_id, 1);
    }

    #[test]
    fn parse_runner_settings_missing_sync_section() {
        let parsed =
            parse_runner_settings(&missing_sync_yaml()).expect("missing syncs is now allowed");
        assert!(
            parsed.syncs.is_empty(),
            "no sync configs should be parsed when the section is absent"
        );
    }

    #[test]
    fn parse_runner_settings_invalid_yaml() {
        let err = parse_runner_settings(minimal_invalid_yaml()).unwrap_err();
        let is_yaml_scan = matches!(err, LocalDbError::YamlScan(_));
        let is_missing_networks = matches!(
            err,
            LocalDbError::SettingsYaml(ref yaml_err)
                if matches!(yaml_err, raindex_app_settings::yaml::YamlError::Field { .. })
        );
        assert!(
            is_yaml_scan || is_missing_networks,
            "expected YAML parsing error, got {err:?}"
        );
    }

    #[test]
    fn map_sync_to_engine_success() {
        let sync = LocalDbSyncCfg {
            document: default_document(),
            key: "network-a".to_string(),
            batch_size: 25,
            max_concurrent_batches: 4,
            retry_attempts: 6,
            retry_delay_ms: 1000,
            rate_limit_delay_ms: 5000,
            finality_depth: 32,
            bootstrap_block_threshold: 100,
            sync_interval_ms: 5000,
        };

        let (fetch, finality) = map_sync_to_engine(&sync).expect("map succeeds");
        assert_eq!(fetch.chunk_size(), 25);
        assert_eq!(fetch.max_concurrent_requests(), 4);
        assert_eq!(fetch.max_concurrent_blocks(), 4);
        assert_eq!(fetch.max_retry_attempts(), 6);
        assert_eq!(fetch.retry_delay_ms(), 1000);
        assert_eq!(fetch.rate_limit_delay_ms(), 5000);
        assert_eq!(finality.depth, 32);
    }

    #[test]
    fn map_sync_to_engine_zero_batch_size_is_error() {
        let sync = LocalDbSyncCfg {
            batch_size: 0,
            ..LocalDbSyncCfg::default()
        };
        assert_fetch_config_error(sync, FetchConfigError::ChunkSizeZero(0));
    }

    #[test]
    fn map_sync_to_engine_zero_concurrency_is_error() {
        let sync = LocalDbSyncCfg {
            max_concurrent_batches: 0,
            ..LocalDbSyncCfg::default()
        };
        assert_fetch_config_error(sync, FetchConfigError::MaxConcurrentRequestsZero(0usize));
    }

    #[test]
    fn map_sync_to_engine_zero_retry_attempts_is_error() {
        let sync = LocalDbSyncCfg {
            retry_attempts: 0,
            ..LocalDbSyncCfg::default()
        };
        assert_fetch_config_error(sync, FetchConfigError::MaxRetryAttemptsZero(0));
    }

    #[test]
    fn build_runner_targets_success() {
        let parsed = parsed_settings();
        let targets =
            build_runner_targets(&parsed.raindexes, &parsed.syncs).expect("targets build");

        assert_eq!(targets.len(), 3);

        let target_a = targets
            .iter()
            .find(|t| t.raindex_key == "raindex-a")
            .expect("target for raindex-a exists");
        assert_eq!(target_a.network_key, "network-a");
        assert_eq!(target_a.inputs.raindex_id.chain_id, 1);
        assert_eq!(
            target_a.inputs.raindex_id.raindex_address,
            address!("00000000000000000000000000000000000000a1")
        );
        assert_eq!(target_a.inputs.cfg.deployment_block, 111);
        assert_eq!(target_a.inputs.cfg.fetch.chunk_size(), 10);
        assert_eq!(target_a.inputs.cfg.fetch.max_concurrent_requests(), 5);
        assert_eq!(target_a.inputs.cfg.fetch.max_concurrent_blocks(), 5);
        assert_eq!(target_a.inputs.cfg.fetch.max_retry_attempts(), 3);
        assert_eq!(target_a.inputs.cfg.fetch.retry_delay_ms(), 100);
        assert_eq!(target_a.inputs.cfg.fetch.rate_limit_delay_ms(), 50);
        assert_eq!(target_a.inputs.cfg.finality.depth, 12);
        assert_eq!(target_a.inputs.block_number_threshold, 10000);
        assert!(target_a.inputs.cfg.window_overrides.start_block.is_none());
        assert_eq!(
            target_a.inputs.metadata_rpcs,
            vec![Url::parse("https://rpc.network/a").expect("valid rpc url")]
        );
        assert_eq!(
            target_a.manifest_url,
            Url::parse("https://remotes.example.com/a.yaml").expect("valid manifest url")
        );
    }

    #[test]
    fn build_runner_targets_missing_sync_config_is_error() {
        let parsed = parsed_settings();
        let mut syncs = parsed.syncs.clone();
        syncs.remove("network-a");

        let err = build_runner_targets(&parsed.raindexes, &syncs).unwrap_err();
        match err {
            LocalDbError::MissingLocalDbSyncForNetwork { network } => {
                assert_eq!(network, "network-a")
            }
            other => panic!("expected MissingLocalDbSyncForNetwork, got {other:?}"),
        }
    }

    #[test]
    fn build_runner_targets_invalid_sync_config_propagates_error() {
        let parsed = parsed_settings();
        let mut syncs = parsed.syncs.clone();
        if let Some(sync) = syncs.get_mut("network-a") {
            sync.batch_size = 0;
        }

        let err = build_runner_targets(&parsed.raindexes, &syncs).unwrap_err();
        match err {
            LocalDbError::FetchConfigError(FetchConfigError::ChunkSizeZero(0)) => {}
            other => panic!("expected fetch config error, got {other:?}"),
        }
    }

    #[test]
    fn build_sync_inputs_from_yaml_success() {
        let inputs =
            build_sync_inputs_from_yaml(&sample_settings_yaml()).expect("inputs build succeeds");
        assert_eq!(inputs.len(), 3);

        let input_b = inputs
            .iter()
            .find(|input| {
                input.raindex_id.raindex_address
                    == address!("00000000000000000000000000000000000000b2")
            })
            .expect("input for raindex-b exists");
        assert_eq!(input_b.raindex_id.chain_id, 2);
        assert_eq!(input_b.cfg.fetch.max_concurrent_requests(), 2);
    }

    #[test]
    fn build_sync_inputs_from_yaml_propagates_parse_error() {
        let err = build_sync_inputs_from_yaml(minimal_invalid_yaml()).unwrap_err();
        let is_yaml_scan = matches!(err, LocalDbError::YamlScan(_));
        let is_settings_yaml = matches!(err, LocalDbError::SettingsYaml(_));
        assert!(
            is_yaml_scan || is_settings_yaml,
            "expected YAML parsing error, got {err:?}"
        );
    }

    #[test]
    fn group_targets_by_network_groups_correctly() {
        let parsed = parsed_settings();
        let targets = build_runner_targets(&parsed.raindexes, &parsed.syncs).unwrap();
        let grouped = group_targets_by_network(&targets);

        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped.get("network-a").map(|v| v.len()), Some(2));
        assert_eq!(grouped.get("network-b").map(|v| v.len()), Some(1));
    }

    #[test]
    fn group_targets_by_network_empty_input() {
        let grouped = group_targets_by_network(&[]);
        assert!(grouped.is_empty());
    }
}
