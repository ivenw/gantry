use anyhow::Result;
use gantry_core::{
    AppEvent, Message, PendingMessage, SelectFormRequest, SelectFormResponse, StreamMessageRequest,
};
use jsonrpsee::core::client::Subscription;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::GantryRpcClient;

pub struct JsonRpcClient {
    inner: WsClient,
}

pub enum WsConnectionEvent {
    Event(AppEvent),
    Disconnected,
    Error(String),
}

impl JsonRpcClient {
    pub async fn connect_ws(addr: &str, port: u16) -> Result<Self> {
        let url = format!("ws://{}:{}", addr, port);
        let inner = WsClientBuilder::default()
            .build(&url)
            .await
            .map_err(|e| anyhow::anyhow!("failed to create ws client: {}", e))?;
        Ok(Self { inner })
    }

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

    pub async fn select_form(
        &self,
        form_id: String,
        selection: String,
    ) -> Result<SelectFormResponse> {
        let req = SelectFormRequest { form_id, selection };
        Ok(self.inner.select_form(req).await?)
    }
}
