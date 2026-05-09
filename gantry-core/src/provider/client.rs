use anyhow::Result;
use rig::client::{CompletionClient, Nothing, ProviderClient as _};
use rig::model::ModelList;
use rig::providers::copilot::{self, CopilotAuth};
use rig::providers::{ollama, openai};

use crate::config::{
    ApiKeyCredential, Credential, OllamaProviderConfig, OpenAiCompletionsProviderConfig,
    OpenAiResponsesProviderConfig,
};
use crate::provider::ModelAlias;
use crate::provider::ToolCallEvent;
use crate::provider::agent::{BoxedAgent, ToolCallHook};
use crate::tools::{BashTool, EditTool, GrepTool, ReadTool, TreeTool, WriteTool};

/// A constructed, ready-to-use provider client that can list models and create agents.
pub enum ProviderClient {
    Ollama(ollama::Client),
    GitHubCopilot {
        client: copilot::Client,
        credential: Credential,
    },
    OpenAiCompletions(openai::CompletionsClient),
    OpenAiResponses(openai::Client),
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
        use rig::client::ModelListingClient as _;

        match self {
            ProviderClient::Ollama(client) => Ok(client.list_models().await?),
            ProviderClient::GitHubCopilot { credential, .. } => {
                fetch_copilot_models(credential).await
            }
            ProviderClient::OpenAiCompletions(_) | ProviderClient::OpenAiResponses(_) => Err(
                anyhow::anyhow!("model listing is not supported for OpenAI-compatible providers"),
            ),
        }
    }

    /// Builds a [`BoxedAgent`] for the given model alias, optional preamble, and hook sender.
    pub fn agent(
        &self,
        model: &ModelAlias,
        preamble: Option<&str>,
        hook_tx: tokio::sync::mpsc::UnboundedSender<ToolCallEvent>,
    ) -> Result<BoxedAgent> {
        let hook = ToolCallHook::new(hook_tx);
        match self {
            ProviderClient::Ollama(client) => {
                let mut builder = client.agent(model.as_str()).hook(hook);
                if let Some(p) = preamble {
                    builder = builder.preamble(p);
                }
                Ok(Box::new(
                    builder
                        .tool(ReadTool)
                        .tool(WriteTool)
                        .tool(EditTool)
                        .tool(GrepTool)
                        .tool(TreeTool)
                        .tool(BashTool)
                        .build(),
                ))
            }
            ProviderClient::GitHubCopilot { client, .. } => {
                let mut builder = client.agent(model.as_str()).hook(hook);
                if let Some(p) = preamble {
                    builder = builder.preamble(p);
                }
                Ok(Box::new(
                    builder
                        .tool(ReadTool)
                        .tool(WriteTool)
                        .tool(EditTool)
                        .tool(GrepTool)
                        .tool(TreeTool)
                        .tool(BashTool)
                        .build(),
                ))
            }
            ProviderClient::OpenAiCompletions(client) => {
                let mut builder = client.agent(model.as_str()).hook(hook);
                if let Some(p) = preamble {
                    builder = builder.preamble(p);
                }
                Ok(Box::new(
                    builder
                        .tool(ReadTool)
                        .tool(WriteTool)
                        .tool(EditTool)
                        .tool(GrepTool)
                        .tool(TreeTool)
                        .tool(BashTool)
                        .build(),
                ))
            }
            ProviderClient::OpenAiResponses(client) => {
                let mut builder = client.agent(model.as_str()).hook(hook);
                if let Some(p) = preamble {
                    builder = builder.preamble(p);
                }
                Ok(Box::new(
                    builder
                        .tool(ReadTool)
                        .tool(WriteTool)
                        .tool(EditTool)
                        .tool(GrepTool)
                        .tool(TreeTool)
                        .tool(BashTool)
                        .build(),
                ))
            }
        }
    }
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
            .map(|m| match m.name {
                Some(name) => rig::model::Model::new(m.id, name),
                None => rig::model::Model::from_id(m.id),
            })
            .collect(),
    ))
}
