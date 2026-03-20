use anyhow::Result;
use gantry_types::Message;
use jsonrpsee::{server::ServerBuilder, RpcModule};
use serde::{Deserialize, Serialize};
use serde_json::{to_value, Value};
use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::server::JsonRpcServer;
use crate::sse_server::{start_sse_server, ClientRegistry};

pub struct GantryRpcServer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageResponse {
    pub pending_id: String,
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectFormRequest {
    pub form_id: String,
    pub selection: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectFormResponse {
    pub success: bool,
    pub selected_by: Option<String>,
    pub message: Option<String>,
}

impl GantryRpcServer {
    pub fn create_rpc_module(server: JsonRpcServer) -> RpcModule<JsonRpcServer> {
        let mut module = RpcModule::new(server.clone());

        module
            .register_async_method("send_message", |params, server, _| async move {
                let content: String = params.one().unwrap_or_default();
                dbg!("send_message received: ", &content);
                let messages = server.send_message(content).await;
                to_value(messages).unwrap_or(Value::Null)
            })
            .unwrap();

        module
            .register_async_method("stream_message", |params, server, _| async move {
                let content: String = params.one().unwrap_or_default();
                dbg!("stream_message received: ", &content);
                let messages = server.stream_message(content).await;
                to_value(messages).unwrap_or(Value::Null)
            })
            .unwrap();

        module
            .register_async_method("stream_message_sse", |params, server, _| async move {
                #[derive(Deserialize)]
                struct Params {
                    content: String,
                    #[serde(default = "default_client_id")]
                    client_id: String,
                }
                let params: Params = params.parse().unwrap_or(Params {
                    content: String::new(),
                    client_id: default_client_id(),
                });
                dbg!("stream_message_sse received: ", &params.content, &params.client_id);
                match server.stream_message_sse(params.client_id, params.content).await {
                    Ok(pending) => to_value(pending).unwrap_or(Value::Null),
                    Err(e) => to_value(serde_json::json!({
                        "error": e.to_string()
                    })).unwrap_or(Value::Null),
                }
            })
            .unwrap();

        module
            .register_method("get_messages", |_params, server, _| {
                let messages: Vec<Message> = server.get_messages();
                to_value(messages).unwrap_or(Value::Null)
            })
            .unwrap();

        module
            .register_method("clear_messages", |_params, server, _| {
                server.clear_messages();
                Ok::<(), jsonrpsee::types::error::ErrorCode>(())
            })
            .unwrap();

        module
            .register_async_method("select_form", |params, server, _| async move {
                #[derive(Deserialize)]
                struct FormParams {
                    form_id: String,
                    selection: String,
                }
                let form_params: FormParams = params.parse().unwrap_or(FormParams {
                    form_id: String::new(),
                    selection: String::new(),
                });

                let active_form = server.get_active_form();
                if let Some(form) = active_form {
                    if form.id == form_params.form_id {
                        server.hide_form(&form, "client", &form_params.selection);
                        to_value(SelectFormResponse {
                            success: true,
                            selected_by: Some("client".to_string()),
                            message: Some(format!("Selected: {}", form_params.selection)),
                        }).unwrap_or(Value::Null)
                    } else {
                        to_value(SelectFormResponse {
                            success: false,
                            selected_by: None,
                            message: Some("Form not found".to_string()),
                        }).unwrap_or(Value::Null)
                    }
                } else {
                    to_value(SelectFormResponse {
                        success: false,
                        selected_by: None,
                        message: Some("No active form".to_string()),
                    }).unwrap_or(Value::Null)
                }
            })
            .unwrap();

        module
    }

    pub async fn start(
        addr: String,
        port: u16,
        sse_port: u16,
    ) -> Result<(jsonrpsee::server::ServerHandle, JoinHandle<Result<()>>)> {
        let registry = Arc::new(ClientRegistry::new());
        let server_state = JsonRpcServer::new(registry.clone());
        server_state.init_llm_client().await?;
        let module = Self::create_rpc_module(server_state.clone());

        let rpc_server = ServerBuilder::new()
            .build((addr.as_str(), port))
            .await?;
        let handle = rpc_server.start(module);

        let sse_handle = tokio::spawn(start_sse_server(server_state, registry, addr, sse_port));

        Ok((handle, sse_handle))
    }
}

fn default_client_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
