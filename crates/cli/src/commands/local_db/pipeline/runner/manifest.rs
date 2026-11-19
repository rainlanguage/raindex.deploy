use super::export::ExportMetadata;
use alloy::primitives::{Address, Bytes};
use raindex_app_settings::local_db_manifest::{LocalDbManifest, ManifestRaindex};
use raindex_app_settings::remote::manifest::ManifestMap;
use raindex_common::local_db::pipeline::runner::utils::RunnerTarget;
use raindex_common::local_db::pipeline::runner::TargetSuccess;
use raindex_common::local_db::{LocalDbError, RaindexIdentifier};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use tokio::fs;
use url::Url;

/// Builds a manifest by overlaying successful producer outcomes on top of any
/// previous manifest entries. New dumps replace old ones; failed or skipped
/// targets keep their last known manifest entry when available.
pub fn build_manifest(
    successes: &[TargetSuccess],
    exports: &HashMap<RaindexIdentifier, Option<ExportMetadata>>,
    target_lookup: &HashMap<RaindexIdentifier, RunnerTarget>,
    release_base_url: &Url,
    previous_manifests: &ManifestMap,
) -> Result<LocalDbManifest, LocalDbError> {
    let mut per_network: HashMap<String, (u32, Vec<ManifestRaindex>)> = HashMap::new();

    // First, record the latest successful exports.
    for success in successes {
        let raindex_id = &success.outcome.raindex_id;
        let export = match exports.get(raindex_id).and_then(|export| export.as_ref()) {
            Some(export) => export,
            None => continue,
        };
        let runner_target =
            target_lookup
                .get(raindex_id)
                .ok_or_else(|| LocalDbError::MissingRunnerTarget {
                    chain_id: raindex_id.chain_id,
                    raindex_address: raindex_id.raindex_address,
                })?;

        let dump_url = build_dump_url(
            release_base_url,
            raindex_id.chain_id,
            raindex_id.raindex_address,
        )?;
        let end_block_hash = Bytes::from_str(export.end_block_hash.as_str())?;

        let manifest_raindex = ManifestRaindex {
            address: raindex_id.raindex_address,
            dump_url,
            end_block: export.end_block,
            end_block_hash,
            end_block_time_ms: export.end_block_time_ms,
        };

        push_entry(
            &mut per_network,
            runner_target.network_key.clone(),
            raindex_id.chain_id,
            manifest_raindex,
        )?;
    }

    // Then, carry forward any existing manifest entries for targets that did
    // not produce a new dump in this run.
    for (raindex_id, runner_target) in target_lookup {
        let already_recorded =
            per_network
                .get(&runner_target.network_key)
                .is_some_and(|(_, raindexes)| {
                    raindexes
                        .iter()
                        .any(|raindex| raindex.address == raindex_id.raindex_address)
                });
        if already_recorded {
            continue;
        }

        let previous_entry = previous_manifests
            .get(&runner_target.manifest_url)
            .and_then(|manifest| manifest.find(raindex_id.chain_id, raindex_id.raindex_address))
            .cloned();

        if let Some(raindex) = previous_entry {
            push_entry(
                &mut per_network,
                runner_target.network_key.clone(),
                raindex_id.chain_id,
                raindex,
            )?;
        }
    }

    let mut manifest = LocalDbManifest::new();
    let mut network_keys: Vec<_> = per_network.keys().cloned().collect();
    network_keys.sort();

    for key in network_keys {
        if let Some((chain_id, mut raindexes)) = per_network.remove(&key) {
            raindexes.sort_by(|a, b| a.address.cmp(&b.address));
            manifest.add_network(&key, chain_id)?;
            for raindex in raindexes {
                manifest.push_raindex(&key, raindex)?;
            }
        }
    }

    Ok(manifest)
}

fn push_entry(
    per_network: &mut HashMap<String, (u32, Vec<ManifestRaindex>)>,
    network_key: String,
    chain_id: u32,
    raindex: ManifestRaindex,
) -> Result<(), LocalDbError> {
    let entry = per_network
        .entry(network_key.clone())
        .or_insert_with(|| (chain_id, Vec::new()));
    if entry.0 != chain_id {
        return Err(LocalDbError::RunnerNetworkChainIdMismatch {
            network_key,
            expected: entry.0,
            found: chain_id,
        });
    }
    entry.1.push(raindex);
    Ok(())
}

fn build_dump_url(
    base_url: &Url,
    chain_id: u32,
    raindex_address: Address,
) -> Result<Url, LocalDbError> {
    let base = base_url.as_str().trim_end_matches('/');
    let address_str = raindex_address.to_string();
    let url_str = format!("{}/{}-{}.sql.gz", base, chain_id, address_str);
    Url::parse(&url_str).map_err(|source| LocalDbError::DumpUrlConstructionFailed {
        url: url_str,
        source,
    })
}

/// Writes the manifest to disk, creating parent directories if necessary.
pub async fn write_manifest_to_path(
    manifest: &LocalDbManifest,
    path: &Path,
) -> Result<(), LocalDbError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let yaml = manifest.to_yaml_string()?;
    fs::write(path, yaml).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::local_db::pipeline::runner::export::ExportMetadata;
    use alloy::primitives::address;
    use raindex_app_settings::local_db_manifest::{
        LocalDbManifest, ManifestNetwork, ManifestRaindex, DB_SCHEMA_VERSION, MANIFEST_VERSION,
    };
    use raindex_common::local_db::pipeline::engine::SyncInputs;
    use raindex_common::local_db::pipeline::{
        FinalityConfig, SyncConfig, SyncOutcome, WindowOverrides,
    };
    use raindex_common::local_db::{FetchConfig, RaindexIdentifier};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn sample_runner_target(network_key: &str, chain_id: u32, address: Address) -> RunnerTarget {
        let fetch = FetchConfig::new(10, 5, 5, 1, 0, 0).unwrap();
        RunnerTarget {
            raindex_key: format!("{}-{}", network_key, address),
            manifest_url: Url::parse("https://example.com/manifest.yaml").unwrap(),
            network_key: network_key.to_string(),
            inputs: SyncInputs {
                raindex_id: RaindexIdentifier {
                    chain_id,
                    raindex_address: address,
                },
                metadata_rpcs: Vec::new(),
                cfg: SyncConfig {
                    deployment_block: 0,
                    fetch,
                    finality: FinalityConfig { depth: 12 },
                    window_overrides: WindowOverrides::default(),
                    max_blocks_per_cycle: None,
                },
                dump_str: None,
                defer_analyze: false,
                block_number_threshold: 10000,
                manifest_end_block: 1,
            },
        }
    }

    fn sample_success(
        target: &RaindexIdentifier,
        dump_suffix: &str,
    ) -> (TargetSuccess, ExportMetadata) {
        (
            TargetSuccess {
                outcome: SyncOutcome {
                    raindex_id: target.clone(),
                    start_block: 0,
                    target_block: 1234,
                    fetched_logs: 10,
                    decoded_events: 5,
                    caught_up: true,
                },
            },
            ExportMetadata {
                dump_path: Path::new(dump_suffix).to_path_buf(),
                end_block: 1234,
                end_block_hash: "0xfeedface".to_string(),
                end_block_time_ms: 1_700_000_000,
            },
        )
    }

    #[test]
    fn build_manifest_skips_missing_exported_dump() {
        let target_included = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: address!("0x0000000000000000000000000000000000000aa1"),
        };
        let target_skipped = RaindexIdentifier {
            chain_id: 10,
            raindex_address: address!("0x0000000000000000000000000000000000000bb2"),
        };

        let mut lookup: HashMap<RaindexIdentifier, RunnerTarget> = HashMap::new();
        lookup.insert(
            target_included.clone(),
            sample_runner_target(
                "arbitrum",
                target_included.chain_id,
                target_included.raindex_address,
            ),
        );
        lookup.insert(
            target_skipped.clone(),
            sample_runner_target(
                "optimism",
                target_skipped.chain_id,
                target_skipped.raindex_address,
            ),
        );

        let (skipped_success, _) = sample_success(&target_skipped, "skipped.sql.gz");
        let (included_success, included_export) =
            sample_success(&target_included, "included.sql.gz");

        let mut exports = HashMap::new();
        exports.insert(target_skipped.clone(), None);
        exports.insert(target_included.clone(), Some(included_export));
        let successes = vec![skipped_success, included_success];

        let base_url = Url::parse("https://releases.example.com").unwrap();
        let manifest = build_manifest(&successes, &exports, &lookup, &base_url, &HashMap::new())
            .expect("manifest build succeeds");

        assert!(
            !manifest.networks.contains_key("optimism"),
            "raindex without dump should be ignored"
        );
        let arbitrum = manifest
            .networks
            .get("arbitrum")
            .expect("network with dump should exist");
        assert_eq!(arbitrum.raindexes.len(), 1);
        assert_eq!(
            arbitrum.raindexes[0].address,
            target_included.raindex_address
        );
    }

    #[test]
    fn build_manifest_happy_path_multiple_networks() {
        let target_a = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: address!("0x0000000000000000000000000000000000000Aa1"),
        };
        let target_b = RaindexIdentifier {
            chain_id: 10,
            raindex_address: address!("0x0000000000000000000000000000000000000Bb2"),
        };

        let mut lookup: HashMap<RaindexIdentifier, RunnerTarget> = HashMap::new();
        lookup.insert(
            target_a.clone(),
            sample_runner_target("anvil", target_a.chain_id, target_a.raindex_address),
        );
        lookup.insert(
            target_b.clone(),
            sample_runner_target("optimism", target_b.chain_id, target_b.raindex_address),
        );

        let (success_a, export_a) = sample_success(&target_a, "dump-a.sql.gz");
        let (success_b, export_b) = sample_success(&target_b, "dump-b.sql.gz");
        let successes = vec![success_a, success_b];
        let mut exports = HashMap::new();
        exports.insert(target_a.clone(), Some(export_a));
        exports.insert(target_b.clone(), Some(export_b));

        let base_url = Url::parse("https://releases.example.com").unwrap();
        let manifest = build_manifest(&successes, &exports, &lookup, &base_url, &HashMap::new())
            .expect("manifest build succeeds");

        assert_eq!(manifest.manifest_version, 1);
        assert_eq!(manifest.db_schema_version, DB_SCHEMA_VERSION);
        assert_eq!(manifest.networks.len(), 2);

        let anvil = manifest.networks.get("anvil").expect("anvil network");
        assert_eq!(anvil.chain_id, 42161);
        assert_eq!(anvil.raindexes.len(), 1);
        let expected_anvil = format!(
            "https://releases.example.com/{}-{}.sql.gz",
            target_a.chain_id, target_a.raindex_address
        );
        assert_eq!(anvil.raindexes[0].dump_url.as_str(), expected_anvil);

        let optimism = manifest.networks.get("optimism").expect("optimism network");
        assert_eq!(optimism.chain_id, 10);
        assert_eq!(optimism.raindexes.len(), 1);
        let expected_optimism = format!(
            "https://releases.example.com/{}-{}.sql.gz",
            target_b.chain_id, target_b.raindex_address
        );
        assert_eq!(optimism.raindexes[0].dump_url.as_str(), expected_optimism);
    }

    #[test]
    fn build_manifest_errors_on_chain_id_mismatch() {
        let target_a = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: address!("0x0000000000000000000000000000000000000cc1"),
        };
        let target_b = RaindexIdentifier {
            chain_id: 10,
            raindex_address: address!("0x0000000000000000000000000000000000000cc2"),
        };

        let mut lookup: HashMap<RaindexIdentifier, RunnerTarget> = HashMap::new();
        lookup.insert(
            target_a.clone(),
            sample_runner_target("shared", target_a.chain_id, target_a.raindex_address),
        );
        lookup.insert(
            target_b.clone(),
            sample_runner_target("shared", target_b.chain_id, target_b.raindex_address),
        );

        let (success_a, export_a) = sample_success(&target_a, "dump-a.sql.gz");
        let (success_b, export_b) = sample_success(&target_b, "dump-b.sql.gz");
        let successes = vec![success_a, success_b];
        let mut exports = HashMap::new();
        exports.insert(target_a.clone(), Some(export_a));
        exports.insert(target_b.clone(), Some(export_b));

        let base_url = Url::parse("https://releases.example.com").unwrap();
        let err = build_manifest(&successes, &exports, &lookup, &base_url, &HashMap::new())
            .expect_err("shared network key with different chain ids should error");

        match err {
            LocalDbError::RunnerNetworkChainIdMismatch {
                network_key,
                expected,
                found,
            } => {
                assert_eq!(network_key, "shared");
                assert_eq!(expected, target_a.chain_id);
                assert_eq!(found, target_b.chain_id);
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn build_manifest_sorts_raindexes_within_network() {
        let target_a = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: address!("0x0000000000000000000000000000000000000aa2"),
        };
        let target_b = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: address!("0x0000000000000000000000000000000000000aa1"),
        };

        let mut lookup: HashMap<RaindexIdentifier, RunnerTarget> = HashMap::new();
        lookup.insert(
            target_a.clone(),
            sample_runner_target("anvil", target_a.chain_id, target_a.raindex_address),
        );
        lookup.insert(
            target_b.clone(),
            sample_runner_target("anvil", target_b.chain_id, target_b.raindex_address),
        );

        // Intentionally provide outcomes out of order to ensure sorting occurs.
        let (success_a, export_a) = sample_success(&target_a, "dump-a.sql.gz");
        let (success_b, export_b) = sample_success(&target_b, "dump-b.sql.gz");
        let successes = vec![success_a, success_b];
        let mut exports = HashMap::new();
        exports.insert(target_a.clone(), Some(export_a));
        exports.insert(target_b.clone(), Some(export_b));

        let base_url = Url::parse("https://releases.example.com").unwrap();
        let manifest = build_manifest(&successes, &exports, &lookup, &base_url, &HashMap::new())
            .expect("manifest build succeeds");
        let anvil = manifest.networks.get("anvil").expect("anvil network");
        let addresses: Vec<_> = anvil
            .raindexes
            .iter()
            .map(|raindex_entry| raindex_entry.address)
            .collect();
        assert_eq!(
            addresses,
            vec![
                address!("0x0000000000000000000000000000000000000aa1"),
                address!("0x0000000000000000000000000000000000000aa2")
            ]
        );
    }

    #[test]
    fn build_manifest_errors_on_missing_target() {
        let raindex_id = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: address!("0x0000000000000000000000000000000000000aa1"),
        };
        let lookup: HashMap<RaindexIdentifier, RunnerTarget> = HashMap::new();
        let (success, export) = sample_success(&raindex_id, "dump.sql.gz");
        let successes = vec![success];
        let mut exports = HashMap::new();
        exports.insert(raindex_id.clone(), Some(export));

        let base_url = Url::parse("https://releases.example.com").unwrap();

        let err = build_manifest(&successes, &exports, &lookup, &base_url, &HashMap::new())
            .expect_err("missing target should error");
        match err {
            LocalDbError::MissingRunnerTarget {
                chain_id,
                raindex_address,
            } => {
                assert_eq!(chain_id, raindex_id.chain_id);
                assert_eq!(raindex_address, raindex_id.raindex_address);
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn build_manifest_errors_on_invalid_end_block_hash() {
        let raindex_id = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: address!("0x0000000000000000000000000000000000000dd1"),
        };

        let mut lookup: HashMap<RaindexIdentifier, RunnerTarget> = HashMap::new();
        lookup.insert(
            raindex_id.clone(),
            sample_runner_target("arbitrum", raindex_id.chain_id, raindex_id.raindex_address),
        );

        let (success, mut export) = sample_success(&raindex_id, "dump.sql.gz");
        export.end_block_hash = "not-a-hex-string".to_string();
        let successes = vec![success];
        let mut exports = HashMap::new();
        exports.insert(raindex_id.clone(), Some(export));

        let base_url = Url::parse("https://releases.example.com").unwrap();
        let err = build_manifest(&successes, &exports, &lookup, &base_url, &HashMap::new())
            .expect_err("invalid hash should error");

        match err {
            LocalDbError::FromHexError(_) => {}
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn build_manifest_returns_empty_manifest_for_no_successes() {
        let base_url = Url::parse("https://releases.example.com").unwrap();
        let manifest = build_manifest(
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &base_url,
            &HashMap::new(),
        )
        .expect("empty input succeeds");

        assert!(manifest.networks.is_empty());
        assert_eq!(manifest.manifest_version, 1);
        assert_eq!(manifest.db_schema_version, DB_SCHEMA_VERSION);
    }

    #[test]
    fn build_manifest_overlays_successes_and_reuses_previous_entries() {
        let target_a = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: address!("0x0000000000000000000000000000000000000aa1"),
        };
        let target_b = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: address!("0x0000000000000000000000000000000000000bb2"),
        };

        let mut lookup: HashMap<RaindexIdentifier, RunnerTarget> = HashMap::new();
        lookup.insert(
            target_a.clone(),
            sample_runner_target("anvil", target_a.chain_id, target_a.raindex_address),
        );
        lookup.insert(
            target_b.clone(),
            sample_runner_target("anvil", target_b.chain_id, target_b.raindex_address),
        );

        let (success_a, export_a) = sample_success(&target_a, "dump-a.sql.gz");
        let successes = vec![success_a];
        let mut exports = HashMap::new();
        exports.insert(target_a.clone(), Some(export_a));

        // Previous manifest contains both raindexes; only A gets a fresh export.
        let previous_manifest = LocalDbManifest {
            manifest_version: MANIFEST_VERSION,
            db_schema_version: DB_SCHEMA_VERSION,
            networks: HashMap::from([(
                "anvil".to_string(),
                ManifestNetwork {
                    chain_id: target_a.chain_id,
                    raindexes: vec![
                        ManifestRaindex {
                            address: target_a.raindex_address,
                            dump_url: Url::parse("https://old.example/a.sql.gz").unwrap(),
                            end_block: 100,
                            end_block_hash: Bytes::from_str("0x0aaa").unwrap(),
                            end_block_time_ms: 1_700_000_000,
                        },
                        ManifestRaindex {
                            address: target_b.raindex_address,
                            dump_url: Url::parse("https://old.example/b.sql.gz").unwrap(),
                            end_block: 200,
                            end_block_hash: Bytes::from_str("0x0bbb").unwrap(),
                            end_block_time_ms: 1_700_000_100,
                        },
                    ],
                },
            )]),
        };
        let manifest_url = Url::parse("https://example.com/manifest.yaml").unwrap();
        let previous_map: ManifestMap = HashMap::from([(manifest_url, previous_manifest)]);

        let base_url = Url::parse("https://releases.example.com").unwrap();
        let manifest = build_manifest(&successes, &exports, &lookup, &base_url, &previous_map)
            .expect("manifest build succeeds");

        let anvil = manifest.networks.get("anvil").expect("anvil network");
        assert_eq!(anvil.raindexes.len(), 2);
        // New dump for target A should use the release base URL, while target B
        // should keep its previous dump URL.
        assert_eq!(
            anvil.raindexes[0].dump_url.as_str(),
            format!(
                "https://releases.example.com/{}-{}.sql.gz",
                target_a.chain_id, target_a.raindex_address
            )
        );
        assert_eq!(
            anvil.raindexes[1].dump_url.as_str(),
            "https://old.example/b.sql.gz"
        );
    }

    #[test]
    fn build_manifest_orders_networks_alphabetically() {
        let target_devnet = RaindexIdentifier {
            chain_id: 1,
            raindex_address: address!("0x0000000000000000000000000000000000000aa1"),
        };
        let target_testnet = RaindexIdentifier {
            chain_id: 10,
            raindex_address: address!("0x0000000000000000000000000000000000000bb1"),
        };

        let mut lookup: HashMap<RaindexIdentifier, RunnerTarget> = HashMap::new();
        lookup.insert(
            target_devnet.clone(),
            sample_runner_target(
                "devnet",
                target_devnet.chain_id,
                target_devnet.raindex_address,
            ),
        );
        lookup.insert(
            target_testnet.clone(),
            sample_runner_target(
                "testnet",
                target_testnet.chain_id,
                target_testnet.raindex_address,
            ),
        );

        let (success_testnet, export_testnet) =
            sample_success(&target_testnet, "testnet-dump.sql.gz");
        let (success_devnet, export_devnet) = sample_success(&target_devnet, "devnet-dump.sql.gz");
        let successes = vec![success_testnet, success_devnet];
        let mut exports = HashMap::new();
        exports.insert(target_testnet.clone(), Some(export_testnet));
        exports.insert(target_devnet.clone(), Some(export_devnet));

        let base_url = Url::parse("https://releases.example.com/").unwrap();
        let manifest = build_manifest(&successes, &exports, &lookup, &base_url, &HashMap::new())
            .expect("manifest build succeeds");

        let yaml = manifest
            .to_yaml_string()
            .expect("yaml serialization succeeds");
        let devnet_pos = yaml.find("devnet").expect("devnet key present");
        let testnet_pos = yaml.find("testnet").expect("testnet key present");
        assert!(
            devnet_pos < testnet_pos,
            "expected alphabetical ordering, got yaml: {yaml}"
        );
    }

    #[test]
    fn build_dump_url_trims_trailing_slash() {
        let base_url = Url::parse("https://releases.example.com/releases/").unwrap();
        let address = address!("0x0000000000000000000000000000000000000fff");

        let url =
            build_dump_url(&base_url, 42161, address).expect("url construction should succeed");
        let expected = format!(
            "https://releases.example.com/releases/42161-{}.sql.gz",
            address
        );
        assert_eq!(url.as_str(), expected);
    }

    #[tokio::test]
    async fn write_manifest_to_path_writes_yaml() {
        let raindex_id = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: address!("0x0000000000000000000000000000000000000aa1"),
        };
        let mut lookup: HashMap<RaindexIdentifier, RunnerTarget> = HashMap::new();
        lookup.insert(
            raindex_id.clone(),
            sample_runner_target("anvil", raindex_id.chain_id, raindex_id.raindex_address),
        );
        let (success, export) = sample_success(&raindex_id, "dump.sql.gz");
        let successes = vec![success];
        let mut exports = HashMap::new();
        exports.insert(raindex_id.clone(), Some(export));

        let base_url = Url::parse("https://releases.example.com").unwrap();
        let manifest = build_manifest(&successes, &exports, &lookup, &base_url, &HashMap::new())
            .expect("manifest build succeeds");

        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("nested/manifest.yaml");
        write_manifest_to_path(&manifest, &manifest_path)
            .await
            .expect("write succeeds");

        let contents = std::fs::read_to_string(&manifest_path).expect("read manifest");
        assert!(
            contents.contains("manifest-version"),
            "expected manifest header"
        );
        assert!(contents.contains("anvil"), "expected network key");
    }
}
