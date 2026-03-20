use anyhow::Result;
use gantry_types::Message;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::rpc_params;
use jsonrpsee::core::client::ClientT;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct JsonRpcClient {
    inner: HttpClient,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingMessage {
    pub id: String,
    pub client_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectFormResponse {
    pub success: bool,
    pub selected_by: Option<String>,
    pub message: Option<String>,
}

impl JsonRpcClient {
    pub async fn connect_tcp(addr: &str, port: u16) -> Result<Self> {
        let url = format!("http://{}:{}/rpc", addr, port);

        let inner = HttpClient::builder()
            .build(&url)
            .map_err(|e| anyhow::anyhow!("failed to create http client: {}", e))?;

        Ok(Self { inner })
    }

    pub async fn send_message(&self, content: String) -> Result<Vec<Message>> {
        Ok(self.inner.request("send_message", rpc_params![content]).await?)
    }

    pub async fn stream_message(&self, content: String) -> Result<Vec<Message>> {
        Ok(self.inner.request("stream_message", rpc_params![content]).await?)
    }

    pub async fn send_message_sse(&self, content: String, client_id: String) -> Result<PendingMessage> {
        Ok(self.inner.request("stream_message_sse", rpc_params![content, client_id]).await?)
    }

    pub async fn get_messages(&self) -> Result<Vec<Message>> {
        Ok(self.inner.request("get_messages", rpc_params![]).await?)
    }

    pub async fn clear_messages(&self) -> Result<()> {
        Ok(self.inner.request("clear_messages", rpc_params![]).await?)
    }

    pub async fn select_form(&self, form_id: String, selection: String) -> Result<SelectFormResponse> {
        Ok(self.inner.request("select_form", rpc_params![form_id, selection]).await?)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenUpdate {
    pub batch: String,
    pub is_complete: bool,
}

impl TokenUpdate {
    pub fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub fn content(&self) -> &str {
        &self.batch
    }
}

pub type StreamingUpdate = TokenUpdate;
