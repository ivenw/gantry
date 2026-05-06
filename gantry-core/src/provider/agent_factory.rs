use anyhow::Result;
use rig::client::{CompletionClient, Nothing};
use rig::providers::ollama;

use crate::provider::agent::ConfiguredAgent;
use crate::provider::catalog::{ModelSelection, ProviderConfig, ProviderConfigCatalog, ProviderId};

/// Builds [`ConfiguredAgent`]s from a validated [`ProviderConfigCatalog`].
#[derive(Clone)]
pub struct AgentFactory {
    catalog: ProviderConfigCatalog,
}

impl AgentFactory {
    /// Creates a new factory, validating the catalog before returning.
    pub fn new(catalog: ProviderConfigCatalog) -> Result<Self> {
        catalog.validate()?;
        Ok(Self { catalog })
    }

    /// Returns all configured providers.
    pub fn providers(&self) -> &[ProviderConfig] {
        &self.catalog.providers
    }

    /// Returns the default [`ModelSelection`] from the catalog.
    pub fn default_selection(&self) -> Result<ModelSelection> {
        self.catalog.default_selection()
    }

    /// Returns the default [`ModelSelection`] for the given provider.
    pub fn default_selection_for(&self, provider_id: &ProviderId) -> Result<ModelSelection> {
        let model_id = self.catalog.provider_default_model(provider_id)?.clone();
        Ok(ModelSelection {
            provider_id: provider_id.clone(),
            model_id,
        })
    }

    /// Validates that the given [`ModelSelection`] refers to a configured provider and model.
    pub fn validate_selection(&self, selection: &ModelSelection) -> Result<()> {
        self.catalog.validate_selection(selection)
    }

    /// Builds a [`ConfiguredAgent`] for the given model selection and optional preamble.
    pub fn agent(
        &self,
        selection: &ModelSelection,
        preamble: Option<&str>,
    ) -> Result<ConfiguredAgent> {
        match self.provider_config(selection)? {
            ProviderConfig::Ollama(provider) => {
                let model = self
                    .catalog
                    .model(&selection.provider_id, &selection.model_id)
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "configured model '{}' not found for provider '{}'",
                            selection.model_id.as_str(),
                            selection.provider_id.as_str()
                        )
                    })?;

                let client = ollama::Client::builder()
                    .api_key(Nothing)
                    .base_url(&provider.base_url)
                    .build()?;

                let mut builder = client.agent(&model.provider_model_name);
                if let Some(p) = preamble {
                    builder = builder.preamble(p);
                }
                Ok(ConfiguredAgent::ollama(builder.build()))
            }
        }
    }

    /// Looks up the [`ProviderConfig`] for the given selection, returning an error if not found.
    fn provider_config(&self, selection: &ModelSelection) -> Result<ProviderConfig> {
        self.catalog
            .provider(&selection.provider_id)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "configured provider '{}' not found",
                    selection.provider_id.as_str()
                )
            })
    }
}
