use anyhow::Result;
use gantry_core::{
    AppEvent, AppService, Message, PendingMessage, SelectFormRequest, SelectFormResponse,
    StreamMessageRequest,
};
use jsonrpsee::RpcModule;
use jsonrpsee::core::{RpcResult, SubscriptionResult, async_trait};
use jsonrpsee::server::{
    PendingSubscriptionSink, ServerBuilder, ServerConfig, ServerHandle, SubscriptionSink,
};
use jsonrpsee::types::ErrorObjectOwned;

use crate::GantryRpcServer;

pub struct RpcApp {
    inner: AppService,
}

impl RpcApp {
    fn new(inner: AppService) -> Self {
        Self { inner }
    }
}

fn internal_error(details: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32603, "Internal error", Some(details.into()))
}

#[async_trait]
impl GantryRpcServer for RpcApp {
    async fn send_message(&self, content: String) -> RpcResult<Vec<Message>> {
        dbg!("rpc.send_message.request", &content);
        let messages = self.inner.send_message(content).await;
        dbg!("rpc.send_message.response_count", messages.len());
        Ok(messages)
    }

    async fn stream_message(&self, req: StreamMessageRequest) -> RpcResult<PendingMessage> {
        dbg!("rpc.stream_message.request.content", &req.content);
        let pending = self
            .inner
            .stream_message(req)
            .await
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!(
            "rpc.stream_message.response.pending",
            &pending.id,
            &pending.content
        );
        Ok(pending)
    }

    async fn subscribe_events(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
        dbg!("rpc.subscribe_events.request");
        let sink = pending.accept().await.map_err(|e| e.to_string())?;
        dbg!("rpc.subscribe_events.accepted");

        let init_event = self.inner.init_event().await;
        if let Err(err) = send_event(&sink, &init_event).await {
            dbg!("rpc.subscribe_events.init_send_failed", &err);
            return Ok(());
        }
        dbg!("rpc.subscribe_events.init_sent");

        let mut event_rx = self.inner.subscribe_events();
        loop {
            tokio::select! {
                _ = sink.closed() => break,
                event = event_rx.recv() => {
                    match event {
                        Ok(ev) => {
                            dbg!("rpc.subscribe_events.broadcast_event", &ev);
                            if let Err(err) = send_event(&sink, &ev).await {
                                dbg!("rpc.subscribe_events.broadcast_send_failed", &err);
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            dbg!("rpc.subscribe_events.lagged");
                            let catch_up = self.inner.init_event().await;
                            if let Err(err) = send_event(&sink, &catch_up).await {
                                dbg!("rpc.subscribe_events.catchup_send_failed", &err);
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            dbg!("rpc.subscribe_events.closed");
                            break;
                        }
                    }
                }
            }
        }
        dbg!("rpc.subscribe_events.ended");
        Ok(())
    }

    async fn select_form(&self, req: SelectFormRequest) -> RpcResult<SelectFormResponse> {
        dbg!("rpc.select_form.request", &req.form_id, &req.selection);
        let response = self.inner.select_form(req.form_id, req.selection).await;
        dbg!("rpc.select_form.response", &response);
        Ok(response)
    }

    async fn get_messages(&self) -> RpcResult<Vec<Message>> {
        dbg!("rpc.get_messages.request");
        let messages = self.inner.get_messages().await;
        dbg!("rpc.get_messages.response_count", messages.len());
        Ok(messages)
    }

    async fn clear_messages(&self) -> RpcResult<()> {
        dbg!("rpc.clear_messages.request");
        self.inner.clear_messages().await;
        dbg!("rpc.clear_messages.done");
        Ok(())
    }

    async fn interrupt_stream(&self, message_id: String) -> RpcResult<bool> {
        dbg!("rpc.interrupt_stream.request", &message_id);
        let result = self.inner.interrupt_stream(message_id).await;
        dbg!("rpc.interrupt_stream.response", result);
        Ok(result)
    }

    async fn ping(&self) -> RpcResult<()> {
        dbg!("rpc.ping.request");
        Ok(())
    }
}

pub async fn start_rpc_server<Context>(
    addr: &str,
    port: u16,
    module: RpcModule<Context>,
) -> Result<ServerHandle>
where
    Context: Send + Sync + 'static,
{
    dbg!("rpc.start_server", addr, port);
    let config = ServerConfig::builder().ws_only().build();
    let rpc_server = ServerBuilder::new()
        .set_config(config)
        .build((addr, port))
        .await?;
    dbg!("rpc.server_listening", addr, port);
    Ok(rpc_server.start(module))
}

pub async fn start_app_rpc_server(addr: &str, port: u16, app: AppService) -> Result<ServerHandle> {
    start_rpc_server(addr, port, RpcApp::new(app).into_rpc().remove_context()).await
}

async fn send_event(sink: &SubscriptionSink, event: &AppEvent) -> SubscriptionResult {
    let Ok(payload) = serde_json::value::to_raw_value(event) else {
        dbg!("rpc.send_event.serialize_failed");
        return Err("failed to serialize event".into());
    };
    sink.send(payload).await.map_err(|e| e.to_string())?;
    dbg!("rpc.send_event.sent", true, event);
    Ok(())
}
