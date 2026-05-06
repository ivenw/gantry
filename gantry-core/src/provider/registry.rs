use std::collections::HashMap;

use anyhow::Result;

use crate::config::{
    ApiKeyCredential, Credential, CredentialsCatalog, ProviderConfig, ProviderConfigCatalog,
};
use crate::provider::agent::ConfiguredAgent;
use crate::provider::client::ProviderClient;
use crate::provider::{ModelSelection, ProviderAlias};

/// Registry of [`ProviderClient`]s for all configured providers.
///
/// Clients are constructed lazily on first access and cached for reuse, preserving
/// any internal state such as auth token caches.
pub struct ProviderClientRegistry {
    catalog: ProviderConfigCatalog,
    credentials: CredentialsCatalog,
    cache: HashMap<ProviderAlias, ProviderClient>,
}

impl ProviderClientRegistry {
    /// Creates a new registry, validating the provider catalog before returning.
    pub fn new(catalog: ProviderConfigCatalog, credentials: CredentialsCatalog) -> Result<Self> {
        catalog.validate()?;
        Ok(Self {
            catalog,
            credentials,
            cache: HashMap::new(),
        })
    }

    /// Returns all configured providers.
    pub fn providers(&self) -> &[ProviderConfig] {
        &self.catalog.providers
    }

    /// Returns the [`ProviderClient`] for the given alias, constructing and caching it on first
    /// access.
    pub fn client(&mut self, alias: &ProviderAlias) -> Result<&ProviderClient> {
        if !self.cache.contains_key(alias) {
            let client = self.build(alias)?;
            self.cache.insert(alias.clone(), client);
        }
        Ok(self.cache.get(alias).expect("just inserted"))
    }

    /// Builds a [`ConfiguredAgent`] for the given model selection and optional preamble.
    pub fn agent(
        &mut self,
        selection: &ModelSelection,
        preamble: Option<&str>,
    ) -> Result<ConfiguredAgent> {
        self.client(&selection.provider)?
            .agent(&selection.model, preamble)
    }

    /// Constructs a fresh [`ProviderClient`] for the given alias.
    fn build(&self, alias: &ProviderAlias) -> Result<ProviderClient> {
        let config = self
            .catalog
            .provider(alias)
            .ok_or_else(|| anyhow::anyhow!("provider '{}' not found", alias.as_str()))?;

        match config {
            ProviderConfig::Ollama(config) => ProviderClient::ollama(config),
            ProviderConfig::Copilot(config) => {
                let credential = self.required_credential(&config.alias)?;
                ProviderClient::copilot(&credential)
            }
            ProviderConfig::OpenAiCompletions(config) => {
                let credential = self.required_api_key_credential(&config.alias)?;
                ProviderClient::openai_completions(config, &credential)
            }
            ProviderConfig::OpenAiResponses(config) => {
                let credential = self.required_api_key_credential(&config.alias)?;
                ProviderClient::openai_responses(config, &credential)
            }
        }
    }

    /// Resolves the credential for the given alias, returning an error if absent.
    fn required_credential(&self, alias: &ProviderAlias) -> Result<Credential> {
        self.credentials.get(alias)?.ok_or_else(|| {
            anyhow::anyhow!("no credential configured for provider '{}'", alias.as_str())
        })
    }

    /// Resolves the credential for the given alias, returning an error if absent or not an API key.
    fn required_api_key_credential(&self, alias: &ProviderAlias) -> Result<ApiKeyCredential> {
        match self.required_credential(alias)? {
            Credential::ApiKey(c) => Ok(c),
            Credential::OauthToken(_) => Err(anyhow::anyhow!(
                "provider '{}' requires an api_key credential, not an oauth token",
                alias.as_str()
            )),
        }
    }
}
