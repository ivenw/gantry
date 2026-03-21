use crate::app::service::AppService;
use anyhow::Result;
use gantry_contract::{AppEvent, Message, SelectFormRequest, StreamMessageRequest};
use jsonrpsee::RpcModule;
use jsonrpsee::server::{ServerBuilder, ServerConfig};
use jsonrpsee::types::ErrorObjectOwned;
use serde_json::{Value, to_value};
use uuid::Uuid;

fn invalid_params(details: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32602, "Invalid params", Some(details.into()))
}

fn internal_error(details: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32603, "Internal error", Some(details.into()))
}

pub fn create_rpc_module(app: AppService) -> Result<RpcModule<AppService>> {
    let mut module = RpcModule::new(app);

    module.register_async_method("send_message", |params, app, _| async move {
        let content: String = params
            .one()
            .map_err(|e| invalid_params(format!("send_message expects a string: {}", e)))?;
        dbg!("rpc.send_message.request", &content);
        let messages = app.send_message(content).await;
        dbg!("rpc.send_message.response_count", messages.len());
        Ok::<Value, ErrorObjectOwned>(to_value(messages).unwrap_or(Value::Null))
    })?;

    module.register_async_method("stream_message", |params, app, _| async move {
        let req: StreamMessageRequest = params
            .one()
            .map_err(|e| invalid_params(format!("stream_message expects an object: {}", e)))?;
        dbg!("rpc.stream_message.request.content", &req.content);
        let pending = app
            .stream_message(req)
            .await
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!(
            "rpc.stream_message.response.pending",
            &pending.id,
            &pending.content
        );
        Ok::<Value, ErrorObjectOwned>(to_value(pending).unwrap_or(Value::Null))
    })?;

    module.register_subscription(
        "subscribe_events",
        "events",
        "unsubscribe_events",
        |_, pending, app, _| async move {
            dbg!("rpc.subscribe_events.request");
            let Ok(sink) = pending.accept().await else {
                dbg!("rpc.subscribe_events.accept_failed");
                return;
            };
            dbg!("rpc.subscribe_events.accepted");

            let init_event = app.init_event(Uuid::new_v4().to_string()).await;
            if !send_event(&sink, &init_event).await {
                dbg!("rpc.subscribe_events.init_send_failed");
                return;
            }
            dbg!("rpc.subscribe_events.init_sent");

            let mut event_rx = app.subscribe_events();
            loop {
                tokio::select! {
                    _ = sink.closed() => break,
                    event = event_rx.recv() => {
                        match event {
                            Ok(ev) => {
                                dbg!("rpc.subscribe_events.broadcast_event", &ev);
                                if !send_event(&sink, &ev).await {
                                    dbg!("rpc.subscribe_events.broadcast_send_failed");
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                dbg!("rpc.subscribe_events.lagged");
                                let catch_up = app.init_event(Uuid::new_v4().to_string()).await;
                                if !send_event(&sink, &catch_up).await {
                                    dbg!("rpc.subscribe_events.catchup_send_failed");
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
        },
    )?;

    module.register_async_method("select_form", |params, app, _| async move {
        let req: SelectFormRequest = params
            .one()
            .map_err(|e| invalid_params(format!("select_form expects an object: {}", e)))?;
        dbg!("rpc.select_form.request", &req.form_id, &req.selection);
        let response = app.select_form(req.form_id, req.selection).await;
        dbg!("rpc.select_form.response", &response);
        Ok::<Value, ErrorObjectOwned>(to_value(response).unwrap_or(Value::Null))
    })?;

    module.register_async_method("get_messages", |_params, app, _| async move {
        dbg!("rpc.get_messages.request");
        let messages: Vec<Message> = app.get_messages().await;
        dbg!("rpc.get_messages.response_count", messages.len());
        Ok::<Value, ErrorObjectOwned>(to_value(messages).unwrap_or(Value::Null))
    })?;

    module.register_async_method("clear_messages", |_params, app, _| async move {
        dbg!("rpc.clear_messages.request");
        app.clear_messages().await;
        dbg!("rpc.clear_messages.done");
        Ok::<(), ErrorObjectOwned>(())
    })?;

    Ok(module)
}

pub async fn start_rpc_server(
    addr: &str,
    port: u16,
    app: AppService,
) -> Result<jsonrpsee::server::ServerHandle> {
    dbg!("rpc.start_server", addr, port);
    let module = create_rpc_module(app)?;
    let config = ServerConfig::builder().ws_only().build();
    let rpc_server = ServerBuilder::new().set_config(config).build((addr, port)).await?;
    dbg!("rpc.server_listening", addr, port);
    Ok(rpc_server.start(module))
}

async fn send_event(
    sink: &jsonrpsee::SubscriptionSink,
    event: &AppEvent,
) -> bool {
    let Ok(payload) = serde_json::value::to_raw_value(event) else {
        dbg!("rpc.send_event.serialize_failed");
        return false;
    };
    let sent = sink.send(payload).await.is_ok();
    dbg!("rpc.send_event.sent", sent, event);
    sent
}
