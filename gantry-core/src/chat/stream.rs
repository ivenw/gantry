use crate::chat::events::StreamMessageRequest;
use crate::chat::{Message, PendingMessage, Role};
use crate::project::resource_loader::discover_agents_md;
use crate::project::system_prompt::build_system_prompt;
use crate::provider::ModelSelection;
use crate::provider::agent_factory::RigAgentFactory;
use crate::session::Session;
use anyhow::Result;
use rig::message::Message as RigMessage;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum StreamEvent {
    MessageReceived {
        pending_id: String,
        content: String,
    },
    StreamStart {
        message_id: String,
        pending_id: String,
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
    Error {
        message: String,
    },
}

pub fn to_rig_messages(messages: Vec<Message>) -> Vec<RigMessage> {
    messages
        .into_iter()
        .map(|msg| match msg.role {
            Role::User => RigMessage::user(msg.content),
            Role::Assistant => RigMessage::assistant(msg.content),
            Role::Error => RigMessage::user(format!("[Error]: {}", msg.content)),
        })
        .collect()
}

pub(crate) async fn stream_message(
    req: StreamMessageRequest,
    project_path: &Path,
    session: &Arc<Mutex<Session>>,
    active_selection: &Arc<Mutex<ModelSelection>>,
    agent_factory: &RigAgentFactory,
) -> Result<(
    PendingMessage,
    oneshot::Sender<()>,
    mpsc::Receiver<StreamEvent>,
)> {
    dbg!("session.stream_message.request", &req.content);

    let pending = PendingMessage::new(req.content.clone());

    {
        let mut sess = session.lock().await;
        sess.append(Role::User, req.content)
            .unwrap_or_else(|_| panic!("failed to persist message"));
    }

    let snapshot = session.lock().await.context_messages();
    let selection = active_selection.lock().await.clone();
    let system_prompt = build_system_prompt(&discover_agents_md(project_path));
    let mut rig_messages = to_rig_messages(snapshot);

    let (event_tx, event_rx) = mpsc::channel(256);
    let (cancel_tx, cancel_rx) = oneshot::channel();

    let _ = event_tx
        .send(StreamEvent::MessageReceived {
            pending_id: pending.id.clone(),
            content: pending.content.clone(),
        })
        .await;

    let Some(prompt) = rig_messages.pop() else {
        let _ = event_tx
            .send(StreamEvent::PendingCleared {
                pending_id: pending.id.clone(),
            })
            .await;
        let _ = event_tx
            .send(StreamEvent::Error {
                message: "cannot generate tokens with empty message history".to_string(),
            })
            .await;
        return Ok((pending, cancel_tx, event_rx));
    };

    dbg!(
        "session.stream_message.snapshot_len",
        rig_messages.len() + 1
    );
    let message_id = Uuid::new_v4().to_string();

    let _ = event_tx
        .send(StreamEvent::StreamStart {
            message_id: message_id.clone(),
            pending_id: pending.id.clone(),
        })
        .await;

    let (token_tx, mut token_rx) = mpsc::channel(128);
    let agent = agent_factory.agent(&selection, Some(&system_prompt)).await;
    let llm_task = tokio::spawn(async move {
        let agent = agent?;
        agent.stream_chat(prompt, rig_messages, token_tx).await
    });

    let session_clone = session.clone();
    let pending_id = pending.id.clone();
    let message_id_clone = message_id.clone();

    tokio::spawn(async move {
        let mut accumulated = String::new();
        let mut token_count = 0usize;
        let mut cancelled = false;
        let mut line_buffer = String::new();
        let mut cancel_rx = cancel_rx;

        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    dbg!("session.stream_message.cancelled");
                    cancelled = true;
                    break;
                }
                token_opt = token_rx.recv() => {
                    match token_opt {
                        Some(token) => {
                            accumulated.push_str(&token);
                            token_count += 1;
                            line_buffer.push_str(&token);

                            while let Some(newline_idx) = line_buffer.find('\n') {
                                let line = line_buffer.drain(..=newline_idx).collect::<String>();
                                let _ = event_tx.send(StreamEvent::Token {
                                    message_id: message_id_clone.clone(),
                                    delta: line,
                                }).await;
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        if !line_buffer.is_empty() {
            let _ = event_tx
                .send(StreamEvent::Token {
                    message_id: message_id_clone.clone(),
                    delta: line_buffer,
                })
                .await;
        }

        if cancelled {
            dbg!("session.stream_message.was_cancelled");
            dbg!("session.stream_message.accumulated_len", accumulated.len());
            if !accumulated.is_empty() {
                session_clone
                    .lock()
                    .await
                    .append(Role::Assistant, accumulated)
                    .ok();
            }
            return;
        }

        match llm_task.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                dbg!("session.stream_message.llm_err", err.to_string());
                let _ = event_tx
                    .send(StreamEvent::PendingCleared {
                        pending_id: pending_id.clone(),
                    })
                    .await;
                let _ = event_tx
                    .send(StreamEvent::Error {
                        message: err.to_string(),
                    })
                    .await;
                return;
            }
            Err(err) => {
                dbg!("session.stream_message.llm_join_err", err.to_string());
                let _ = event_tx
                    .send(StreamEvent::PendingCleared {
                        pending_id: pending_id.clone(),
                    })
                    .await;
                let _ = event_tx
                    .send(StreamEvent::Error {
                        message: format!("llm task failed: {}", err),
                    })
                    .await;
                return;
            }
        }

        dbg!("session.stream_message.tokens_received", token_count);
        dbg!("session.stream_message.accumulated_len", accumulated.len());

        let _ = event_tx
            .send(StreamEvent::StreamEnd {
                message_id: message_id_clone,
                content: accumulated.clone(),
            })
            .await;

        session_clone
            .lock()
            .await
            .append(Role::Assistant, accumulated)
            .ok();

        let _ = event_tx
            .send(StreamEvent::PendingCleared { pending_id })
            .await;
        dbg!("session.stream_message.done");
    });

    Ok((pending, cancel_tx, event_rx))
}
