use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The full set of configured providers.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfigCatalog {
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

/// A resolved provider and model pair used to select a specific model for inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelection {
    pub provider: ProviderAlias,
    pub model: ModelAlias,
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

const OLLAMA_DEFAULT_BASE_URL: &str = "http://localhost:11434";

/// Configuration for an Ollama provider instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaProviderConfig {
    pub alias: ProviderAlias,
    /// Overrides the default Ollama base URL (`http://localhost:11434`).
    pub base_url: Option<String>,
}

impl OllamaProviderConfig {
    /// Fetches the list of available models from the Ollama `/api/tags` endpoint.
    pub async fn fetch_models(&self) -> anyhow::Result<Vec<ModelAlias>> {
        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<OllamaModelEntry>,
        }

        #[derive(Deserialize)]
        struct OllamaModelEntry {
            name: String,
        }

        let base_url = self.base_url.as_deref().unwrap_or(OLLAMA_DEFAULT_BASE_URL);
        let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
        let response: TagsResponse = reqwest::get(&url).await?.json().await?;
        Ok(response
            .models
            .into_iter()
            .map(|m| ModelAlias::new(m.name))
            .collect())
    }
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

/// User-defined alias for a provider instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderAlias(pub String);

impl ProviderAlias {
    /// Creates a new [`ProviderAlias`] from any string-like value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// User-defined alias for a model within a provider.
///
/// For Ollama, this is the model name as returned by `/api/tags` (e.g. `"llama3.2:3b"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelAlias(pub String);

impl ModelAlias {
    /// Creates a new [`ModelAlias`] from any string-like value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
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
