pub mod agent_factory;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(pub String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelId(pub String);

impl ModelId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredModel {
    pub id: ModelId,
    pub provider_model_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaProviderConfig {
    pub id: ProviderId,
    pub base_url: String,
    pub models: Vec<ConfiguredModel>,
    pub default_model: ModelId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderConfig {
    Ollama(OllamaProviderConfig),
}

impl ProviderConfig {
    pub fn id(&self) -> &ProviderId {
        match self {
            ProviderConfig::Ollama(config) => &config.id,
        }
    }

    pub fn models(&self) -> &[ConfiguredModel] {
        match self {
            ProviderConfig::Ollama(config) => &config.models,
        }
    }

    pub fn default_model(&self) -> &ModelId {
        match self {
            ProviderConfig::Ollama(config) => &config.default_model,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigCatalog {
    pub providers: Vec<ProviderConfig>,
    pub default_provider: ProviderId,
}

impl ProviderConfigCatalog {
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

    pub fn provider(&self, provider_id: &ProviderId) -> Option<&ProviderConfig> {
        self.providers
            .iter()
            .find(|provider| provider.id() == provider_id)
    }

    pub fn model(&self, provider_id: &ProviderId, model_id: &ModelId) -> Option<&ConfiguredModel> {
        self.provider(provider_id)?
            .models()
            .iter()
            .find(|model| model.id == *model_id)
    }

    pub fn provider_default_model(&self, provider_id: &ProviderId) -> anyhow::Result<&ModelId> {
        self.provider(provider_id)
            .map(ProviderConfig::default_model)
            .ok_or_else(|| anyhow::anyhow!("provider '{}' not found", provider_id.as_str()))
    }

    pub fn default_selection(&self) -> anyhow::Result<ModelSelection> {
        Ok(ModelSelection {
            provider_id: self.default_provider.clone(),
            model_id: self.provider_default_model(&self.default_provider)?.clone(),
        })
    }

    pub fn selection(&self, selection: &ModelSelection) -> anyhow::Result<()> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelection {
    pub provider_id: ProviderId,
    pub model_id: ModelId,
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
