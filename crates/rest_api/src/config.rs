use std::path::PathBuf;

pub const DEFAULT_REGISTRY_URL: &str = "https://raw.githubusercontent.com/rainlanguage/rain.strategies/e189647e34e865fc62ee78720a8bd7a3d199270f/registry";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub registry_url: String,
    pub local_db_path: PathBuf,
    pub log_dir: PathBuf,
    pub local_db_ready_timeout_seconds: u64,
    pub cache_ttl_seconds: u64,
    pub rate_limit_global_rpm: u64,
    pub rate_limit_per_ip_rpm: u64,
    pub snapshot_recent_trades_limit: u16,
    pub trusted_proxy_ip_header: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            registry_url: DEFAULT_REGISTRY_URL.to_string(),
            local_db_path: PathBuf::from(".raindex/market-data.sqlite"),
            log_dir: PathBuf::from(".raindex/logs"),
            local_db_ready_timeout_seconds: 600,
            cache_ttl_seconds: 60,
            rate_limit_global_rpm: 6_000,
            rate_limit_per_ip_rpm: 120,
            snapshot_recent_trades_limit: 20,
            trusted_proxy_ip_header: None,
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let defaults = Self::default();
        let config = Self {
            registry_url: env_string("RAINDEX_REGISTRY_URL", defaults.registry_url),
            local_db_path: PathBuf::from(env_string(
                "RAINDEX_LOCAL_DB_PATH",
                defaults.local_db_path.to_string_lossy().into_owned(),
            )),
            log_dir: PathBuf::from(env_string(
                "RAINDEX_LOG_DIR",
                defaults.log_dir.to_string_lossy().into_owned(),
            )),
            local_db_ready_timeout_seconds: env_parse(
                "RAINDEX_LOCAL_DB_READY_TIMEOUT_SECONDS",
                defaults.local_db_ready_timeout_seconds,
            )?,
            cache_ttl_seconds: env_parse("RAINDEX_CACHE_TTL_SECONDS", defaults.cache_ttl_seconds)?,
            rate_limit_global_rpm: env_parse(
                "RAINDEX_RATE_LIMIT_GLOBAL_RPM",
                defaults.rate_limit_global_rpm,
            )?,
            rate_limit_per_ip_rpm: env_parse(
                "RAINDEX_RATE_LIMIT_PER_IP_RPM",
                defaults.rate_limit_per_ip_rpm,
            )?,
            snapshot_recent_trades_limit: env_parse(
                "RAINDEX_SNAPSHOT_RECENT_TRADES_LIMIT",
                defaults.snapshot_recent_trades_limit,
            )?,
            trusted_proxy_ip_header: std::env::var("RAINDEX_TRUSTED_PROXY_IP_HEADER")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        };
        if config.cache_ttl_seconds == 0 {
            return Err(ConfigError::InvalidValue(
                "RAINDEX_CACHE_TTL_SECONDS",
                "0".into(),
            ));
        }
        if config.local_db_ready_timeout_seconds == 0 {
            return Err(ConfigError::InvalidValue(
                "RAINDEX_LOCAL_DB_READY_TIMEOUT_SECONDS",
                "0".into(),
            ));
        }
        Ok(config)
    }
}

fn env_string(name: &'static str, default: String) -> String {
    std::env::var(name).unwrap_or(default)
}

fn env_parse<T>(name: &'static str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
{
    std::env::var(name)
        .ok()
        .map(|value| {
            value
                .parse()
                .map_err(|_| ConfigError::InvalidValue(name, value))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid value for {0}: {1}")]
    InvalidValue(&'static str, String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_suitable_for_minutely_public_polling() {
        let config = Config::default();
        assert_eq!(config.cache_ttl_seconds, 60);
        assert_eq!(config.local_db_ready_timeout_seconds, 600);
        assert!(config.rate_limit_per_ip_rpm >= 60);
        assert_eq!(config.snapshot_recent_trades_limit, 20);
        assert_eq!(crate::cache::MAX_ORDERBOOK_DEPTH, 1_000);
    }
}
