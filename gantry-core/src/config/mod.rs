pub mod credentials;
pub mod project;
pub mod provider;

use std::path::Path;

use anyhow::{Context, Result};

pub use credentials::{ApiKeyCredential, Credential, CredentialsCatalog, OauthCredential, StoredCredential};
pub use project::Project;
pub use provider::{
    CopilotProviderConfig, OllamaProviderConfig, OpenAiCompletionsProviderConfig,
    OpenAiResponsesProviderConfig, ProviderConfig, ProviderConfigCatalog,
};

use crate::dirs::GlobalConfigDir;
use crate::provider::ProviderAlias;

/// Loads application configuration from `~/.gantry/`.
pub struct ConfigLoader {
    config_path: std::path::PathBuf,
    credentials_path: std::path::PathBuf,
}

impl ConfigLoader {
    /// Creates a new [`ConfigLoader`] using the global config directory.
    pub fn new() -> Result<Self> {
        let dir = GlobalConfigDir::new()?;
        Ok(Self {
            config_path: dir.config_path(),
            credentials_path: dir.credentials_path(),
        })
    }

    /// Loads and returns the [`ProviderConfigCatalog`] from `config.toml`.
    ///
    /// Returns an empty catalog if the file does not exist.
    pub fn load_provider_catalog(&self) -> Result<ProviderConfigCatalog> {
        load_toml_or_default(&self.config_path)
    }

    /// Loads and returns the [`CredentialsCatalog`] from `credentials.toml`.
    ///
    /// Returns an empty catalog if the file does not exist.
    pub fn load_credentials(&self) -> Result<CredentialsCatalog> {
        load_toml_or_default(&self.credentials_path)
    }

    /// Writes a single credential entry to `credentials.toml`, preserving all other entries.
    pub fn save_credential(&self, alias: &ProviderAlias, credential: &StoredCredential) -> Result<()> {
        let raw = if self.credentials_path.exists() {
            std::fs::read_to_string(&self.credentials_path)
                .context("failed to read credentials.toml")?
        } else {
            String::new()
        };

        let mut doc = raw
            .parse::<toml_edit::DocumentMut>()
            .context("failed to parse credentials.toml")?;

        let value = toml_edit::ser::to_document(credential)
            .context("failed to serialize credential")?;
        doc[alias.as_str()] = toml_edit::Item::Table(value.as_table().clone());

        write_secret_file(&self.credentials_path, &doc.to_string())
    }
}

/// Deserializes a TOML file into `T`, returning `T::default()` if the file does not exist.
fn load_toml_or_default<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
}

/// Writes `contents` to `path` with `0600` permissions, creating parent directories as needed.
fn write_secret_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    // Write first, then restrict permissions.
    std::fs::write(path, contents)
        .with_context(|| format!("failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }

    Ok(())
}
