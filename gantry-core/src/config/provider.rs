use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::provider::ProviderAlias;

/// Manages persistence of [`ProviderConfigCatalog`] to and from `config.toml`.
pub struct ProviderConfigRepository {
    path: PathBuf,
    pub catalog: ProviderConfigCatalog,
}

impl ProviderConfigRepository {
    /// Loads provider configuration from `path`.
    ///
    /// Returns an empty catalog if the file does not exist.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                path: path.to_path_buf(),
                catalog: ProviderConfigCatalog::default(),
            });
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let catalog: ProviderConfigCatalog =
            toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            catalog,
        })
    }

    /// Appends a new provider entry to `config.toml`, preserving all existing content.
    ///
    /// Returns an error if a provider with the same alias already exists.
    pub fn add_provider(&mut self, config: ProviderConfig) -> Result<()> {
        if self.catalog.provider(config.alias()).is_some() {
            anyhow::bail!("provider '{}' already exists", config.alias().as_str());
        }

        let raw = if self.path.exists() {
            std::fs::read_to_string(&self.path)
                .with_context(|| format!("failed to read {}", self.path.display()))?
        } else {
            String::new()
        };

        let mut doc = raw
            .parse::<toml_edit::DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.path.display()))?;

        let entry =
            toml_edit::ser::to_document(&config).context("failed to serialize provider config")?;

        // `[[providers]]` is an array-of-tables in TOML.
        if !doc.contains_key("providers") {
            doc["providers"] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
        }
        let array = doc["providers"]
            .as_array_of_tables_mut()
            .context("'providers' is not an array of tables")?;
        array.push(entry.as_table().clone());

        atomic_write(&self.path, &doc.to_string())?;
        self.catalog.providers.push(config);
        Ok(())
    }

    /// Removes the provider with the given alias from `config.toml`.
    ///
    /// Returns an error if no provider with that alias exists.
    pub fn remove_provider(&mut self, alias: &ProviderAlias) -> Result<()> {
        let pos = self
            .catalog
            .providers
            .iter()
            .position(|p| p.alias() == alias)
            .ok_or_else(|| anyhow::anyhow!("provider '{}' not found", alias.as_str()))?;

        let raw = std::fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;

        let mut doc = raw
            .parse::<toml_edit::DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.path.display()))?;

        let array = doc["providers"]
            .as_array_of_tables_mut()
            .context("'providers' is not an array of tables")?;

        // Find the matching entry by alias field value.
        let toml_pos = array
            .iter()
            .position(|t| {
                t.get("alias")
                    .and_then(|v| v.as_str())
                    .map(|s| s == alias.as_str())
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                anyhow::anyhow!("provider '{}' not found in config file", alias.as_str())
            })?;

        array.remove(toml_pos);
        atomic_write(&self.path, &doc.to_string())?;
        self.catalog.providers.remove(pos);
        Ok(())
    }
}

/// The full set of configured providers, deserialized from `config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderConfigCatalog {
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

impl ProviderConfigCatalog {
    /// Checks that provider aliases are unique.
    pub fn validate(&self) -> anyhow::Result<()> {
        let mut provider_aliases = HashSet::new();
        for provider in &self.providers {
            if !provider_aliases.insert(provider.alias().clone()) {
                return Err(anyhow::anyhow!(
                    "duplicate provider alias '{}'",
                    provider.alias().as_str()
                ));
            }
        }
        Ok(())
    }

    /// Returns the [`ProviderConfig`] for the given provider alias, if it exists.
    pub fn provider(&self, provider_alias: &ProviderAlias) -> Option<&ProviderConfig> {
        self.providers
            .iter()
            .find(|provider| provider.alias() == provider_alias)
    }
}

/// Discriminated union of all supported provider configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderConfig {
    Ollama(OllamaProviderConfig),
    Copilot(CopilotProviderConfig),
    OpenAiCompletions(OpenAiCompletionsProviderConfig),
    OpenAiResponses(OpenAiResponsesProviderConfig),
}

impl ProviderConfig {
    /// Returns the provider's user-defined alias.
    pub fn alias(&self) -> &ProviderAlias {
        match self {
            ProviderConfig::Ollama(config) => &config.alias,
            ProviderConfig::Copilot(config) => &config.alias,
            ProviderConfig::OpenAiCompletions(config) => &config.alias,
            ProviderConfig::OpenAiResponses(config) => &config.alias,
        }
    }
}

/// Configuration for an Ollama provider instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaProviderConfig {
    pub alias: ProviderAlias,
    /// Overrides the default Ollama base URL (`http://localhost:11434`).
    pub base_url: Option<String>,
    /// Context window size in tokens. Ollama uses a server/model-load-time setting rather than
    /// per-model metadata, so this must be configured explicitly if context tracking is desired.
    pub context_window: Option<u32>,
}

/// Configuration for a GitHub Copilot provider instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotProviderConfig {
    pub alias: ProviderAlias,
}

/// Configuration for an OpenAI-compatible provider using the chat completions API.
///
/// Suitable for LLM routers and self-hosted endpoints that implement the OpenAI completions API.
/// Requires an `api_key` credential stored under this provider's alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCompletionsProviderConfig {
    pub alias: ProviderAlias,
    /// Base URL of the OpenAI-compatible completions endpoint.
    pub base_url: String,
}

/// Configuration for a provider using the OpenAI responses API.
///
/// Suitable for endpoints that implement the newer OpenAI responses API.
/// Requires an `api_key` credential stored under this provider's alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiResponsesProviderConfig {
    pub alias: ProviderAlias,
    /// Base URL of the OpenAI-compatible responses endpoint.
    pub base_url: String,
}

/// Writes `contents` to `path` atomically via a sibling temp file and rename.
///
/// Creates parent directories if they do not exist.
fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, contents).with_context(|| format!("failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_catalog() -> ProviderConfigCatalog {
        ProviderConfigCatalog {
            providers: vec![ProviderConfig::Ollama(OllamaProviderConfig {
                alias: ProviderAlias::new("ollama"),
                base_url: None,
            })],
        }
    }

    #[test]
    fn catalog_validation_accepts_valid_catalog() {
        sample_catalog().validate().unwrap();
    }

    #[test]
    fn catalog_validation_rejects_duplicate_alias() {
        let mut catalog = sample_catalog();
        catalog
            .providers
            .push(ProviderConfig::Ollama(OllamaProviderConfig {
                alias: ProviderAlias::new("ollama"),
                base_url: None,
            }));
        assert!(catalog.validate().is_err());
    }

    #[test]
    fn add_and_remove_provider_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut repo = ProviderConfigRepository::load(&path).unwrap();
        repo.add_provider(ProviderConfig::Ollama(OllamaProviderConfig {
            alias: ProviderAlias::new("local"),
            base_url: None,
        }))
        .unwrap();

        assert_eq!(repo.catalog.providers.len(), 1);
        assert!(path.exists());

        let reloaded = ProviderConfigRepository::load(&path).unwrap();
        assert_eq!(reloaded.catalog.providers.len(), 1);

        repo.remove_provider(&ProviderAlias::new("local")).unwrap();

        assert_eq!(repo.catalog.providers.len(), 0);

        let reloaded = ProviderConfigRepository::load(&path).unwrap();
        assert_eq!(reloaded.catalog.providers.len(), 0);
    }

    #[test]
    fn add_provider_rejects_duplicate_alias() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut repo = ProviderConfigRepository::load(&path).unwrap();
        repo.add_provider(ProviderConfig::Ollama(OllamaProviderConfig {
            alias: ProviderAlias::new("local"),
            base_url: None,
        }))
        .unwrap();
        let err = repo
            .add_provider(ProviderConfig::Ollama(OllamaProviderConfig {
                alias: ProviderAlias::new("local"),
                base_url: None,
            }))
            .unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }
}
