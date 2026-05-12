use std::collections::HashMap;

use anyhow::Result;

use rig::tool::ToolDyn;

use crate::config::{
    ApiKeyCredential, Credential, CredentialsRepository, ProviderConfig, ProviderConfigRepository,
};
use crate::provider::agent::BoxedAgent;
use crate::provider::client::ProviderClient;
use crate::provider::{ModelSelection, ProviderAlias};

/// Registry of [`ProviderClient`]s for all configured providers.
///
/// Clients are constructed lazily on first access and cached for reuse, preserving
/// any internal state such as auth token caches.
pub struct ProviderClientRegistry {
    pub(crate) providers: ProviderConfigRepository,
    pub(crate) credentials: CredentialsRepository,
    cache: HashMap<ProviderAlias, ProviderClient>,
}

impl ProviderClientRegistry {
    /// Creates a new registry, validating the provider catalog before returning.
    pub fn new(
        providers: ProviderConfigRepository,
        credentials: CredentialsRepository,
    ) -> Result<Self> {
        providers.catalog.validate()?;
        Ok(Self {
            providers,
            credentials,
            cache: HashMap::new(),
        })
    }

    /// Returns all configured providers.
    pub fn providers(&self) -> &[ProviderConfig] {
        &self.providers.catalog.providers
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

    /// Builds a [`BoxedAgent`] for the given model selection, optional preamble, and tools.
    pub fn agent(
        &mut self,
        selection: &ModelSelection,
        preamble: Option<&str>,
        tools: Vec<Box<dyn ToolDyn>>,
    ) -> Result<BoxedAgent> {
        self.client(&selection.provider_alias)?
            .agent(&selection.model_id, preamble, tools)
    }

    /// Constructs a fresh [`ProviderClient`] for the given alias.
    fn build(&self, alias: &ProviderAlias) -> Result<ProviderClient> {
        let config = self
            .providers
            .catalog
            .provider(alias)
            .ok_or_else(|| anyhow::anyhow!("provider '{}' not found", alias.as_str()))?;

        match config {
            ProviderConfig::Ollama(config) => ProviderClient::ollama(config),
            ProviderConfig::Copilot(config) => {
                let credential = self.required_credential(&config.alias)?;
                ProviderClient::github_copilot(&credential)
            }
            ProviderConfig::OpenAiCompletions(config) => {
                let credential = self.required_api_key_credential(&config.alias)?;
                ProviderClient::openai_completions(config, &credential)
            }
            ProviderConfig::OpenAiResponses(config) => {
                let credential = self.required_api_key_credential(&config.alias)?;
                ProviderClient::openai_responses(config, &credential)
            }
            ProviderConfig::Cortecs(config) => {
                let credential = self.required_api_key_credential(&config.alias)?;
                ProviderClient::cortecs(config, &credential)
            }
        }
    }

    /// Resolves the credential for the given alias, returning an error if absent.
    fn required_credential(&self, alias: &ProviderAlias) -> Result<Credential> {
        self.credentials.catalog.get(alias)?.ok_or_else(|| {
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
