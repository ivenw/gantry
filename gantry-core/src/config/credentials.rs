use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::provider::ProviderAlias;

/// The full set of credentials, keyed by provider alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialsCatalog {
    #[serde(skip)]
    path: PathBuf,
    #[serde(flatten)]
    entries: HashMap<ProviderAlias, StoredCredential>,
}

impl CredentialsCatalog {
    /// Loads credentials from `path`.
    ///
    /// Returns an empty catalog if the file does not exist.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                path: path.to_path_buf(),
                entries: HashMap::new(),
            });
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let entries: HashMap<ProviderAlias, StoredCredential> =
            toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            entries,
        })
    }

    /// Writes a single credential entry to the credentials file, preserving all other entries.
    pub fn save_credential(&mut self, alias: &ProviderAlias, credential: StoredCredential) -> Result<()> {
        let raw = if self.path.exists() {
            std::fs::read_to_string(&self.path).context("failed to read credentials.toml")?
        } else {
            String::new()
        };

        let mut doc = raw
            .parse::<toml_edit::DocumentMut>()
            .context("failed to parse credentials.toml")?;

        let value = toml_edit::ser::to_document(&credential)
            .context("failed to serialize credential")?;
        doc[alias.as_str()] = toml_edit::Item::Table(value.as_table().clone());

        write_secret_file(&self.path, &doc.to_string())?;
        self.entries.insert(alias.clone(), credential);
        Ok(())
    }

    /// Removes the credential for the given provider alias from the credentials file.
    ///
    /// Returns an error if no credential exists for the alias.
    pub fn remove_credential(&mut self, alias: &ProviderAlias) -> Result<()> {
        if !self.entries.contains_key(alias) {
            anyhow::bail!("no credential configured for provider '{}'", alias.as_str());
        }

        let raw = std::fs::read_to_string(&self.path)
            .context("failed to read credentials.toml")?;

        let mut doc = raw
            .parse::<toml_edit::DocumentMut>()
            .context("failed to parse credentials.toml")?;

        doc.remove(alias.as_str());

        write_secret_file(&self.path, &doc.to_string())?;
        self.entries.remove(alias);
        Ok(())
    }

    /// Resolves and returns the credential for the given provider alias.
    ///
    /// Returns `None` if no credential is configured for the alias, or an error
    /// if resolution fails (e.g. a referenced environment variable is unset).
    pub fn get(&self, alias: &ProviderAlias) -> anyhow::Result<Option<Credential>> {
        self.entries.get(alias).map(StoredCredential::resolve).transpose()
    }
}

/// Writes `contents` to `path` with `0600` permissions, creating parent directories as needed.
///
/// Uses an atomic temp-file rename to avoid partial writes.
fn write_secret_file(path: &std::path::Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, contents)
        .with_context(|| format!("failed to write {}", tmp.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set permissions on {}", tmp.display()))?;
    }

    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to rename {} to {}", tmp.display(), path.display()))?;

    Ok(())
}

/// A credential as stored on disk; may reference an environment variable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StoredCredential {
    /// A literal API key value stored directly in the credentials file.
    ApiKey { value: String },
    /// An API key read from the named environment variable at the point of use.
    ApiKeyEnv { var: String },
    /// An OAuth token set managed by the application.
    OauthToken {
        access_token: String,
        refresh_token: String,
        expires_at: String,
    },
}

impl StoredCredential {
    /// Resolves the credential into a [`Credential`], expanding `ApiKeyEnv` by
    /// reading the environment variable. Returns an error if the variable is unset.
    pub fn resolve(&self) -> anyhow::Result<Credential> {
        match self {
            StoredCredential::ApiKey { value } => Ok(Credential::ApiKey(ApiKeyCredential {
                value: value.clone(),
            })),
            StoredCredential::ApiKeyEnv { var } => {
                let value = std::env::var(var)
                    .map_err(|_| anyhow::anyhow!("environment variable '{}' is not set", var))?;
                Ok(Credential::ApiKey(ApiKeyCredential { value }))
            }
            StoredCredential::OauthToken {
                access_token,
                refresh_token,
                expires_at,
            } => Ok(Credential::OauthToken(OauthCredential {
                access_token: access_token.clone(),
                refresh_token: refresh_token.clone(),
                expires_at: expires_at.clone(),
            })),
        }
    }
}

/// A fully resolved credential with no references to external state.
#[derive(Debug, Clone)]
pub enum Credential {
    /// A literal API key.
    ApiKey(ApiKeyCredential),
    /// An OAuth token set managed by the application.
    OauthToken(OauthCredential),
}

/// A resolved API key credential.
#[derive(Debug, Clone)]
pub struct ApiKeyCredential {
    pub value: String,
}

/// A resolved OAuth token credential.
#[derive(Debug, Clone)]
pub struct OauthCredential {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: String,
}
