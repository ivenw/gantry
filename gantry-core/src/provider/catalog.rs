use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The full set of configured providers and the system-wide default provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigCatalog {
    pub providers: Vec<ProviderConfig>,
    pub default_provider: ProviderAlias,
}

impl ProviderConfigCatalog {
    /// Checks that provider IDs are unique, model IDs are unique within each provider,
    /// each provider's default model exists, and the catalog's default provider exists.
    pub fn validate(&self) -> anyhow::Result<()> {
        let mut provider_aliases = HashSet::new();
        for provider in &self.providers {
            if !provider_aliases.insert(provider.alias().clone()) {
                return Err(anyhow::anyhow!(
                    "duplicate provider alias '{}'",
                    provider.alias().as_str()
                ));
            }

            let mut model_aliases = HashSet::new();
            for model in provider.models() {
                if !model_aliases.insert(model.alias.clone()) {
                    return Err(anyhow::anyhow!(
                        "duplicate model alias '{}' for provider '{}'",
                        model.alias.as_str(),
                        provider.alias().as_str()
                    ));
                }
            }

            if !provider
                .models()
                .iter()
                .any(|model| model.alias == *provider.default_model())
            {
                return Err(anyhow::anyhow!(
                    "default model '{}' not found for provider '{}'",
                    provider.default_model().as_str(),
                    provider.alias().as_str()
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

    /// Returns the [`ProviderConfig`] for the given provider alias, if it exists.
    pub fn provider(&self, provider_alias: &ProviderAlias) -> Option<&ProviderConfig> {
        self.providers
            .iter()
            .find(|provider| provider.alias() == provider_alias)
    }

    /// Returns the [`ConfiguredModel`] for the given provider and model aliases, if both exist.
    pub fn model(
        &self,
        provider_alias: &ProviderAlias,
        model_alias: &ModelAlias,
    ) -> Option<&ConfiguredModel> {
        self.provider(provider_alias)?
            .models()
            .iter()
            .find(|model| model.alias == *model_alias)
    }

    /// Returns the default model alias for the given provider, or an error if the provider is not found.
    pub fn provider_default_model(
        &self,
        provider_alias: &ProviderAlias,
    ) -> anyhow::Result<&ModelAlias> {
        self.provider(provider_alias)
            .map(ProviderConfig::default_model)
            .ok_or_else(|| anyhow::anyhow!("provider '{}' not found", provider_alias.as_str()))
    }

    /// Returns a [`ModelSelection`] pointing to the catalog's default provider and its default model.
    pub fn default_selection(&self) -> anyhow::Result<ModelSelection> {
        Ok(ModelSelection {
            provider_alias: self.default_provider.clone(),
            model_alias: self.provider_default_model(&self.default_provider)?.clone(),
        })
    }

    /// Checks that the given [`ModelSelection`] refers to a configured provider and model.
    pub fn validate_selection(&self, selection: &ModelSelection) -> anyhow::Result<()> {
        self.provider(&selection.provider_alias).ok_or_else(|| {
            anyhow::anyhow!(
                "provider '{}' not found",
                selection.provider_alias.as_str()
            )
        })?;
        self.model(&selection.provider_alias, &selection.model_alias)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "model '{}' not found for provider '{}'",
                    selection.model_alias.as_str(),
                    selection.provider_alias.as_str()
                )
            })?;
        Ok(())
    }
}

/// A resolved provider and model pair used to select a specific model for inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelection {
    pub provider_alias: ProviderAlias,
    pub model_alias: ModelAlias,
}

/// Discriminated union of all supported provider configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderConfig {
    Ollama(OllamaProviderConfig),
}

impl ProviderConfig {
    /// Returns the provider's user-defined alias.
    pub fn alias(&self) -> &ProviderAlias {
        match self {
            ProviderConfig::Ollama(config) => &config.alias,
        }
    }

    /// Returns the list of models configured for this provider.
    pub fn models(&self) -> &[ConfiguredModel] {
        match self {
            ProviderConfig::Ollama(config) => &config.models,
        }
    }

    /// Returns the default model alias for this provider.
    pub fn default_model(&self) -> &ModelAlias {
        match self {
            ProviderConfig::Ollama(config) => &config.default_model,
        }
    }
}

/// Configuration for an Ollama provider instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaProviderConfig {
    pub alias: ProviderAlias,
    pub base_url: String,
    pub models: Vec<ConfiguredModel>,
    pub default_model: ModelAlias,
}

/// A model exposed to callers, mapped to its provider-specific model name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredModel {
    pub alias: ModelAlias,
    pub provider_model_name: String,
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
                base_url: "http://localhost:11434".to_string(),
                models: vec![ConfiguredModel {
                    alias: ModelAlias::new("default"),
                    provider_model_name: "ministral-3:3b".to_string(),
                }],
                default_model: ModelAlias::new("default"),
            })],
            default_provider: ProviderAlias::new("ollama"),
        }
    }

    #[test]
    fn catalog_validation_accepts_valid_catalog() {
        sample_catalog().validate().unwrap();
    }

    #[test]
    fn catalog_validation_rejects_missing_default_provider() {
        let mut catalog = sample_catalog();
        catalog.default_provider = ProviderAlias::new("missing");
        assert!(catalog.validate().is_err());
    }

    #[test]
    fn catalog_validation_rejects_missing_default_model() {
        let mut catalog = sample_catalog();
        let ProviderConfig::Ollama(provider) = &mut catalog.providers[0];
        provider.default_model = ModelAlias::new("missing");
        assert!(catalog.validate().is_err());
    }
}
