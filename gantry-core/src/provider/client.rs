use anyhow::Result;
use rig::client::{CompletionClient, ModelListingClient as _, Nothing, ProviderClient as _};
use rig::model::ModelList;
use rig::providers::copilot::{self, CopilotAuth};
use rig::providers::{ollama, openai};
use rig::tool::ToolDyn;

use crate::config::{
    ApiKeyCredential, CortecsProviderConfig, Credential, OllamaProviderConfig,
    OpenAiCompletionsProviderConfig, OpenAiResponsesProviderConfig,
};
use crate::provider::ModelId;
use crate::provider::agent::BoxedAgent;
use crate::providers::cortecs;

/// A constructed, ready-to-use provider client that can list models and create agents.
pub enum ProviderClient {
    Ollama(ollama::Client),
    GitHubCopilot {
        client: copilot::Client,
        credential: Credential,
    },
    OpenAiCompletions(openai::CompletionsClient),
    OpenAiResponses(openai::Client),
    Cortecs(cortecs::Client),
}

impl ProviderClient {
    pub(crate) fn ollama(config: &OllamaProviderConfig) -> Result<Self> {
        let mut builder = ollama::Client::builder().api_key(Nothing);
        if let Some(base_url) = &config.base_url {
            builder = builder.base_url(base_url);
        }
        Ok(Self::Ollama(builder.build()?))
    }

    pub(crate) fn openai_completions(
        config: &OpenAiCompletionsProviderConfig,
        credential: &ApiKeyCredential,
    ) -> Result<Self> {
        let client = openai::CompletionsClient::builder()
            .api_key(&credential.value)
            .base_url(&config.base_url)
            .build()?;
        Ok(Self::OpenAiCompletions(client))
    }

    pub(crate) fn openai_responses(
        config: &OpenAiResponsesProviderConfig,
        credential: &ApiKeyCredential,
    ) -> Result<Self> {
        let client = openai::Client::builder()
            .api_key(&credential.value)
            .base_url(&config.base_url)
            .build()?;
        Ok(Self::OpenAiResponses(client))
    }

    pub(crate) fn cortecs(
        config: &CortecsProviderConfig,
        credential: &ApiKeyCredential,
    ) -> Result<Self> {
        let _ = config;
        let client = cortecs::Client::builder()
            .api_key(&credential.value)
            .build()?;
        Ok(Self::Cortecs(client))
    }

    pub(crate) fn github_copilot(credential: &Credential) -> Result<Self> {
        let auth = match credential {
            Credential::ApiKey(c) => CopilotAuth::ApiKey(c.value.clone()),
            Credential::OauthToken(c) => CopilotAuth::GitHubAccessToken(c.access_token.clone()),
        };
        Ok(Self::GitHubCopilot {
            client: copilot::Client::from_val(auth)?,
            credential: credential.clone(),
        })
    }

    /// Lists available models from the provider.
    ///
    /// Returns an error for providers that do not support model listing.
    pub async fn list_models(&self) -> Result<ModelList> {
        match self {
            ProviderClient::Ollama(client) => Ok(client.list_models().await?),
            ProviderClient::GitHubCopilot { credential, .. } => {
                fetch_copilot_models(credential).await
            }
            ProviderClient::Cortecs(client) => Ok(client.list_models().await?),
            ProviderClient::OpenAiCompletions(_) | ProviderClient::OpenAiResponses(_) => Err(
                anyhow::anyhow!("model listing is not supported for OpenAI-compatible providers"),
            ),
        }
    }

    /// Builds a [`BoxedAgent`] for the given model alias, optional preamble, and tools.
    pub fn agent(
        &self,
        model: &ModelId,
        preamble: Option<&str>,
        tools: Vec<Box<dyn ToolDyn>>,
    ) -> Result<BoxedAgent> {
        let agent = match self {
            ProviderClient::Ollama(client) => {
                configure_agent(client.agent(model.as_str()), preamble, tools)
            }
            ProviderClient::GitHubCopilot { client, .. } => {
                configure_agent(client.agent(model.as_str()), preamble, tools)
            }
            ProviderClient::OpenAiCompletions(client) => {
                configure_agent(client.agent(model.as_str()), preamble, tools)
            }
            ProviderClient::OpenAiResponses(client) => {
                configure_agent(client.agent(model.as_str()), preamble, tools)
            }
            ProviderClient::Cortecs(client) => {
                configure_agent(client.agent(model.as_str()), preamble, tools)
            }
        };
        Ok(agent)
    }
}

/// Applies the shared configuration — preamble and tools — to any provider's agent builder,
/// returning a type-erased [`BoxedAgent`].
fn configure_agent<M>(
    builder: rig::agent::AgentBuilder<M>,
    preamble: Option<&str>,
    tools: Vec<Box<dyn ToolDyn>>,
) -> BoxedAgent
where
    M: rig::completion::CompletionModel + Send + Sync + 'static,
    M::StreamingResponse: rig::wasm_compat::WasmCompatSend,
{
    // TODO: 100 max turn limit is likely too high but I am not sure what a good limit is right
    // now. More importantly, if we hit the limit right now we error but for this we should find
    // a more graceful way perhaps to handle this.
    let mut b = builder.tools(tools).default_max_turns(100);
    if let Some(p) = preamble {
        b = b.preamble(p);
    }
    Box::new(b.build())
}

const COPILOT_DEFAULT_BASE_URL: &str = "https://api.githubcopilot.com";
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";

/// Fetches available models from the Copilot `/models` endpoint.
///
/// For API key credentials the key is used directly. For OAuth tokens the GitHub
/// access token is first exchanged for a short-lived Copilot token via the token
/// endpoint, which also returns the correct API base URL. Only models that support
/// tool calls are returned.
async fn fetch_copilot_models(credential: &Credential) -> Result<ModelList> {
    #[derive(serde::Deserialize)]
    struct TokenResponse {
        token: String,
        #[serde(default)]
        endpoints: Option<TokenEndpoints>,
    }

    #[derive(serde::Deserialize)]
    struct TokenEndpoints {
        api: Option<String>,
    }

    #[derive(serde::Deserialize)]
    struct ModelsResponse {
        data: Vec<ModelEntry>,
    }

    #[derive(serde::Deserialize)]
    struct ModelEntry {
        id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        capabilities: Option<ModelCapabilities>,
        #[serde(default)]
        limits: Option<ModelLimits>,
    }

    #[derive(serde::Deserialize)]
    struct ModelCapabilities {
        #[serde(default)]
        supports: Option<ModelSupports>,
    }

    #[derive(serde::Deserialize)]
    struct ModelSupports {
        #[serde(default)]
        tool_calls: Option<bool>,
    }

    #[derive(serde::Deserialize)]
    struct ModelLimits {
        #[serde(default)]
        max_context_window_tokens: Option<u32>,
    }

    let http = reqwest::Client::new();
    let (api_token, base_url) = match credential {
        Credential::ApiKey(c) => (c.value.clone(), COPILOT_DEFAULT_BASE_URL.to_string()),
        Credential::OauthToken(c) => {
            let response: TokenResponse = http
                .get(COPILOT_TOKEN_URL)
                .header(reqwest::header::ACCEPT, "application/json")
                .header("editor-version", "vscode/1.95.0")
                .header("editor-plugin-version", "copilot-chat/0.26.7")
                .header("user-agent", "GitHubCopilotChat/0.26.7")
                .header(
                    reqwest::header::AUTHORIZATION,
                    format!("token {}", c.access_token),
                )
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let base_url = response
                .endpoints
                .and_then(|e| e.api)
                .unwrap_or_else(|| COPILOT_DEFAULT_BASE_URL.to_string());
            (response.token, base_url)
        }
    };

    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let response: ModelsResponse = http
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header("copilot-integration-id", "vscode-chat")
        .header("editor-version", "vscode/1.95.0")
        .header("editor-plugin-version", "copilot-chat/0.26.7")
        .header("user-agent", "GitHubCopilotChat/0.26.7")
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {api_token}"),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(ModelList::new(
        response
            .data
            .into_iter()
            .filter(|m| {
                m.capabilities
                    .as_ref()
                    .and_then(|c| c.supports.as_ref())
                    .and_then(|s| s.tool_calls)
                    .unwrap_or(false)
            })
            .map(|m| {
                let context_length = m.limits.and_then(|l| l.max_context_window_tokens);
                let mut model = match m.name {
                    Some(name) => rig::model::Model::new(m.id, name),
                    None => rig::model::Model::from_id(m.id),
                };
                model.context_length = context_length;
                model
            })
            .collect(),
    ))
}
