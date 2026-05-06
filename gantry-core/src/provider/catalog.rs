use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The full set of configured providers and the system-wide default provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigCatalog {
    pub providers: Vec<ProviderConfig>,
    pub default_provider: ProviderId,
}

impl ProviderConfigCatalog {
    /// Checks that provider IDs are unique, model IDs are unique within each provider,
    /// each provider's default model exists, and the catalog's default provider exists.
    pub fn validate(&self) -> anyhow::Result<()> {
        let mut provider_ids = HashSet::new();
        for provider in &self.providers {
            if !provider_ids.insert(provider.id().clone()) {
                return Err(anyhow::anyhow!(
                    "duplicate provider id '{}'",
                    provider.id().as_str()
                ));
            }

            let mut model_ids = HashSet::new();
            for model in provider.models() {
                if !model_ids.insert(model.id.clone()) {
                    return Err(anyhow::anyhow!(
                        "duplicate model id '{}' for provider '{}'",
                        model.id.as_str(),
                        provider.id().as_str()
                    ));
                }
            }

            if !provider
                .models()
                .iter()
                .any(|model| model.id == *provider.default_model())
            {
                return Err(anyhow::anyhow!(
                    "default model '{}' not found for provider '{}'",
                    provider.default_model().as_str(),
                    provider.id().as_str()
                ));
            }
        }

        if self.provider(&self.default_provider).is_none() {
            return Err(anyhow::anyhow!(
                "default provider '{}' not found",
                self.default_provider.as_str()
            ));
        }

        Ok(())
    }

    /// Returns the [`ProviderConfig`] for the given provider ID, if it exists.
    pub fn provider(&self, provider_id: &ProviderId) -> Option<&ProviderConfig> {
        self.providers
            .iter()
            .find(|provider| provider.id() == provider_id)
    }

    /// Returns the [`ConfiguredModel`] for the given provider and model IDs, if both exist.
    pub fn model(&self, provider_id: &ProviderId, model_id: &ModelId) -> Option<&ConfiguredModel> {
        self.provider(provider_id)?
            .models()
            .iter()
            .find(|model| model.id == *model_id)
    }

    /// Returns the default model ID for the given provider, or an error if the provider is not found.
    pub fn provider_default_model(&self, provider_id: &ProviderId) -> anyhow::Result<&ModelId> {
        self.provider(provider_id)
            .map(ProviderConfig::default_model)
            .ok_or_else(|| anyhow::anyhow!("provider '{}' not found", provider_id.as_str()))
    }

    /// Returns a [`ModelSelection`] pointing to the catalog's default provider and its default model.
    pub fn default_selection(&self) -> anyhow::Result<ModelSelection> {
        Ok(ModelSelection {
            provider_id: self.default_provider.clone(),
            model_id: self.provider_default_model(&self.default_provider)?.clone(),
        })
    }

    /// Checks that the given [`ModelSelection`] refers to a configured provider and model.
    pub fn validate_selection(&self, selection: &ModelSelection) -> anyhow::Result<()> {
        self.provider(&selection.provider_id).ok_or_else(|| {
            anyhow::anyhow!("provider '{}' not found", selection.provider_id.as_str())
        })?;
        self.model(&selection.provider_id, &selection.model_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "model '{}' not found for provider '{}'",
                    selection.model_id.as_str(),
                    selection.provider_id.as_str()
                )
            })?;
        Ok(())
    }
}

/// A resolved provider and model pair used to select a specific model for inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelection {
    pub provider_id: ProviderId,
    pub model_id: ModelId,
}

/// Discriminated union of all supported provider configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderConfig {
    Ollama(OllamaProviderConfig),
}

impl ProviderConfig {
    /// Returns the provider's unique identifier.
    pub fn id(&self) -> &ProviderId {
        match self {
            ProviderConfig::Ollama(config) => &config.id,
        }
    }

    /// Returns the list of models configured for this provider.
    pub fn models(&self) -> &[ConfiguredModel] {
        match self {
            ProviderConfig::Ollama(config) => &config.models,
        }
    }

    /// Returns the default model identifier for this provider.
    pub fn default_model(&self) -> &ModelId {
        match self {
            ProviderConfig::Ollama(config) => &config.default_model,
        }
    }
}

/// Configuration for an Ollama provider instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaProviderConfig {
    pub id: ProviderId,
    pub base_url: String,
    pub models: Vec<ConfiguredModel>,
    pub default_model: ModelId,
}

/// A model exposed to callers, mapped to its provider-specific model name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredModel {
    pub id: ModelId,
    pub provider_model_name: String,
}

/// Unique identifier for a provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(pub String);

impl ProviderId {
    /// Creates a new [`ProviderId`] from any string-like value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Unique identifier for a model within a provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelId(pub String);

impl ModelId {
    /// Creates a new [`ModelId`] from any string-like value.
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
                id: ProviderId::new("ollama"),
                base_url: "http://localhost:11434".to_string(),
                models: vec![ConfiguredModel {
                    id: ModelId::new("default"),
                    provider_model_name: "ministral-3:3b".to_string(),
                }],
                default_model: ModelId::new("default"),
            })],
            default_provider: ProviderId::new("ollama"),
        }
    }

    #[test]
    fn catalog_validation_accepts_valid_catalog() {
        sample_catalog().validate().unwrap();
    }

    #[test]
    fn catalog_validation_rejects_missing_default_provider() {
        let mut catalog = sample_catalog();
        catalog.default_provider = ProviderId::new("missing");
        assert!(catalog.validate().is_err());
    }

    #[test]
    fn catalog_validation_rejects_missing_default_model() {
        let mut catalog = sample_catalog();
        let ProviderConfig::Ollama(provider) = &mut catalog.providers[0];
        provider.default_model = ModelId::new("missing");
        assert!(catalog.validate().is_err());
    }
}
