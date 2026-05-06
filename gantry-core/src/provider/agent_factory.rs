use anyhow::Result;
use rig::client::{CompletionClient, Nothing};
use rig::providers::ollama;

use crate::provider::agent::ConfiguredAgent;
use crate::provider::catalog::{ModelSelection, ProviderConfig, ProviderConfigCatalog};

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
                let mut builder = ollama::Client::builder().api_key(Nothing);
                if let Some(base_url) = &provider.base_url {
                    builder = builder.base_url(base_url);
                }
                let client = builder.build()?;

                let mut builder = client.agent(selection.model.as_str());
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
            .provider(&selection.provider)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "configured provider '{}' not found",
                    selection.provider.as_str()
                )
            })
    }
}
