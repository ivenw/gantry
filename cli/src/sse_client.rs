use anyhow::Result;
use futures::StreamExt;
use gantry_types::Message;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::mpsc,
    task::JoinHandle,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::Message as WsMessage,
};

use gantry_proto::events::{
    FormShownEvent, FormHiddenEvent, InitEvent, MessageReceivedEvent, PendingClearedEvent,
    StreamEndEvent, StreamStartEvent, TokenEvent,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingMessage {
    pub id: String,
    pub client_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormState {
    pub id: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum SseEvent {
    Init(InitEvent),
    MessageReceived(MessageReceivedEvent),
    StreamStart(StreamStartEvent),
    Token(TokenEvent),
    StreamEnd(StreamEndEvent),
    PendingCleared(PendingClearedEvent),
    FormShown(FormShownEvent),
    FormHidden(FormHiddenEvent),
    Error(ErrorEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    pub message: String,
}

pub enum ClientEvent {
    Init {
        client_id: String,
        messages: Vec<Message>,
        pending_message: Option<PendingMessage>,
        form: Option<FormState>,
    },
    MessageReceived {
        id: String,
        client_id: String,
        content: String,
    },
    StreamStart {
        message_id: String,
        pending_of: String,
    },
    Token {
        message_id: String,
        delta: String,
    },
    StreamEnd {
        message_id: String,
        content: String,
    },
    PendingCleared {
        pending_id: String,
    },
    FormShown {
        id: String,
        options: Vec<String>,
    },
    FormHidden {
        id: String,
        selected_by: String,
        selected: String,
    },
    Error {
        message: String,
    },
    Connected,
    Disconnected,
}

pub struct SseClient {
    url: String,
    event_tx: mpsc::Sender<ClientEvent>,
}

impl SseClient {
    pub fn new(addr: &str, port: u16) -> (Self, mpsc::Receiver<ClientEvent>) {
        let url = format!("ws://{}:{}/events", addr, port);
        let (event_tx, event_rx) = mpsc::channel(100);
        (
            Self { url, event_tx },
            event_rx,
        )
    }

    pub async fn connect(&self) -> JoinHandle<Result<()>> {
        let url = self.url.clone();
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            loop {
                match connect_async(&url).await {
                    Ok((ws_stream, _)) => {
                        let _ = event_tx.send(ClientEvent::Connected).await;
                        Self::handle_connection(ws_stream, &event_tx).await;
                        let _ = event_tx.send(ClientEvent::Disconnected).await;
                    }
                    Err(e) => {
                        let _ = event_tx.send(ClientEvent::Error {
                            message: format!("Connection failed: {}", e),
                        }).await;
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            }
        })
    }

    async fn handle_connection<St>(
        ws_stream: tokio_tungstenite::WebSocketStream<St>,
        event_tx: &mpsc::Sender<ClientEvent>,
    ) where
        St: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let (_write, mut read) = ws_stream.split();

        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(WsMessage::Binary(data))) => {
                            if let Some(event) = Self::parse_event(&data) {
                                let _ = event_tx.send(event).await;
                            }
                        }
                        Some(Ok(WsMessage::Close(_))) | None => {
                            break;
                        }
                        Some(Err(e)) => {
                            let _ = event_tx.send(ClientEvent::Error {
                                message: format!("Read error: {}", e),
                            }).await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn parse_event(data: &[u8]) -> Option<ClientEvent> {
        let s = String::from_utf8_lossy(data);
        
        let mut event_type = None;
        let mut event_data = None;

        for line in s.lines() {
            if line.starts_with("event: ") {
                event_type = Some(line.trim_start_matches("event: ").to_string());
            } else if line.starts_with("data: ") {
                event_data = Some(line.trim_start_matches("data: "));
            }
        }

        let data_str = event_data?;
        let ty = event_type?;

        let json: serde_json::Value = serde_json::from_str(data_str).ok()?;

        match ty.as_str() {
            "init" => {
                let client_id = json.get("clientId")?.as_str()?.to_string();
                let messages: Vec<Message> = json.get("messages")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let pending_message: Option<PendingMessage> = json.get("pendingMessage")
                    .and_then(|v| {
                        if v.is_null() { None } else { serde_json::from_value(v.clone()).ok() }
                    });
                let form: Option<FormState> = json.get("form")
                    .and_then(|v| {
                        if v.is_null() { None } else { serde_json::from_value(v.clone()).ok() }
                    });
                Some(ClientEvent::Init { client_id, messages, pending_message, form })
            }
            "message_received" => {
                Some(ClientEvent::MessageReceived {
                    id: json.get("id")?.as_str()?.to_string(),
                    client_id: json.get("clientId")?.as_str()?.to_string(),
                    content: json.get("content")?.as_str()?.to_string(),
                })
            }
            "stream_start" => {
                Some(ClientEvent::StreamStart {
                    message_id: json.get("messageId")?.as_str()?.to_string(),
                    pending_of: json.get("pendingOf")?.as_str()?.to_string(),
                })
            }
            "token" => {
                Some(ClientEvent::Token {
                    message_id: json.get("messageId")?.as_str()?.to_string(),
                    delta: json.get("delta")?.as_str()?.to_string(),
                })
            }
            "stream_end" => {
                Some(ClientEvent::StreamEnd {
                    message_id: json.get("messageId")?.as_str()?.to_string(),
                    content: json.get("content")?.as_str()?.to_string(),
                })
            }
            "pending_cleared" => {
                Some(ClientEvent::PendingCleared {
                    pending_id: json.get("pendingId")?.as_str()?.to_string(),
                })
            }
            "form_shown" => {
                let options: Vec<String> = json.get("options")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                Some(ClientEvent::FormShown {
                    id: json.get("id")?.as_str()?.to_string(),
                    options,
                })
            }
            "form_hidden" => {
                Some(ClientEvent::FormHidden {
                    id: json.get("id")?.as_str()?.to_string(),
                    selected_by: json.get("selectedBy")?.as_str()?.to_string(),
                    selected: json.get("selected")?.as_str()?.to_string(),
                })
            }
            "error" => {
                Some(ClientEvent::Error {
                    message: json.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error").to_string(),
                })
            }
            _ => None,
        }
    }
}
