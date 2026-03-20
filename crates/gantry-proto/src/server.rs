use anyhow::Result;
use gantry_types::{Message, Role};
use jsonrpsee::{server::{ServerBuilder, ServerHandle}, types::error::ErrorCode, RpcModule};
use serde_json::{value::to_value, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct JsonRpcServer {
    messages: Arc<Mutex<Vec<Message>>>,
    sender: broadcast::Sender<Message>,
}

impl JsonRpcServer {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(100);
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            sender,
        }
    }

    pub fn send_message(&self, content: String) -> Vec<Message> {
        let user_msg = Message::new(Role::User, content.clone());
        self.messages.lock().unwrap().push(user_msg);

        let response = Message::new(Role::Assistant, format!("Echo: {}", content));
        self.messages.lock().unwrap().push(response.clone());

        let _ = self.sender.send(response.clone());
        self.messages.lock().unwrap().clone()
    }

    pub fn get_messages(&self) -> Vec<Message> {
        self.messages.lock().unwrap().clone()
    }

    pub fn clear_messages(&self) {
        self.messages.lock().unwrap().clear();
    }
}

impl Default for JsonRpcServer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct GantryRpcServer;

impl GantryRpcServer {
    pub fn create_rpc_module(server: JsonRpcServer) -> RpcModule<JsonRpcServer> {
        let mut module = RpcModule::new(server);

        module
            .register_method("send_message", |params, server| {
                let content: String = params.one().unwrap_or_default();
                let messages = server.send_message(content);
                to_value(messages).unwrap_or(Value::Null)
            })
            .unwrap();

        module
            .register_method("get_messages", |_params, server| {
                let messages: Vec<Message> = server.get_messages();
                to_value(messages).unwrap_or(Value::Null)
            })
            .unwrap();

        module
            .register_method("clear_messages", |_params, server| {
                server.clear_messages();
                Ok::<(), ErrorCode>(())
            })
            .unwrap();

        module
    }

    pub async fn start(addr: &str, port: u16) -> Result<ServerHandle> {
        let server = JsonRpcServer::new();
        let module = Self::create_rpc_module(server);
        let server = ServerBuilder::new().build((addr, port)).await?;
        let handle = server.start(module);
        Ok(handle)
    }
}
