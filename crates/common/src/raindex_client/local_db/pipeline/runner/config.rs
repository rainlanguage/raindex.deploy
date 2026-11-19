use crate::local_db::pipeline::runner::utils::{
    build_runner_targets, ParsedRunnerSettings, RunnerTarget,
};
use crate::local_db::LocalDbError;
use raindex_app_settings::local_db_sync::LocalDbSyncCfg;
use raindex_app_settings::raindex::RaindexCfg;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct NetworkRunnerConfig {
    pub network_key: String,
    pub chain_id: u32,
    pub settings: ParsedRunnerSettings,
}

impl NetworkRunnerConfig {
    pub fn from_global_settings(
        global: &ParsedRunnerSettings,
        network_key: &str,
    ) -> Result<Self, LocalDbError> {
        let filtered_raindexes: HashMap<String, RaindexCfg> = global
            .raindexes
            .iter()
            .filter(|(_, raindex_cfg)| raindex_cfg.network.key == network_key)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if filtered_raindexes.is_empty() {
            return Err(LocalDbError::CustomError(format!(
                "no raindexes found for network: {}",
                network_key
            )));
        }

        let chain_id = filtered_raindexes
            .values()
            .next()
            .map(|raindex_cfg| raindex_cfg.network.chain_id)
            .ok_or_else(|| {
                LocalDbError::CustomError(format!(
                    "could not determine chain_id for network: {}",
                    network_key
                ))
            })?;

        let filtered_syncs: HashMap<String, LocalDbSyncCfg> = global
            .syncs
            .iter()
            .filter(|(k, _)| *k == network_key)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Ok(Self {
            network_key: network_key.to_string(),
            chain_id,
            settings: ParsedRunnerSettings {
                raindexes: filtered_raindexes,
                syncs: filtered_syncs,
            },
        })
    }

    /// Builds this network's targets with a per-cycle block cap of
    /// [`MAX_CHUNKS_PER_CLIENT_CYCLE`] fetch chunks, so a client whose cycles
    /// keep failing re-reads a bounded range instead of everything since its
    /// last persisted block.
    pub fn build_targets(&self) -> Result<Vec<RunnerTarget>, LocalDbError> {
        let mut targets = build_runner_targets(&self.settings.raindexes, &self.settings.syncs)?;
        for target in &mut targets {
            let chunk_size = target.inputs.cfg.fetch.chunk_size();
            target.inputs.cfg.max_blocks_per_cycle =
                Some(chunk_size.saturating_mul(MAX_CHUNKS_PER_CLIENT_CYCLE));
        }
        Ok(targets)
    }
}

/// Most fetch chunks (`batch-size` blocks each) one client sync cycle covers.
/// A bootstrapped client is normally a few chunks behind, so this only bites
/// when a client falls far behind or keeps failing.
pub const MAX_CHUNKS_PER_CLIENT_CYCLE: u64 = 100;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_db::pipeline::runner::utils::parse_runner_settings;
    use raindex_app_settings::spec_version::SpecVersion;

    fn sample_settings_yaml() -> String {
        format!(
            r#"
version: {}
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
            SpecVersion::current()
        )
    }

    #[test]
    fn from_global_settings_filters_raindexes_for_network() {
        let global = parse_runner_settings(&sample_settings_yaml()).expect("valid yaml");
        let config =
            NetworkRunnerConfig::from_global_settings(&global, "network-a").expect("config ok");

        assert_eq!(config.network_key, "network-a");
        assert_eq!(config.chain_id, 1);
        assert_eq!(config.settings.raindexes.len(), 2);
        assert!(config.settings.raindexes.contains_key("raindex-a"));
        assert!(config.settings.raindexes.contains_key("raindex-c"));
        assert!(!config.settings.raindexes.contains_key("raindex-b"));
    }

    #[test]
    fn from_global_settings_filters_syncs_for_network() {
        let global = parse_runner_settings(&sample_settings_yaml()).expect("valid yaml");
        let config =
            NetworkRunnerConfig::from_global_settings(&global, "network-b").expect("config ok");

        assert_eq!(config.settings.syncs.len(), 1);
        assert!(config.settings.syncs.contains_key("network-b"));
        assert!(!config.settings.syncs.contains_key("network-a"));
    }

    #[test]
    fn from_global_settings_errors_for_unknown_network() {
        let global = parse_runner_settings(&sample_settings_yaml()).expect("valid yaml");
        let err = NetworkRunnerConfig::from_global_settings(&global, "network-unknown")
            .expect_err("should error");

        assert!(matches!(err, LocalDbError::CustomError(_)));
    }

    #[test]
    fn build_targets_returns_targets_for_network_only() {
        let global = parse_runner_settings(&sample_settings_yaml()).expect("valid yaml");
        let config =
            NetworkRunnerConfig::from_global_settings(&global, "network-a").expect("config ok");

        let targets = config.build_targets().expect("targets ok");
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().all(|t| t.network_key == "network-a"));
    }

    #[test]
    fn build_targets_caps_blocks_per_cycle_from_batch_size() {
        let global = parse_runner_settings(&sample_settings_yaml()).expect("valid yaml");
        let config =
            NetworkRunnerConfig::from_global_settings(&global, "network-a").expect("config ok");

        for target in config.build_targets().expect("targets ok") {
            let chunk_size = target.inputs.cfg.fetch.chunk_size();
            assert_eq!(
                target.inputs.cfg.max_blocks_per_cycle,
                Some(chunk_size * MAX_CHUNKS_PER_CLIENT_CYCLE)
            );
        }
    }

    #[test]
    fn build_targets_single_raindex_network() {
        let global = parse_runner_settings(&sample_settings_yaml()).expect("valid yaml");
        let config =
            NetworkRunnerConfig::from_global_settings(&global, "network-b").expect("config ok");

        let targets = config.build_targets().expect("targets ok");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].raindex_key, "raindex-b");
    }
}
