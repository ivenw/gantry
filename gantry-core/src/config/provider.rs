use std::collections::HashSet;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::dirs::GlobalConfigDir;
use crate::provider::ProviderAlias;

/// The full set of configured providers, deserialized from `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfigCatalog {
    pub providers: Vec<ProviderConfig>,
}

impl ProviderConfigCatalog {
    /// Loads provider configuration from `~/.gantry/config.toml`.
    ///
    /// Returns an empty catalog if the file does not exist.
    pub fn load() -> Result<Self> {
        let path = GlobalConfigDir::new()?.config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
    }

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
}
