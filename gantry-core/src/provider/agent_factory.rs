use anyhow::Result;
use rig::client::{CompletionClient, Nothing, ProviderClient};
use rig::providers::copilot::{self, CopilotAuth};
use rig::providers::{ollama, openai};

use crate::config::credentials::{Credential, CredentialsCatalog};
use crate::provider::agent::ConfiguredAgent;
use crate::provider::catalog::{
    ModelSelection, OllamaProviderConfig, OpenAiCompletionsProviderConfig,
    OpenAiResponsesProviderConfig, ProviderConfig, ProviderConfigCatalog,
};

/// Builds [`ConfiguredAgent`]s from a validated [`ProviderConfigCatalog`].
#[derive(Clone)]
pub struct AgentFactory {
    catalog: ProviderConfigCatalog,
    credentials: CredentialsCatalog,
}

impl AgentFactory {
    /// Creates a new factory, validating the catalog before returning.
    pub fn new(catalog: ProviderConfigCatalog, credentials: CredentialsCatalog) -> Result<Self> {
        catalog.validate()?;
        Ok(Self {
            catalog,
            credentials,
        })
    }

    /// Returns all configured providers.
    pub fn providers(&self) -> &[ProviderConfig] {
        &self.catalog.providers
    }

    /// Builds a [`ConfiguredAgent`] for the given model selection and optional preamble.
    ///
    /// The model alias is passed directly to the provider as the model name.
    pub fn agent(
        &self,
        selection: &ModelSelection,
        preamble: Option<&str>,
    ) -> Result<ConfiguredAgent> {
        match self.provider_config(selection)? {
            ProviderConfig::Ollama(provider) => ollama_agent(&provider, selection, preamble),
            ProviderConfig::Copilot(provider) => {
                let credential = self.credentials.get(&provider.alias)?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "no credential configured for provider '{}'",
                        provider.alias.as_str()
                    )
                })?;
                copilot_agent(&credential, selection, preamble)
            }
            ProviderConfig::OpenAiCompletions(provider) => {
                let credential = self.credentials.get(&provider.alias)?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "no credential configured for provider '{}'",
                        provider.alias.as_str()
                    )
                })?;
                openai_completions_agent(&provider, &credential, selection, preamble)
            }
            ProviderConfig::OpenAiResponses(provider) => {
                let credential = self.credentials.get(&provider.alias)?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "no credential configured for provider '{}'",
                        provider.alias.as_str()
                    )
                })?;
                openai_responses_agent(&provider, &credential, selection, preamble)
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

/// Builds an Ollama-backed [`ConfiguredAgent`].
fn ollama_agent(
    provider: &OllamaProviderConfig,
    selection: &ModelSelection,
    preamble: Option<&str>,
) -> Result<ConfiguredAgent> {
    let mut builder = ollama::Client::builder().api_key(Nothing);
    if let Some(base_url) = &provider.base_url {
        builder = builder.base_url(base_url);
    }
    let client = builder.build()?;

    let mut agent = client.agent(selection.model.as_str());
    if let Some(p) = preamble {
        agent = agent.preamble(p);
    }
    Ok(ConfiguredAgent::ollama(agent.build()))
}

/// Builds a GitHub Copilot-backed [`ConfiguredAgent`].
fn copilot_agent(
    credential: &Credential,
    selection: &ModelSelection,
    preamble: Option<&str>,
) -> Result<ConfiguredAgent> {
    let auth = match credential {
        Credential::ApiKey { value } => CopilotAuth::ApiKey(value.clone()),
        Credential::OauthToken { access_token, .. } => {
            CopilotAuth::GitHubAccessToken(access_token.clone())
        }
    };
    let client = copilot::Client::from_val(auth)?;

    let mut agent = client.agent(selection.model.as_str());
    if let Some(p) = preamble {
        agent = agent.preamble(p);
    }
    Ok(ConfiguredAgent::copilot(agent.build()))
}

/// Builds an OpenAI-compatible completions API [`ConfiguredAgent`].
fn openai_completions_agent(
    provider: &OpenAiCompletionsProviderConfig,
    credential: &Credential,
    selection: &ModelSelection,
    preamble: Option<&str>,
) -> Result<ConfiguredAgent> {
    let api_key = extract_api_key(credential, &provider.alias.0)?;
    let client = openai::CompletionsClient::builder()
        .api_key(&api_key)
        .base_url(&provider.base_url)
        .build()?;

    let mut agent = client.agent(selection.model.as_str());
    if let Some(p) = preamble {
        agent = agent.preamble(p);
    }
    Ok(ConfiguredAgent::openai_completions(agent.build()))
}

/// Builds an OpenAI-compatible responses API [`ConfiguredAgent`].
fn openai_responses_agent(
    provider: &OpenAiResponsesProviderConfig,
    credential: &Credential,
    selection: &ModelSelection,
    preamble: Option<&str>,
) -> Result<ConfiguredAgent> {
    let api_key = extract_api_key(credential, &provider.alias.0)?;
    let client = openai::Client::builder()
        .api_key(&api_key)
        .base_url(&provider.base_url)
        .build()?;

    let mut agent = client.agent(selection.model.as_str());
    if let Some(p) = preamble {
        agent = agent.preamble(p);
    }
    Ok(ConfiguredAgent::openai_responses(agent.build()))
}

/// Extracts an API key from a credential, returning an error if the credential type is not an API key.
fn extract_api_key(credential: &Credential, provider_alias: &str) -> Result<String> {
    match credential {
        Credential::ApiKey { value } => Ok(value.clone()),
        Credential::OauthToken { .. } => Err(anyhow::anyhow!(
            "provider '{}' requires an api_key credential, not an oauth token",
            provider_alias
        )),
    }
}
