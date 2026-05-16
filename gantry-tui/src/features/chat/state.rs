use gantry_core::{DiffHunk, UserId};

pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub streaming_content: Option<String>,
    pub streaming_message_idx: Option<usize>,
    /// False until the first content is appended — delays the assistant message from appearing.
    pub streaming_message_pushed: bool,
    /// Byte offset into `streaming_content` up to which content has been synced to `messages`.
    /// Tokens are only flushed to the visible message on paragraph boundaries (`\n\n`).
    streaming_rendered_len: usize,
    pub streaming_reasoning_content: Option<String>,
    pub streaming_reasoning_message_idx: Option<usize>,
    /// False until the first reasoning content is appended.
    pub streaming_reasoning_pushed: bool,
    /// Byte offset into `streaming_reasoning_content` up to which content has been synced to `messages`.
    streaming_reasoning_rendered_len: usize,
    /// Number of lines scrolled up from the bottom (0 = pinned to bottom).
    pub scroll_offset: u16,
    /// True while the user has manually scrolled up; suppresses auto-scroll-to-bottom.
    pub user_is_scrolling: bool,
}

impl ChatState {
    /// Creates a new empty `ChatState`.
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            streaming_content: None,
            streaming_message_idx: None,
            streaming_message_pushed: false,
            streaming_rendered_len: 0,
            streaming_reasoning_content: None,
            streaming_reasoning_message_idx: None,
            streaming_reasoning_pushed: false,
            streaming_reasoning_rendered_len: 0,
            scroll_offset: 0,
            user_is_scrolling: false,
        }
    }

    /// Ensures any accumulated streaming text is committed to `messages` before an
    /// interleaved event (tool call or new streaming turn) modifies the list.
    fn flush_streaming(&mut self) {
        self.flush_reasoning_rendered();
        self.flush_rendered();
    }

    /// Inserts a tool call row with `done: false`, flushing any pending assistant text first.
    pub fn push_tool_call(&mut self, id: String, name: String, arguments: serde_json::Value) {
        self.flush_streaming();
        self.messages.push(ChatMessage::ToolCall {
            id,
            name,
            arguments,
            done: false,
            is_error: false,
            hunks: vec![],
        });
    }

    /// Attaches diff hunks to the most recent edit tool call whose path argument matches.
    pub fn attach_edit_diff(&mut self, path: &std::path::Path, hunks: Vec<DiffHunk>) {
        for msg in self.messages.iter_mut().rev() {
            if let ChatMessage::ToolCall {
                name,
                arguments,
                hunks: msg_hunks,
                ..
            } = msg
                && name == "edit_file"
                && arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .map(std::path::Path::new)
                    .as_deref()
                    == Some(path)
                && msg_hunks.is_empty()
            {
                *msg_hunks = hunks;
                break;
            }
        }
    }

    /// Marks the tool call with `id` as done, recording whether it produced an error.
    pub fn finish_tool_call(&mut self, id: &str, is_error: bool) {
        for msg in &mut self.messages {
            if let ChatMessage::ToolCall {
                id: msg_id,
                done,
                is_error: msg_is_error,
                ..
            } = msg
                && msg_id == id
            {
                *done = true;
                *msg_is_error = is_error;
                break;
            }
        }
    }

    /// Adds a user message with no sender (single-user session).
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(ChatMessage::User {
            sender: None,
            content,
        });
    }

    /// Begins a new assistant streaming slot. Flushes any buffered text from the previous
    /// slot first so that text arriving before a tool result is not discarded.
    pub fn start_streaming_message(&mut self) {
        self.flush_streaming();
        self.streaming_reasoning_content = Some(String::new());
        self.streaming_reasoning_message_idx = None;
        self.streaming_reasoning_pushed = false;
        self.streaming_reasoning_rendered_len = 0;
        self.streaming_content = Some(String::new());
        self.streaming_message_idx = None;
        self.streaming_message_pushed = false;
        self.streaming_rendered_len = 0;
    }

    /// Appends `content` to the current reasoning turn, syncing to the visible message only
    /// at paragraph boundaries (`\n\n`). The message slot is not pushed until the first flush.
    pub fn append_to_reasoning(&mut self, content: &str) {
        let Some(ref mut streaming) = self.streaming_reasoning_content else {
            return;
        };
        streaming.push_str(content);

        let unrendered = &streaming[self.streaming_reasoning_rendered_len..];
        let flush_end = match unrendered.rfind("\n\n") {
            Some(pos) => self.streaming_reasoning_rendered_len + pos + 2,
            None => return,
        };

        if flush_end <= self.streaming_reasoning_rendered_len {
            return;
        }

        let pending = streaming[self.streaming_reasoning_rendered_len..flush_end].to_owned();
        self.streaming_reasoning_rendered_len = flush_end;

        if !self.streaming_reasoning_pushed {
            self.messages
                .push(ChatMessage::Reasoning { content: pending });
            self.streaming_reasoning_message_idx = Some(self.messages.len() - 1);
            self.streaming_reasoning_pushed = true;
        } else if let Some(idx) = self.streaming_reasoning_message_idx
            && let Some(ChatMessage::Reasoning {
                content: msg_content,
            }) = self.messages.get_mut(idx)
        {
            msg_content.push_str(&pending);
        }
    }

    /// Syncs all buffered reasoning content to the visible message entry.
    fn flush_reasoning_rendered(&mut self) {
        let Some(ref streaming) = self.streaming_reasoning_content else {
            return;
        };
        if self.streaming_reasoning_rendered_len >= streaming.len() {
            return;
        }
        let pending = streaming[self.streaming_reasoning_rendered_len..].to_owned();
        self.streaming_reasoning_rendered_len = streaming.len();

        if !self.streaming_reasoning_pushed {
            self.messages
                .push(ChatMessage::Reasoning { content: pending });
            self.streaming_reasoning_message_idx = Some(self.messages.len() - 1);
            self.streaming_reasoning_pushed = true;
        } else if let Some(idx) = self.streaming_reasoning_message_idx
            && let Some(ChatMessage::Reasoning {
                content: msg_content,
            }) = self.messages.get_mut(idx)
        {
            msg_content.push_str(&pending);
        }
    }

    /// Appends `content` to the current streaming turn, syncing to the visible message only
    /// at paragraph boundaries (`\n\n`) to reduce render churn. The message slot is not pushed
    /// until the first flush so that an empty `<<` never appears.
    pub fn append_to_streaming(&mut self, content: &str) {
        let Some(ref mut streaming) = self.streaming_content else {
            return;
        };
        streaming.push_str(content);

        // Flush up to the end of the last complete paragraph. Hold back content that hasn't
        // reached a paragraph boundary yet — it will be flushed by finish_streaming.
        let unrendered = &streaming[self.streaming_rendered_len..];
        let flush_end = match unrendered.rfind("\n\n") {
            Some(pos) => self.streaming_rendered_len + pos + 2,
            None => return,
        };

        if flush_end <= self.streaming_rendered_len {
            return;
        }

        let pending = streaming[self.streaming_rendered_len..flush_end].to_owned();
        self.streaming_rendered_len = flush_end;

        if !self.streaming_message_pushed {
            self.messages
                .push(ChatMessage::Assistant { content: pending });
            self.streaming_message_idx = Some(self.messages.len() - 1);
            self.streaming_message_pushed = true;
        } else if let Some(idx) = self.streaming_message_idx
            && let Some(ChatMessage::Assistant {
                content: msg_content,
            }) = self.messages.get_mut(idx)
        {
            msg_content.push_str(&pending);
        }
    }

    /// Syncs all buffered streaming content to the visible message entry, pushing the message
    /// slot if it hasn't been created yet.
    fn flush_rendered(&mut self) {
        let Some(ref streaming) = self.streaming_content else {
            return;
        };
        if self.streaming_rendered_len >= streaming.len() {
            return;
        }
        let pending = streaming[self.streaming_rendered_len..].to_owned();
        self.streaming_rendered_len = streaming.len();

        if !self.streaming_message_pushed {
            self.messages
                .push(ChatMessage::Assistant { content: pending });
            self.streaming_message_idx = Some(self.messages.len() - 1);
            self.streaming_message_pushed = true;
        } else if let Some(idx) = self.streaming_message_idx
            && let Some(ChatMessage::Assistant {
                content: msg_content,
            }) = self.messages.get_mut(idx)
        {
            msg_content.push_str(&pending);
        }
    }

    /// Interrupts an in-progress stream, flushing any buffered content to the visible
    /// message so it remains readable. Unlike `cancel_streaming`, no messages are removed.
    pub fn interrupt_streaming(&mut self) {
        self.flush_reasoning_rendered();
        self.streaming_reasoning_content = None;
        self.streaming_reasoning_message_idx = None;
        self.streaming_reasoning_pushed = false;
        self.streaming_reasoning_rendered_len = 0;
        self.flush_rendered();
        self.streaming_content = None;
        self.streaming_message_idx = None;
        self.streaming_message_pushed = false;
        self.streaming_rendered_len = 0;
    }

    /// Cancels an in-progress stream, rolling back the optimistic user message and any
    /// partial assistant content. Returns the rolled-back user message text so the caller
    /// can restore it to the input.
    pub fn cancel_streaming(&mut self) -> Option<String> {
        // Remove any partial assistant message that was pushed during streaming.
        if self.streaming_message_pushed
            && let Some(idx) = self.streaming_message_idx
            && idx < self.messages.len()
        {
            self.messages.remove(idx);
        }
        // Remove the optimistic user message that was added just before streaming started.
        // It sits immediately before the (now-removed) assistant message.
        let user_idx = self
            .streaming_message_idx
            .map(|i| i.saturating_sub(1))
            .unwrap_or_else(|| self.messages.len().saturating_sub(1));
        let restored = if user_idx < self.messages.len() {
            if let ChatMessage::User { .. } = self.messages[user_idx] {
                let msg = self.messages.remove(user_idx);
                if let ChatMessage::User { content, .. } = msg {
                    Some(content)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        self.streaming_reasoning_content = None;
        self.streaming_reasoning_message_idx = None;
        self.streaming_reasoning_pushed = false;
        self.streaming_reasoning_rendered_len = 0;
        self.streaming_content = None;
        self.streaming_message_idx = None;
        self.streaming_message_pushed = false;
        self.streaming_rendered_len = 0;
        restored
    }

    /// Finalizes the current streaming turn, flushing any remaining buffered content and
    /// clearing all streaming state.
    pub fn finish_streaming(&mut self) {
        self.flush_reasoning_rendered();
        self.streaming_reasoning_content = None;
        self.streaming_reasoning_message_idx = None;
        self.streaming_reasoning_pushed = false;
        self.streaming_reasoning_rendered_len = 0;
        self.flush_rendered();
        self.streaming_content = None;
        self.streaming_message_idx = None;
        self.streaming_message_pushed = false;
        self.streaming_rendered_len = 0;
    }

    /// Scrolls by `delta` lines within `[0, max]`, updating the user-scrolling flag.
    pub fn scroll_by(&mut self, delta: i32, max: u16) {
        let offset = self.scroll_offset as i32 + delta;
        self.scroll_offset = offset.clamp(0, max as i32) as u16;
        self.user_is_scrolling = self.scroll_offset > 0;
    }

    /// Resets scroll to the bottom unless the user is actively scrolling.
    pub fn scroll_to_bottom(&mut self) {
        if !self.user_is_scrolling {
            self.scroll_offset = 0;
        }
    }

    /// Clears all chat state, resetting to a fresh empty session.
    pub fn reset(&mut self) {
        self.messages.clear();
        self.streaming_reasoning_content = None;
        self.streaming_reasoning_message_idx = None;
        self.streaming_reasoning_pushed = false;
        self.streaming_reasoning_rendered_len = 0;
        self.streaming_content = None;
        self.streaming_message_idx = None;
        self.streaming_message_pushed = false;
        self.streaming_rendered_len = 0;
        self.scroll_offset = 0;
        self.user_is_scrolling = false;
    }
}

/// A simplified message representation used for rendering in the TUI.
#[derive(Debug, Clone)]
pub enum ChatMessage {
    User {
        sender: Option<UserId>,
        content: String,
    },
    Reasoning {
        content: String,
    },
    Assistant {
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
        done: bool,
        is_error: bool,
        hunks: Vec<DiffHunk>,
    },
}

impl ChatMessage {
    /// Converts a list of gantry messages into `ChatMessage`s for rendering.
    pub fn messages_from(msgs: Vec<gantry_core::Message>) -> Vec<Self> {
        msgs.into_iter()
            .map(|msg| {
                let text = msg.text();
                match msg {
                    gantry_core::Message::User { sender, .. } => Self::User {
                        sender,
                        content: text,
                    },
                    gantry_core::Message::Assistant { .. } => Self::Assistant { content: text },
                }
            })
            .collect()
    }
}
