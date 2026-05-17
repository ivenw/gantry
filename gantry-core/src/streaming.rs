use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use anyhow::Result;
use async_stream::stream;
use futures::Stream;
use rig::agent::{FinalResponse, MultiTurnStreamItem, StreamingError};
use rig::completion::Usage as RigUsage;
use rig::message::{
    AssistantContent, Reasoning, ReasoningContent, ToolCall, ToolFunction, UserContent,
};
use rig::one_or_many::OneOrMany;
use rig::streaming::{StreamedAssistantContent, StreamedUserContent};
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tokio::time::sleep;

use crate::app::App;
use crate::input::{InputToken, build_user_message};
use crate::message::Message;
use crate::metrics::CharCounts;
use crate::provider::agent::ChatStream;

/// Returned by [`stream_message`]. Holds the live stream and the deferred commit.
pub struct StreamingResponse {
    /// Yields streamed assistant content as it arrives.
    pub stream: ChatStream,
    commit_future: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
}

impl StreamingResponse {
    /// Drops the stream and persists the buffered assistant reply to the session.
    ///
    /// Must be called after the stream is consumed or abandoned. Safe to call after an
    /// interrupt — whatever was buffered up to that point will be persisted.
    pub async fn commit(self) {
        drop(self.stream);
        self.commit_future.await;
    }
}

/// Wraps a [`ChatStream`], accumulating structured assistant content and tool results into a
/// sequence of [`Message`]s. On drop — whether the stream was fully consumed or interrupted —
/// the accumulated messages are sent through a oneshot channel so the caller can persist them.
///
/// The accumulator mirrors rig's conversation structure: assistant content (text, reasoning,
/// tool calls) is collected into a single `Message::Assistant`. When a tool result arrives, the
/// in-progress assistant message is flushed and a `Message::User { ToolResult }` is appended,
/// then a fresh assistant message begins. This preserves the invariant that tool results are
/// recorded as user messages, which is required for replaying history to the agent correctly.
///
/// Reasoning chunks are concatenated into a single [`AssistantContent::Reasoning`] per
/// contiguous reasoning block. Partial content at interrupt time is flushed as-is — dangling
/// tool calls (no result yet) are acceptable and recoverable.
///
/// Usage from the [`FinalResponse`] is attached to the last message in the chain, which is
/// always an assistant message.
struct CommittingStream {
    inner: ChatStream,
    commit_tx: Option<oneshot::Sender<(Vec<Message>, Option<RigUsage>)>>,
    /// Completed messages (assistant + tool result pairs) from earlier in the turn.
    completed: Vec<Message>,
    /// Assistant content items being assembled for the current assistant message.
    current_assistant: Vec<AssistantContent>,
    usage: Option<RigUsage>,
}

impl CommittingStream {
    fn new(
        inner: ChatStream,
        commit_tx: oneshot::Sender<(Vec<Message>, Option<RigUsage>)>,
    ) -> Self {
        Self {
            inner,
            commit_tx: Some(commit_tx),
            completed: Vec::new(),
            current_assistant: Vec::new(),
            usage: None,
        }
    }

    /// Appends a text chunk to the current assistant message, merging into an existing
    /// trailing `Text` item if present to avoid redundant content items.
    fn push_text(&mut self, text: &str) {
        if let Some(AssistantContent::Text(t)) = self.current_assistant.last_mut() {
            t.text.push_str(text);
        } else {
            self.current_assistant.push(AssistantContent::text(text));
        }
    }

    /// Appends a reasoning chunk to the current assistant message, merging into an existing
    /// trailing `Reasoning` item if present.
    fn push_reasoning(&mut self, incoming: &Reasoning) {
        let text: String = incoming
            .content
            .iter()
            .filter_map(|c| {
                if let ReasoningContent::Text { text, .. } = c {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect();
        if text.is_empty() {
            return;
        }
        if let Some(AssistantContent::Reasoning(r)) = self.current_assistant.last_mut() {
            r.content.push(ReasoningContent::Text {
                text,
                signature: None,
            });
        } else {
            self.current_assistant
                .push(AssistantContent::Reasoning(Reasoning::new(&text)));
        }
    }

    /// Flushes the current in-progress assistant content into `completed` as a
    /// `Message::Assistant`, then appends the tool result as a `Message::User`.
    fn flush_assistant_and_push_tool_result(&mut self, result: rig::message::ToolResult) {
        if !self.current_assistant.is_empty() {
            let content = OneOrMany::many(std::mem::take(&mut self.current_assistant))
                .expect("current_assistant is non-empty");
            self.completed.push(Message::assistant_content(content));
        }
        self.completed.push(Message::User {
            sender: None,
            content: OneOrMany::one(UserContent::ToolResult(result)),
        });
    }

    /// Assembles and sends the final message sequence through the oneshot channel.
    ///
    /// Any remaining assistant content is flushed as the last message, and usage is
    /// attached to it — enforcing the invariant that usage always lands on the final
    /// assistant message.
    fn send_commit(&mut self) {
        let Some(tx) = self.commit_tx.take() else {
            return;
        };
        if !self.current_assistant.is_empty() {
            let content = OneOrMany::many(std::mem::take(&mut self.current_assistant))
                .expect("current_assistant is non-empty");
            self.completed.push(Message::assistant_content(content));
        }
        let messages = std::mem::take(&mut self.completed);
        let usage = self.usage.take();
        let _ = tx.send((messages, usage));
    }
}

impl Drop for CommittingStream {
    fn drop(&mut self) {
        self.send_commit();
    }
}

impl Stream for CommittingStream {
    type Item = Result<MultiTurnStreamItem<()>, StreamingError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(item)) => {
                match &item {
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Text(t),
                    )) => {
                        self.push_text(&t.text);
                    }
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Reasoning(r),
                    )) => {
                        let r = r.clone();
                        self.push_reasoning(&r);
                    }
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::ToolCall { tool_call, .. },
                    )) => {
                        self.current_assistant
                            .push(AssistantContent::ToolCall(tool_call.clone()));
                    }
                    Ok(MultiTurnStreamItem::StreamUserItem(StreamedUserContent::ToolResult {
                        tool_result,
                        ..
                    })) => {
                        let result = tool_result.clone();
                        self.flush_assistant_and_push_tool_result(result);
                    }
                    Ok(MultiTurnStreamItem::FinalResponse(f)) => {
                        self.usage = Some(f.usage());
                    }
                    _ => {}
                }
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                self.send_commit();
                Poll::Ready(None)
            }
        }
    }
}

/// Expands `tokens` into a user message, persists it, then streams the agent response.
///
/// Returns a [`StreamingResponse`]. The caller must call [`StreamingResponse::commit`] after
/// the stream is done or abandoned to persist the assistant reply and update token usage.
pub async fn stream_message(
    app: Arc<Mutex<App>>,
    tokens: Vec<InputToken>,
) -> Result<StreamingResponse> {
    let mut guard = app.lock().await;
    let message = build_user_message(tokens, &guard.project_path).await?;
    guard.append_message(message)?;
    let history: Vec<rig::message::Message> = guard.history().into_iter().map(Into::into).collect();
    guard.last_char_counts = Some(CharCounts::new(&guard.system_prompt, &history));
    let selection = guard
        .selection
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no active model selection"))?;
    let system_prompt = guard.system_prompt.to_string();
    let tools = guard.tools();
    let agent = guard
        .registry
        .agent(&selection, Some(&system_prompt), tools)?;
    let Some(prompt) = history.last().cloned() else {
        anyhow::bail!("no messages to stream");
    };
    let history = history[..history.len() - 1].to_vec();
    drop(guard);
    let (commit_tx, commit_rx) = oneshot::channel::<(Vec<Message>, Option<RigUsage>)>();
    let inner = agent.stream_chat(prompt, history).await;
    let stream = Box::pin(CommittingStream::new(inner, commit_tx));
    let commit_future = async move {
        let Ok((messages, usage)) = commit_rx.await else {
            return;
        };
        let mut guard = app.lock().await;
        guard.commit_response(messages, usage);
        let event = crate::events::AppEvent::MetricsUpdated {
            context_window: guard.context_window(),
            total_consumption: guard.total_consumption().clone(),
        };
        let _ = guard.event_sender().send(event);
    };
    Ok(StreamingResponse {
        stream,
        commit_future: Box::pin(commit_future),
    })
}

/// Produces a scripted streaming response for testing consumers of the streaming protocol.
///
/// Emits a sequence of reasoning tokens, text, tool calls with results, and a final text
/// turn, with realistic per-item delays to simulate token streaming and tool execution
/// latency. Useful for any client — TUI, RPC, or otherwise — that needs a predictable
/// stream without hitting a real model.
pub fn mock_stream_message(
    event_tx: tokio::sync::broadcast::Sender<crate::AppEvent>,
) -> StreamingResponse {
    let read_id = uuid::Uuid::new_v4().to_string();
    let edit_error_id = uuid::Uuid::new_v4().to_string();
    let edit_id = uuid::Uuid::new_v4().to_string();
    let bash_id = uuid::Uuid::new_v4().to_string();
    let write_id = uuid::Uuid::new_v4().to_string();

    // Token delay — simulates the inter-token gap during LLM streaming.
    let token_ms = Duration::from_millis(200);
    // Slightly longer gap between reasoning chunks to feel deliberate.
    let reasoning_ms = Duration::from_millis(200);

    let reasoning_chunks: &[&str] = &[
        "The user wants me to look at the codebase. ",
        "I should start by reading the main entry point ",
        "to understand the overall structure.\n",
        "\n",
        "Once I have a picture of the module layout ",
        "I can identify the specific file the user mentioned.",
    ];

    let text_chunks: &[&str] = &[
        "Sure, ",
        "I'll take a look at the codebase for you.\n",
        "Let me start by reading `src/main.rs` ",
        "to understand the entry point, ",
        "then I'll follow the module tree from there.\n",
        "\n",
        "This is the second paragraph. ",
        "It should only appear after the double newline above ",
        "has been received by the renderer.\n",
    ];

    let edit_reasoning_chunks: &[&str] = &[
        "The main function just prints hello. ",
        "I should update it to print a proper greeting ",
        "with the program name included.",
    ];

    let edit_text_chunks: &[&str] = &[
        "The entry point is simple. ",
        "I'll update the greeting to be more descriptive.\n",
    ];

    let second_reasoning_chunks: &[&str] = &[
        "The edit looks good. ",
        "Now I should run the tests to verify correctness.",
    ];

    let pre_bash_text_chunks: &[&str] = &[
        "Now let me run the tests ",
        "to make sure nothing is broken.\n",
    ];

    let pre_write_text_chunks: &[&str] = &["Let me write the updated config file.\n"];

    let final_text_chunks: &[&str] = &[
        "All tests pass. ",
        "The implementation looks correct.\n",
        "Anything else I can help with?",
    ];

    // Clone all string data up-front so the stream owns it.
    let reasoning_chunks: Vec<String> = reasoning_chunks.iter().map(|s| s.to_string()).collect();
    let text_chunks: Vec<String> = text_chunks.iter().map(|s| s.to_string()).collect();
    let edit_reasoning_chunks: Vec<String> = edit_reasoning_chunks
        .iter()
        .map(|s| s.to_string())
        .collect();
    let edit_text_chunks: Vec<String> = edit_text_chunks.iter().map(|s| s.to_string()).collect();
    let second_reasoning_chunks: Vec<String> = second_reasoning_chunks
        .iter()
        .map(|s| s.to_string())
        .collect();
    let pre_bash_text_chunks: Vec<String> =
        pre_bash_text_chunks.iter().map(|s| s.to_string()).collect();
    let pre_write_text_chunks: Vec<String> = pre_write_text_chunks
        .iter()
        .map(|s| s.to_string())
        .collect();
    let final_text_chunks: Vec<String> = final_text_chunks.iter().map(|s| s.to_string()).collect();

    let chat_stream: ChatStream = Box::pin(stream! {
        for chunk in &reasoning_chunks {
            sleep(reasoning_ms).await;
            yield Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::Reasoning(Reasoning::new(chunk.as_str())),
            ));
        }

        for chunk in &text_chunks {
            sleep(token_ms).await;
            yield Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::Text(rig::message::Text { text: chunk.clone() }),
            ));
        }

        yield Ok(MultiTurnStreamItem::StreamAssistantItem(
            StreamedAssistantContent::ToolCall {
                tool_call: ToolCall::new(
                    read_id.clone(),
                    ToolFunction::new(
                        "read_file".to_string(),
                        serde_json::json!({ "path": "src/main.rs", "offset": 50, "limit": 100 }),
                    ),
                ),
                internal_call_id: read_id.clone(),
            },
        ));

        // read: fast tool, ~100ms
        sleep(Duration::from_millis(100)).await;
        yield Ok(MultiTurnStreamItem::StreamUserItem(
            StreamedUserContent::tool_result(
                rig::message::ToolResult {
                    id: read_id.clone(),
                    call_id: None,
                    content: rig::OneOrMany::one(rig::message::ToolResultContent::text(
                        "fn main() { println!(\"hello\"); }",
                    )),
                },
                read_id,
            ),
        ));


        for chunk in &edit_reasoning_chunks {
            sleep(reasoning_ms).await;
            yield Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::Reasoning(Reasoning::new(chunk.as_str())),
            ));
        }

        for chunk in &edit_text_chunks {
            sleep(token_ms).await;
            yield Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::Text(rig::message::Text { text: chunk.clone() }),
            ));
        }

        yield Ok(MultiTurnStreamItem::StreamAssistantItem(
            StreamedAssistantContent::ToolCall {
                tool_call: ToolCall::new(
                    edit_error_id.clone(),
                    ToolFunction::new(
                        "edit_file".to_string(),
                        serde_json::json!({
                            "path": "src/main.rs",
                            "ops": [{ "start": "1#xx", "end": "1#xx", "content": "irrelevant" }]
                        }),
                    ),
                ),
                internal_call_id: edit_error_id.clone(),
            },
        ));

        sleep(Duration::from_millis(100)).await;
        yield Ok(MultiTurnStreamItem::StreamUserItem(
            StreamedUserContent::tool_result(
                rig::message::ToolResult {
                    id: edit_error_id.clone(),
                    call_id: None,
                    content: rig::OneOrMany::one(rig::message::ToolResultContent::text(
                        format!("{}stale line references:\nline 1 is stale: expected hash 'ab', got 'zz'", crate::tools::TOOL_ERROR_PREFIX),
                    )),
                },
                edit_error_id,
            ),
        ));

        sleep(Duration::from_millis(400)).await;

        let edit_path = "src/main.rs";
        yield Ok(MultiTurnStreamItem::StreamAssistantItem(
            StreamedAssistantContent::ToolCall {
                tool_call: ToolCall::new(
                    edit_id.clone(),
                    ToolFunction::new(
                        "edit_file".to_string(),
                        serde_json::json!({
                            "path": edit_path,
                            "ops": [
                                { "start": "1#ab", "end": "1#ab", "content": "fn main() {\n    println!(\"gantry: hello from main\");\n}" }
                            ]
                        }),
                    ),
                ),
                internal_call_id: edit_id.clone(),
            },
        ));

        // edit: medium tool, ~800ms
        sleep(Duration::from_millis(800)).await;
        let _ = event_tx.send(crate::AppEvent::EditDiff {
            path: std::path::PathBuf::from(edit_path),
            hunks: vec![
                gantry_tools::DiffHunk {
                    old_start: 1,
                    new_start: 1,
                    old_lines: vec!["fn main() { println!(\"hello\"); }".to_string()],
                    new_lines: vec![
                        "fn main() {".to_string(),
                        "    println!(\"gantry: hello from main\");".to_string(),
                        "}".to_string(),
                    ],
                },
            ],
        });
        yield Ok(MultiTurnStreamItem::StreamUserItem(
            StreamedUserContent::tool_result(
                rig::message::ToolResult {
                    id: edit_id.clone(),
                    call_id: None,
                    content: rig::OneOrMany::one(rig::message::ToolResultContent::text(
                        "applied 1 edit(s) to src/main.rs",
                    )),
                },
                edit_id,
            ),
        ));

        for chunk in &second_reasoning_chunks {
            sleep(reasoning_ms).await;
            yield Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::Reasoning(Reasoning::new(chunk.as_str())),
            ));
        }

        for chunk in &pre_bash_text_chunks {
            sleep(token_ms).await;
            yield Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::Text(rig::message::Text { text: chunk.clone() }),
            ));
        }

        yield Ok(MultiTurnStreamItem::StreamAssistantItem(
            StreamedAssistantContent::ToolCall {
                tool_call: ToolCall::new(
                    bash_id.clone(),
                    ToolFunction::new(
                        "bash".to_string(),
                        serde_json::json!({ "command": "cargo check&&cargo clippy &&cargo test&& echo '&&'\\&\\& \\&" }),
                    ),
                ),
                internal_call_id: bash_id.clone(),
            },
        ));

        // bash: slow tool, ~3s
        sleep(Duration::from_millis(3000)).await;
        yield Ok(MultiTurnStreamItem::StreamUserItem(
            StreamedUserContent::tool_result(
                rig::message::ToolResult {
                    id: bash_id.clone(),
                    call_id: None,
                    content: rig::OneOrMany::one(rig::message::ToolResultContent::text(
                        "test result: ok. 3 passed.",
                    )),
                },
                bash_id,
            ),
        ));

        for chunk in &pre_write_text_chunks {
            sleep(token_ms).await;
            yield Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::Text(rig::message::Text { text: chunk.clone() }),
            ));
        }

        yield Ok(MultiTurnStreamItem::StreamAssistantItem(
            StreamedAssistantContent::ToolCall {
                tool_call: ToolCall::new(
                    write_id.clone(),
                    ToolFunction::new(
                        "write_file".to_string(),
                        serde_json::json!({
                            "path": "config/settings.toml",
                            "content": "[server]\nhost = \"localhost\"\nport = 8080\n\n[database]\nurl = \"postgres://localhost/app\"\nmax_connections = 10\npool_timeout = 30\n"
                        }),
                    ),
                ),
                internal_call_id: write_id.clone(),
            },
        ));

        // write: fast tool, ~200ms
        sleep(Duration::from_millis(200)).await;
        yield Ok(MultiTurnStreamItem::StreamUserItem(
            StreamedUserContent::tool_result(
                rig::message::ToolResult {
                    id: write_id.clone(),
                    call_id: None,
                    content: rig::OneOrMany::one(rig::message::ToolResultContent::text(
                        "File written successfully.",
                    )),
                },
                write_id,
            ),
        ));

        for chunk in &final_text_chunks {
            sleep(token_ms).await;
            yield Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::Text(rig::message::Text { text: chunk.clone() }),
            ));
        }

        yield Ok(MultiTurnStreamItem::FinalResponse(FinalResponse::empty()));
    });

    // Use a no-op commit: nothing is persisted to session history.
    let (commit_tx, _commit_rx) = oneshot::channel::<(Vec<Message>, Option<RigUsage>)>();
    let committing = Box::pin(CommittingStream::new(chat_stream, commit_tx));
    let commit_future = async move {};

    StreamingResponse {
        stream: committing,
        commit_future: Box::pin(commit_future),
    }
}
