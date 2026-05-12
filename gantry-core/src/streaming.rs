use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use anyhow::Result;
use async_stream::stream;
use futures::Stream;
use rig::agent::{FinalResponse, MultiTurnStreamItem, StreamingError};
use rig::completion::Usage as RigUsage;
use rig::message::{Reasoning, ToolCall, ToolFunction};
use rig::streaming::{StreamedAssistantContent, StreamedUserContent};
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tokio::time::sleep;

use crate::app::App;
use crate::input::{InputToken, build_user_message};
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

/// Produces a fake streaming response for exercising the TUI rendering pipeline.
///
/// Emits a scripted sequence of reasoning tokens, text, two tool calls with their
/// results, and a final text turn, with realistic per-item delays to simulate token
/// streaming and tool execution latency.
pub fn mock_stream_message() -> StreamingResponse {
    let read_id = uuid::Uuid::new_v4().to_string();
    let edit_error_id = uuid::Uuid::new_v4().to_string();
    let edit_id = uuid::Uuid::new_v4().to_string();
    let bash_id = uuid::Uuid::new_v4().to_string();

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
                        "read".to_string(),
                        serde_json::json!({ "path": "src/main.rs" }),
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

        yield Ok(MultiTurnStreamItem::StreamAssistantItem(
            StreamedAssistantContent::ToolCall {
                tool_call: ToolCall::new(
                    edit_error_id.clone(),
                    ToolFunction::new(
                        "edit".to_string(),
                        serde_json::json!({
                            "path": "src/main.rs",
                            "old_string": "this string does not exist",
                            "new_string": "irrelevant"
                        }),
                    ),
                ),
                internal_call_id: edit_error_id.clone(),
            },
        ));

        sleep(Duration::from_millis(400)).await;
        yield Ok(MultiTurnStreamItem::StreamUserItem(
            StreamedUserContent::tool_result(
                rig::message::ToolResult {
                    id: edit_error_id.clone(),
                    call_id: None,
                    content: rig::OneOrMany::one(rig::message::ToolResultContent::text(
                        &format!("{}stale line references:\nline 1 is stale: expected hash 'ab', got 'zz'", crate::tools::TOOL_ERROR_PREFIX),
                    )),
                },
                edit_error_id,
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
                    edit_id.clone(),
                    ToolFunction::new(
                        "edit".to_string(),
                        serde_json::json!({
                            "path": "src/main.rs",
                            "old_string": "fn main() { println!(\"hello\"); }",
                            "new_string": "fn main() {\n    println!(\"gantry: hello from main\");\n}"
                        }),
                    ),
                ),
                internal_call_id: edit_id.clone(),
            },
        ));

        // edit: medium tool, ~800ms
        sleep(Duration::from_millis(800)).await;
        yield Ok(MultiTurnStreamItem::StreamUserItem(
            StreamedUserContent::tool_result(
                rig::message::ToolResult {
                    id: edit_id.clone(),
                    call_id: None,
                    content: rig::OneOrMany::one(rig::message::ToolResultContent::text(
                        "Edit applied successfully.",
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
                        serde_json::json!({ "command": "cargo test" }),
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

        for chunk in &final_text_chunks {
            sleep(token_ms).await;
            yield Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::Text(rig::message::Text { text: chunk.clone() }),
            ));
        }

        yield Ok(MultiTurnStreamItem::FinalResponse(FinalResponse::empty()));
    });

    // Use a no-op commit: nothing is persisted to session history.
    let (commit_tx, _commit_rx) = oneshot::channel::<(String, Option<RigUsage>)>();
    let buffering = Box::pin(BufferingStream::new(chat_stream, commit_tx));
    let commit_future = async move {};

    StreamingResponse {
        stream: buffering,
        commit_future: Box::pin(commit_future),
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
    let (commit_tx, commit_rx) = oneshot::channel::<(String, Option<RigUsage>)>();
    let inner = agent.stream_chat(prompt, history).await;
    let stream = Box::pin(BufferingStream::new(inner, commit_tx));
    let commit_future = async move {
        let Ok((text, usage)) = commit_rx.await else {
            return;
        };
        app.lock().await.commit_response(text, usage);
    };
    Ok(StreamingResponse {
        stream,
        commit_future: Box::pin(commit_future),
    })
}

/// Wraps a [`ChatStream`], accumulating streamed text and usage. On drop (whether the stream
/// was fully consumed or interrupted), sends the buffer through a oneshot channel so the
/// caller's commit future can persist the assistant reply.
struct BufferingStream {
    inner: ChatStream,
    commit_tx: Option<oneshot::Sender<(String, Option<RigUsage>)>>,
    buffer: String,
    usage: Option<RigUsage>,
}

impl BufferingStream {
    fn new(inner: ChatStream, commit_tx: oneshot::Sender<(String, Option<RigUsage>)>) -> Self {
        Self {
            inner,
            commit_tx: Some(commit_tx),
            buffer: String::new(),
            usage: None,
        }
    }

    fn send_commit(&mut self) {
        if let Some(tx) = self.commit_tx.take() {
            let text = std::mem::take(&mut self.buffer);
            let usage = self.usage.take();
            let _ = tx.send((text, usage));
        }
    }
}

impl Drop for BufferingStream {
    fn drop(&mut self) {
        self.send_commit();
    }
}

impl Stream for BufferingStream {
    type Item = Result<MultiTurnStreamItem<()>, StreamingError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(item)) => {
                match &item {
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Text(t),
                    )) => {
                        self.buffer.push_str(&t.text);
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
