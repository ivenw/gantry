use crate::chat::events::{
    AppEvent, ErrorEvent, MessageReceivedEvent, PendingClearedEvent, StreamEndEvent,
    StreamMessageRequest, StreamStartEvent, TokenEvent,
};
use crate::chat::{Message, PendingMessage, Role};
use crate::event_bus::EventBus;
use crate::project::resource_loader::discover_agents_md;
use crate::project::system_prompt::build_system_prompt;
use crate::provider::agent_factory::RigAgentFactory;
use crate::provider::ModelSelection;
use crate::session::Session;
use anyhow::Result;
use rig::message::Message as RigMessage;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, oneshot};
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct StreamingGuard {
    pub is_streaming: Arc<AtomicBool>,
}

impl Drop for StreamingGuard {
    fn drop(&mut self) {
        self.is_streaming.store(false, Ordering::SeqCst);
    }
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

pub(crate) async fn clear_pending(
    pending_id: &str,
    pending_message: &Arc<Mutex<Option<PendingMessage>>>,
    event_bus: &EventBus,
) {
    dbg!("session.clear_pending", pending_id);
    *pending_message.lock().await = None;
    event_bus.publish(AppEvent::PendingCleared(PendingClearedEvent {
        pending_id: pending_id.to_string(),
    }));
}

pub(crate) async fn stream_message(
    req: StreamMessageRequest,
    project_path: &Path,
    session: &Arc<Mutex<Session>>,
    pending_message: &Arc<Mutex<Option<PendingMessage>>>,
    active_selection: &Arc<Mutex<ModelSelection>>,
    event_bus: &EventBus,
    agent_factory: &RigAgentFactory,
    is_streaming: &Arc<AtomicBool>,
    cancel_tx: &Arc<Mutex<Option<oneshot::Sender<()>>>>,
) -> Result<PendingMessage> {
    dbg!("session.stream_message.request", &req.content);
    if is_streaming
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(anyhow::anyhow!("a stream is already in progress"));
    }
    let _streaming_guard = StreamingGuard {
        is_streaming: is_streaming.clone(),
    };

    let pending = PendingMessage::new(req.content.clone());

    {
        let mut sess = session.lock().await;
        sess.append(Role::User, req.content)
            .unwrap_or_else(|_| panic!("failed to persist message"));
        *pending_message.lock().await = Some(pending.clone());
    }

    event_bus.publish(AppEvent::MessageReceived(MessageReceivedEvent {
        id: pending.id.clone(),
        content: pending.content.clone(),
    }));
    dbg!("session.stream_message.pending_published", &pending.id);

    let snapshot = session.lock().await.context_messages();
    let selection = active_selection.lock().await.clone();
    let system_prompt = build_system_prompt(&discover_agents_md(project_path));
    let mut rig_messages = to_rig_messages(snapshot);
    let Some(prompt) = rig_messages.pop() else {
        clear_pending(&pending.id, pending_message, event_bus).await;
        event_bus.publish(AppEvent::Error(ErrorEvent {
            message: "cannot generate tokens with empty message history".to_string(),
        }));
        return Ok(pending);
    };

    dbg!(
        "session.stream_message.snapshot_len",
        rig_messages.len() + 1
    );
    let message_id = Uuid::new_v4().to_string();
    event_bus.publish(AppEvent::StreamStart(StreamStartEvent {
        message_id: message_id.clone(),
        pending_of: pending.id.clone(),
    }));

    let (token_tx, mut token_rx) = mpsc::channel(128);
    let (cancel_tx_inner, mut cancel_rx) = oneshot::channel();
    *cancel_tx.lock().await = Some(cancel_tx_inner);

    let agent = agent_factory.agent(&selection, Some(&system_prompt)).await;
    let llm_task = tokio::spawn(async move {
        let agent = agent?;
        agent.stream_chat(prompt, rig_messages, token_tx).await
    });

    let mut accumulated = String::new();
    let mut token_count = 0usize;
    let mut cancelled = false;
    let mut line_buffer = String::new();
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
                            event_bus.publish(AppEvent::Token(TokenEvent {
                                message_id: message_id.clone(),
                                delta: line,
                            }));
                        }
                    }
                    None => break,
                }
            }
        }
    }

    if !line_buffer.is_empty() {
        event_bus.publish(AppEvent::Token(TokenEvent {
            message_id: message_id.clone(),
            delta: line_buffer,
        }));
    }

    if cancelled {
        dbg!("session.stream_message.was_cancelled");
        dbg!("session.stream_message.accumulated_len", accumulated.len());
        if !accumulated.is_empty() {
            session
                .lock()
                .await
                .append(Role::Assistant, accumulated)
                .ok();
        }
        is_streaming.store(false, Ordering::SeqCst);
        return Ok(pending);
    }

    match llm_task.await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            dbg!("session.stream_message.llm_err", err.to_string());
            clear_pending(&pending.id, pending_message, event_bus).await;
            event_bus.publish(AppEvent::Error(ErrorEvent {
                message: err.to_string(),
            }));
            return Ok(pending);
        }
        Err(err) => {
            dbg!("session.stream_message.llm_join_err", err.to_string());
            clear_pending(&pending.id, pending_message, event_bus).await;
            event_bus.publish(AppEvent::Error(ErrorEvent {
                message: format!("llm task failed: {}", err),
            }));
            return Ok(pending);
        }
    }

    dbg!("session.stream_message.tokens_received", token_count);
    dbg!("session.stream_message.accumulated_len", accumulated.len());

    event_bus.publish(AppEvent::StreamEnd(StreamEndEvent {
        message_id,
        content: accumulated.clone(),
    }));
    dbg!("session.stream_message.end_published");

    {
        session
            .lock()
            .await
            .append(Role::Assistant, accumulated)
            .ok();
    }

    clear_pending(&pending.id, pending_message, event_bus).await;
    dbg!("session.stream_message.done", &pending.id);
    Ok(pending)
}

pub(crate) async fn interrupt_stream(
    message_id: String,
    pending_message: &Arc<Mutex<Option<PendingMessage>>>,
    event_bus: &EventBus,
    is_streaming: &Arc<AtomicBool>,
    cancel_tx: &Arc<Mutex<Option<oneshot::Sender<()>>>>,
) -> bool {
    dbg!("session.interrupt_stream", &message_id);

    if let Some(tx) = cancel_tx.lock().await.take() {
        let _ = tx.send(());
        dbg!("session.interrupt_stream.sent_cancel");
    }

    let pending = pending_message.lock().await.clone();

    if let Some(pending) = pending {
        dbg!("session.interrupt_stream.clearing_pending");

        event_bus.publish(AppEvent::StreamEnd(StreamEndEvent {
            message_id: message_id.clone(),
            content: String::new(),
        }));

        clear_pending(&pending.id, pending_message, event_bus).await;
    }

    is_streaming.store(false, Ordering::SeqCst);
    dbg!("session.interrupt_stream.done");
    true
}
