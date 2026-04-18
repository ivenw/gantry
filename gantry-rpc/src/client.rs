use anyhow::Result;
use gantry_core::{
    AppEvent, Message, PendingMessage, SelectFormRequest, SelectFormResponse, SessionInfo,
    StreamMessageRequest,
};
use jsonrpsee::core::client::Subscription;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use std::path::PathBuf;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::GantryRpcClient;

pub struct JsonRpcClient {
    inner: std::sync::Arc<WsClient>,
}

impl JsonRpcClient {
    pub async fn connect_ws(addr: &str, port: u16) -> Result<Self> {
        let url = format!("ws://{}:{}", addr, port);
        let inner = WsClientBuilder::default()
            .build(&url)
            .await
            .map_err(|e| anyhow::anyhow!("failed to create ws client: {}", e))?;
        Ok(Self {
            inner: std::sync::Arc::new(inner),
        })
    }

    // --- project & session management ---

    pub async fn register_project(&self, path: PathBuf) -> Result<()> {
        Ok(self.inner.register_project(path).await?)
    }

    pub async fn list_projects(&self) -> Result<Vec<PathBuf>> {
        Ok(self.inner.list_projects().await?)
    }

    pub async fn unregister_project(&self, path: PathBuf) -> Result<()> {
        Ok(self.inner.unregister_project(path).await?)
    }

    pub async fn create_session(&self, project_path: PathBuf) -> Result<String> {
        Ok(self.inner.create_session(project_path).await?)
    }

    pub async fn list_sessions(&self, project_path: PathBuf) -> Result<Vec<SessionInfo>> {
        Ok(self.inner.list_sessions(project_path).await?)
    }

    pub async fn bind_session(&self, session_id: String, project_path: PathBuf) -> Result<()> {
        Ok(self.inner.bind_session(session_id, project_path).await?)
    }

    // --- messaging ---

    pub async fn subscribe_events(
        &self,
    ) -> Result<(JoinHandle<()>, mpsc::Receiver<WsConnectionEvent>)> {
        let mut sub: Subscription<AppEvent> = self.inner.subscribe_events().await?;

        let (event_tx, event_rx) = mpsc::channel(100);
        let handle = tokio::spawn(async move {
            while let Some(next) = sub.next().await {
                match next {
                    Ok(event) => {
                        if event_tx
                            .send(WsConnectionEvent::Event(event))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(err) => {
                        let _ = event_tx
                            .send(WsConnectionEvent::Error(format!(
                                "Subscription error: {}",
                                err
                            )))
                            .await;
                        break;
                    }
                }
            }

            let _ = event_tx.send(WsConnectionEvent::Disconnected).await;
        });

        Ok((handle, event_rx))
    }

    pub async fn send_message(&self, content: String) -> Result<Vec<Message>> {
        Ok(self.inner.send_message(content).await?)
    }

    pub async fn stream_message(&self, content: String) -> Result<PendingMessage> {
        let req = StreamMessageRequest { content };
        Ok(self.inner.stream_message(req).await?)
    }

    pub async fn get_messages(&self) -> Result<Vec<Message>> {
        Ok(self.inner.get_messages().await?)
    }

    pub async fn clear_messages(&self) -> Result<()> {
        Ok(self.inner.clear_messages().await?)
    }

    pub async fn interrupt_stream(&self, message_id: String) -> Result<bool> {
        Ok(self.inner.interrupt_stream(message_id).await?)
    }

    pub async fn select_form(
        &self,
        form_id: String,
        selection: String,
    ) -> Result<SelectFormResponse> {
        let req = SelectFormRequest { form_id, selection };
        Ok(self.inner.select_form(req).await?)
    }

    pub async fn ping(&self) -> Result<()> {
        Ok(self.inner.ping().await?)
    }

    pub async fn get_tree(&self) -> Result<gantry_core::SessionTree> {
        Ok(self.inner.get_tree().await?)
    }

    pub async fn branch(&self, entry_id: String) -> Result<()> {
        Ok(self.inner.branch(entry_id).await?)
    }
}

impl Clone for JsonRpcClient {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub enum WsConnectionEvent {
    Event(AppEvent),
    Disconnected,
    Error(String),
}
