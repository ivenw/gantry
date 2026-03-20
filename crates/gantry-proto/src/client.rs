use anyhow::Result;
use gantry_types::Message;
use jsonrpsee::{rpc_params, core::client::ClientT};
pub struct JsonRpcClient {
    inner: jsonrpsee::http_client::HttpClient,
}

impl JsonRpcClient {
    pub async fn connect_tcp(addr: &str, port: u16) -> Result<Self> {
        let url = format!("http://{}:{}/rpc", addr, port);
        
        let inner = jsonrpsee::http_client::HttpClientBuilder::default()
            .build(&url)
            .map_err(|e| anyhow::anyhow!("failed to create http client: {}", e))?;
        
        Ok(Self { inner })
    }

    pub async fn send_message(&self, content: String) -> Result<Vec<Message>> {
        Ok(self.inner.request("send_message", (content,)).await?)
    }

    pub async fn get_messages(&self) -> Result<Vec<Message>> {
        Ok(self.inner.request("get_messages", rpc_params![]).await?)
    }

    pub async fn clear_messages(&self) -> Result<()> {
        Ok(self.inner.request("clear_messages", rpc_params![]).await?)
    }
}
