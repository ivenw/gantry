use crate::chat::events::StreamMessageRequest;
use crate::chat::system_prompt::build_system_prompt;
use crate::project::resource_loader::discover_agents_md;
use crate::provider::agent_factory::RigAgentFactory;
use crate::session::SessionHandle;
use anyhow::Result;
use rig::message::Message;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum StreamEvent {
    MessageReceived {
        content: String,
        pending_id: String,
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
    ToolCallStarted {
        tool_call_id: String,
        tool_name: String,
    },
    ToolResultReceived {
        tool_call_id: String,
        tool_name: String,
        content: String,
    },
    Error {
        message: String,
    },
}

pub(crate) async fn stream_message_with_factory(
    req: StreamMessageRequest,
    handle: Arc<SessionHandle>,
    agent_factory: &RigAgentFactory,
) -> Result<(String, oneshot::Sender<()>, mpsc::Receiver<StreamEvent>)> {
    dbg!("session.stream_message.request", &req.content);

    let pending_id = Uuid::new_v4().to_string();
    let pending_content = req.content.clone();

    handle
        .append_message(Message::user(req.content))
        .await
        .unwrap_or_else(|_| panic!("failed to persist message"));

    let (mut rig_messages, selection) = handle.snapshot().await;
    let system_prompt = build_system_prompt(&discover_agents_md(&handle.project_path));

    let (event_tx, event_rx) = mpsc::channel(256);
    let (cancel_tx, cancel_rx) = oneshot::channel();

    let _ = event_tx
        .send(StreamEvent::MessageReceived {
            content: pending_content,
            pending_id: pending_id.clone(),
        })
        .await;

    let Some(prompt) = rig_messages.pop() else {
        let _ = event_tx
            .send(StreamEvent::PendingCleared {
                pending_id: pending_id.clone(),
            })
            .await;
        let _ = event_tx
            .send(StreamEvent::Error {
                message: "cannot generate tokens with empty message history".to_string(),
            })
            .await;
        return Ok((pending_id, cancel_tx, event_rx));
    };

    dbg!(
        "session.stream_message.snapshot_len",
        rig_messages.len() + 1
    );
    let message_id = Uuid::new_v4().to_string();

    let _ = event_tx
        .send(StreamEvent::StreamStart {
            message_id: message_id.clone(),
            pending_id: pending_id.clone(),
        })
        .await;

    let (stream_event_tx, mut stream_event_rx) = mpsc::channel(128);
    let agent = agent_factory.agent(&selection, Some(&system_prompt)).await;
    let llm_task = tokio::spawn(async move {
        let agent = agent?;
        agent
            .stream_chat(prompt, rig_messages, stream_event_tx)
            .await
    });

    let handle_clone = handle.clone();
    let message_id_clone = message_id.clone();
    let pending_id_clone = pending_id.clone();

    tokio::spawn(async move {
        let pending_id = pending_id_clone;
        let mut accumulated = String::new();
        let mut token_count = 0usize;
        let mut cancelled = false;
        let mut line_buffer = String::new();
        let mut cancel_rx = cancel_rx;
        let mut tool_turns: Vec<Message> = Vec::new();

        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    dbg!("session.stream_message.cancelled");
                    cancelled = true;
                    break;
                }
                ev_opt = stream_event_rx.recv() => {
                    match ev_opt {
                        Some(crate::provider::agent_factory::AgentStreamEvent::Token(token)) => {
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
                        Some(crate::provider::agent_factory::AgentStreamEvent::ToolCallStarted { tool_call_id, tool_name }) => {
                            let tc = rig::message::ToolCall {
                                id: tool_call_id.clone(),
                                call_id: None,
                                function: rig::message::ToolFunction {
                                    name: tool_name.clone(),
                                    arguments: serde_json::Value::Null,
                                },
                                signature: None,
                                additional_params: None,
                            };
                            tool_turns.push(Message::Assistant {
                                id: None,
                                content: rig::one_or_many::OneOrMany::one(
                                    rig::message::AssistantContent::ToolCall(tc),
                                ),
                            });
                            let _ = event_tx.send(StreamEvent::ToolCallStarted {
                                tool_call_id,
                                tool_name,
                            }).await;
                        }
                        Some(crate::provider::agent_factory::AgentStreamEvent::ToolResultReceived { tool_call_id, tool_name, content }) => {
                            let tr = rig::message::ToolResult {
                                id: tool_name.clone(),
                                call_id: Some(tool_call_id.clone()),
                                content: rig::one_or_many::OneOrMany::one(
                                    rig::message::ToolResultContent::Text(rig::message::Text {
                                        text: content.clone(),
                                    }),
                                ),
                            };
                            tool_turns.push(Message::User {
                                content: rig::one_or_many::OneOrMany::one(
                                    rig::message::UserContent::ToolResult(tr),
                                ),
                            });
                            let _ = event_tx.send(StreamEvent::ToolResultReceived {
                                tool_call_id,
                                tool_name,
                                content,
                            }).await;
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
                handle_clone
                    .append_message(Message::assistant(accumulated))
                    .await
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

        for turn in tool_turns {
            handle_clone.append_message(turn).await.ok();
        }
        handle_clone
            .append_message(Message::assistant(accumulated))
            .await
            .ok();

        let _ = event_tx
            .send(StreamEvent::PendingCleared { pending_id })
            .await;
        dbg!("session.stream_message.done");
    });

    Ok((pending_id, cancel_tx, event_rx))
}
