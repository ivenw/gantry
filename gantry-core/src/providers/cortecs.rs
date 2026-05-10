use rig::{
    client::{
        self, BearerAuth, Capabilities, Capable, DebugExt, ModelLister, Nothing, Provider,
        ProviderBuilder,
    },
    http_client::{self, HttpClientExt, NoBody},
    model::{Model, ModelList, ModelListingError},
    providers::openai::{GenericCompletionModel, GenericEmbeddingModel},
    wasm_compat::{WasmCompatSend, WasmCompatSync},
};
use serde::Deserialize;

const CORTECS_API_BASE_URL: &str = "https://api.cortecs.ai/v1";

// ================================================================
// Client
// ================================================================

/// Provider extension marker for Cortecs.
#[derive(Debug, Default, Clone, Copy)]
pub struct CortecsExt;

/// Builder extension marker for Cortecs.
#[derive(Debug, Default, Clone, Copy)]
pub struct CortecsBuilder;

type CortecsApiKey = BearerAuth;

impl Provider for CortecsExt {
    type Builder = CortecsBuilder;
    const VERIFY_PATH: &'static str = "/models";
}

impl<H> Capabilities<H> for CortecsExt {
    type Completion = Capable<CompletionModel<H>>;
    type Embeddings = Capable<EmbeddingModel<H>>;
    type Transcription = Nothing;
    type ModelListing = Capable<CortecsModelLister<H>>;
}

impl DebugExt for CortecsExt {}

impl ProviderBuilder for CortecsBuilder {
    type Extension<H>
        = CortecsExt
    where
        H: HttpClientExt;
    type ApiKey = CortecsApiKey;

    const BASE_URL: &'static str = CORTECS_API_BASE_URL;

    fn build<H>(
        _builder: &client::ClientBuilder<Self, Self::ApiKey, H>,
    ) -> http_client::Result<Self::Extension<H>>
    where
        H: HttpClientExt,
    {
        Ok(CortecsExt)
    }
}

/// A Cortecs API client.
pub type Client<H = reqwest::Client> = client::Client<CortecsExt, H>;

/// Builder for a [`Client`].
pub type ClientBuilder<H = reqwest::Client> =
    client::ClientBuilder<CortecsBuilder, CortecsApiKey, H>;

/// Completion model for Cortecs, delegating to OpenAI's generic chat completions implementation.
pub type CompletionModel<H = reqwest::Client> = GenericCompletionModel<CortecsExt, H>;

/// Embedding model for Cortecs, delegating to OpenAI's generic embeddings implementation.
pub type EmbeddingModel<H = reqwest::Client> = GenericEmbeddingModel<CortecsExt, H>;

// ================================================================
// Model listing
// ================================================================

#[derive(Debug, Deserialize)]
struct ListModelsResponse {
    data: Vec<ListModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ListModelEntry {
    id: String,
    #[serde(default)]
    created: Option<u64>,
    #[serde(default)]
    owned_by: Option<String>,
}

impl From<ListModelEntry> for Model {
    fn from(value: ListModelEntry) -> Self {
        let mut model = Model::from_id(value.id);
        model.created_at = value.created;
        model.owned_by = value.owned_by;
        model
    }
}

/// [`ModelLister`] for the Cortecs `/models` endpoint.
#[derive(Clone)]
pub struct CortecsModelLister<H = reqwest::Client> {
    client: Client<H>,
}

impl<H> ModelLister<H> for CortecsModelLister<H>
where
    H: HttpClientExt + WasmCompatSend + WasmCompatSync + 'static,
{
    type Client = Client<H>;

    fn new(client: Self::Client) -> Self {
        Self { client }
    }

    async fn list_all(&self) -> Result<ModelList, ModelListingError> {
        let path = "/models";
        let req = self.client.get(path)?.body(NoBody)?;
        let response = self.client.send::<_, Vec<u8>>(req).await?;

        if !response.status().is_success() {
            let status_code = response.status().as_u16();
            let body = response.into_body().await?;
            let message = String::from_utf8_lossy(&body).to_string();
            return Err(ModelListingError::api_error(status_code, message));
        }

        let body = response.into_body().await?;
        let api_resp: ListModelsResponse = serde_json::from_slice(&body)
            .map_err(|e| ModelListingError::parse_error(e.to_string()))?;
        let models = api_resp.data.into_iter().map(Model::from).collect();

        Ok(ModelList::new(models))
    }
}
