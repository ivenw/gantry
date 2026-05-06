use anyhow::Result;
use rig::client::{CompletionClient, Nothing};
use rig::providers::ollama;

use crate::provider::agent::ConfiguredAgent;
use crate::provider::catalog::{ModelSelection, ProviderAlias, ProviderConfig, ProviderConfigCatalog};

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
    pub fn default_selection_for(&self, provider_alias: &ProviderAlias) -> Result<ModelSelection> {
        let model_alias = self.catalog.provider_default_model(provider_alias)?.clone();
        Ok(ModelSelection {
            provider_alias: provider_alias.clone(),
            model_alias,
        })
    }

    /// Builds a [`ConfiguredAgent`] for the given model selection and optional preamble.
    ///
    /// The `model_alias` is used directly as the Ollama model name.
    pub fn agent(
        &self,
        selection: &ModelSelection,
        preamble: Option<&str>,
    ) -> Result<ConfiguredAgent> {
        match self.provider_config(selection)? {
            ProviderConfig::Ollama(provider) => {
                let client = ollama::Client::builder()
                    .api_key(Nothing)
                    .base_url(&provider.base_url)
                    .build()?;

                let mut builder = client.agent(selection.model_alias.as_str());
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
            .provider(&selection.provider_alias)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "configured provider '{}' not found",
                    selection.provider_alias.as_str()
                )
            })
    }
}
