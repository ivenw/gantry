use anyhow::Result;
use axum::{
    extract::{State, WebSocketUpgrade},
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use axum::extract::ws::{Message as WsMessage, WebSocket};

use crate::events::{
    create_error_event, create_form_hidden_event, create_form_shown_event, create_init_event,
    create_message_received_event, create_pending_cleared_event, create_stream_end_event,
    create_stream_start_event, create_token_event, ClientId, FormState,
    PendingMessage, SseEvent as AppSseEvent,
};
use crate::server::JsonRpcServer;
use gantry_types::Message;

pub struct ClientRegistry {
    clients: Arc<std::sync::Mutex<std::collections::HashMap<ClientId, broadcast::Sender<AppSseEvent>>>>,
    event_tx: broadcast::Sender<AppSseEvent>,
    pending_message: Arc<std::sync::Mutex<Option<PendingMessage>>>,
    active_form: Arc<std::sync::Mutex<Option<FormState>>>,
}

impl ClientRegistry {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        Self {
            clients: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            event_tx,
            pending_message: Arc::new(std::sync::Mutex::new(None)),
            active_form: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn register_client(&self, client_id: ClientId) -> broadcast::Receiver<AppSseEvent> {
        let tx = self.event_tx.clone();
        let mut clients = self.clients.lock().unwrap();
        clients.insert(client_id, tx.clone());
        self.event_tx.subscribe()
    }

    pub fn unregister_client(&self, client_id: ClientId) {
        let mut clients = self.clients.lock().unwrap();
        clients.remove(&client_id);
    }

    pub fn broadcast(&self, event: AppSseEvent) {
        let _ = self.event_tx.send(event);
    }

    pub fn set_pending_message(&self, pending: Option<PendingMessage>) {
        let mut current = self.pending_message.lock().unwrap();
        *current = pending;
    }

    pub fn get_pending_message(&self) -> Option<PendingMessage> {
        self.pending_message.lock().unwrap().clone()
    }

    pub fn set_active_form(&self, form: Option<FormState>) {
        let mut current = self.active_form.lock().unwrap();
        *current = form;
    }

    pub fn get_active_form(&self) -> Option<FormState> {
        self.active_form.lock().unwrap().clone()
    }

    pub fn create_init_event(&self, client_id: &ClientId, messages: Vec<Message>) -> AppSseEvent {
        create_init_event(
            client_id,
            messages,
            self.get_pending_message(),
            self.get_active_form(),
        )
    }

    pub fn broadcast_message_received(&self, pending: &PendingMessage) {
        self.broadcast(create_message_received_event(pending));
    }

    pub fn broadcast_stream_start(&self, message_id: &str, pending_of: &str) {
        self.broadcast(create_stream_start_event(message_id, pending_of));
    }

    pub fn broadcast_token(&self, message_id: &str, delta: &str) {
        self.broadcast(create_token_event(message_id, delta));
    }

    pub fn broadcast_stream_end(&self, message_id: &str, content: &str) {
        self.broadcast(create_stream_end_event(message_id, content));
    }

    pub fn broadcast_pending_cleared(&self, pending_id: &str) {
        self.broadcast(create_pending_cleared_event(pending_id));
    }

    pub fn broadcast_form_shown(&self, form: &FormState) {
        self.set_active_form(Some(form.clone()));
        self.broadcast(create_form_shown_event(form));
    }

    pub fn broadcast_form_hidden(&self, form: &FormState, selected_by: &str, selected: &str) {
        self.set_active_form(None);
        self.broadcast(create_form_hidden_event(form, selected_by, selected));
    }

    pub fn broadcast_error(&self, message: &str) {
        self.broadcast(create_error_event(message));
    }
}

impl Default for ClientRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AppState {
    pub server: JsonRpcServer,
    pub registry: Arc<ClientRegistry>,
}

async fn sse_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> axum::response::Response {
    let client_id = ClientId::new();
    
    ws.on_upgrade(move |socket| {
        let client_id = client_id.clone();
        let state = state.clone();
        handle_socket(socket, client_id, state)
    })
}

async fn handle_socket(
    socket: WebSocket,
    client_id: ClientId,
    state: Arc<AppState>,
) {
    let (mut sender, mut receiver) = socket.split();

    let init_event = state.registry.create_init_event(&client_id, state.server.get_messages());
    let sse_bytes = init_event.to_sse_format().into_bytes();
    if sender.send(WsMessage::Binary(sse_bytes)).await.is_err() {
        return;
    }

    let mut event_rx = state.registry.register_client(client_id.clone());

    loop {
        tokio::select! {
            Some(msg) = receiver.next() => {
                if let Ok(msg) = msg {
                    if matches!(msg, WsMessage::Close(_)) {
                        break;
                    }
                }
            }
            event = event_rx.recv() => {
                match event {
                    Ok(ev) => {
                        let sse_bytes = ev.to_sse_format().into_bytes();
                        if sender.send(WsMessage::Binary(sse_bytes)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_n)) => {
                        let init_event = state.registry.create_init_event(&client_id, state.server.get_messages());
                        let sse_bytes = init_event.to_sse_format().into_bytes();
                        if sender.send(WsMessage::Binary(sse_bytes)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        }
    }

    state.registry.unregister_client(client_id);
}

pub async fn start_sse_server(
    server: JsonRpcServer,
    registry: Arc<ClientRegistry>,
    addr: String,
    port: u16,
) -> Result<()> {
    let state = Arc::new(AppState { server, registry });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/events", get(sse_handler))
        .layer(cors)
        .with_state(state);

    let bind_addr = format!("{}:{}", addr, port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    println!("SSE server listening on http://{}", bind_addr);

    axum::serve(listener, app).await?;
    Ok(())
}
