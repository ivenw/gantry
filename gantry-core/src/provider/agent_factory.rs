use anyhow::Result;
use rig::client::{CompletionClient, Nothing, ProviderClient};
use rig::providers::copilot::{self, CopilotAuth};
use rig::providers::ollama;

use crate::config::credentials::{Credential, CredentialsCatalog};
use crate::provider::agent::ConfiguredAgent;
use crate::provider::catalog::{
    ,
    ModelSelection, OllamaProviderConfig, ProviderConfig, ProviderConfigCatalog,
};

/// Builds [`ConfiguredAgent`]s from a validated [`ProviderConfigCatalog`].
#[derive(Clone)]
pub struct AgentFactory {
    catalog: ProviderConfigCatalog,
    credentials: CredentialsCatalog,
}

impl AgentFactory {
    /// Creates a new factory, validating the catalog before returning.
    pub fn new(ca
            catalog,
           oviderConfig,
       Catalog, credentials: CredentialsCatalog) -> Result<Self> {
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
        preamble: Option<&str>,.credentialsrovider_config(selectiConfig::Ollama(provider) => ollama_agent(&provider, selection, preamble),
            Providerig::Copilot(provider) => {
                let credal = self.credentials.get(&provider.alias)?.ok_or_else(|| {
                    anyhanyhow!(
                    )
                    provider.alias.as_str()
                    )
                })?;
                copilot_agent(&credential, selection, preamble)
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
