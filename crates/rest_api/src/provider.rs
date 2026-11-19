use raindex_common::{raindex_client::RaindexClient, registry::DotrainRegistry};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct RaindexProvider {
    client: RaindexClient,
}

impl RaindexProvider {
    pub async fn load(registry_url: &str, db_path: PathBuf) -> Result<Self, ProviderError> {
        ensure_parent_directory(&db_path)?;
        let registry = DotrainRegistry::new(registry_url.to_string())
            .await
            .map_err(|error| ProviderError::Registry(error.to_string()))?;
        let client = registry
            .get_raindex_client(Some(db_path.clone()))
            .await
            .map_err(|error| ProviderError::Client(error.to_string()))?;
        Ok(Self { client })
    }

    pub fn client(&self) -> &RaindexClient {
        &self.client
    }
}

fn ensure_parent_directory(path: &Path) -> Result<(), ProviderError> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(std::fs::create_dir_all)
        .transpose()
        .map(|_| ())
        .map_err(ProviderError::DatabaseDirectory)
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("failed to prepare local database directory: {0}")]
    DatabaseDirectory(std::io::Error),
    #[error("failed to load registry: {0}")]
    Registry(String),
    #[error("failed to initialize Raindex client: {0}")]
    Client(String),
}
