use gantry_core::AppEvent;
use gantry_rpc::{JsonRpcClient, WsConnectionEvent};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;

pub enum Msg {
    // Input
    Key(crossterm::event::KeyEvent),

    // WebSocket transport (unwrapped from WsConnectionEvent)
    WsDisconnected,
    WsError(String),

    // Server app events
    AppEvent(AppEvent),

    // Streaming result
    StreamResult(Result<(), String>),

    // Connection lifecycle
    ReconnectSuccess {
        client: JsonRpcClient,
        session_id: String,
        event_handle: JoinHandle<()>,
        event_rx: Receiver<WsConnectionEvent>,
        clear_messages: bool,
    },

    // Command results (replaces CommandEffect)
    SetStatus(String),
    NewSession {
        client: Arc<JsonRpcClient>,
        session_id: String,
        event_handle: JoinHandle<()>,
        event_rx: Receiver<WsConnectionEvent>,
    },

    // Scroll the chat window (positive = up, negative = down)
    ScrollChat(i32),

    // Side-effect signals intercepted by Runtime before update()
    SendMessage(String),
    InterruptStream,
    ExecuteCommand(std::sync::Arc<dyn crate::commands::Command>),
    Quit,
}
