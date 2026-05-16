use gantry_core::{DiffHunk, UserId};

pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    buffer: Option<TurnBuffer>,
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
            buffer: None,
            scroll_offset: 0,
            user_is_scrolling: false,
        }
    }

    /// Inserts a tool call row with `done: false`, flushing any pending buffered text first.
    pub fn push_tool_call(&mut self, id: String, name: String, arguments: serde_json::Value) {
        self.drain_buffer();
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
        for msg in self.messages.iter_mut().rev() {
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
            content: content.trim().to_string(),
        });
    }

    /// Begins a new assistant turn, flushing any buffered content from the previous turn first.
    pub fn start_streaming_message(&mut self) {
        self.drain_buffer();
        self.buffer = Some(TurnBuffer::new(BufferKind::Reasoning));
    }

    /// Appends reasoning content to the current turn buffer.
    pub fn append_to_reasoning(&mut self, content: &str) {
        if let Some(ref mut buf) = self.buffer {
            buf.append(content, &mut self.messages);
        }
    }

    /// Appends assistant content to the current turn buffer, switching the buffer kind if needed.
    pub fn append_to_streaming(&mut self, content: &str) {
        if let Some(ref mut buf) = self.buffer {
            if !matches!(buf.kind, BufferKind::Assistant) {
                buf.flush_all(&mut self.messages);
                *buf = TurnBuffer::new(BufferKind::Assistant);
            }
            buf.append(content, &mut self.messages);
        }
    }

    /// Interrupts an in-progress turn, flushing buffered content so it remains readable.
    pub fn interrupt_streaming(&mut self) {
        self.drain_buffer();
    }

    /// Rolls back an in-progress turn, removing the optimistic user message and any
    /// partial assistant content. Returns the rolled-back user message text so the caller
    /// can restore it to the input.
    pub fn rollback_streaming(&mut self) -> Option<String> {
        // Determine the message index the buffer was writing into (if any).
        let buf_msg_idx = self.buffer.as_ref().and_then(|b| b.message_idx);
        self.buffer = None;

        if let Some(idx) = buf_msg_idx {
            self.messages.remove(idx);
        }

        // The optimistic user message sits immediately before the (now-removed) assistant slot.
        let user_idx = buf_msg_idx
            .map(|i| i.saturating_sub(1))
            .unwrap_or_else(|| self.messages.len().saturating_sub(1));

        if matches!(self.messages.get(user_idx), Some(ChatMessage::User { .. })) {
            let ChatMessage::User { content, .. } = self.messages.remove(user_idx) else {
                unreachable!()
            };
            return Some(content);
        }
        None
    }

    /// Finalizes the current turn, flushing all remaining buffered content.
    pub fn finish_streaming(&mut self) {
        self.drain_buffer();
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
        self.buffer = None;
        self.scroll_offset = 0;
        self.user_is_scrolling = false;
    }

    /// Flushes all buffered content to `messages` and clears the buffer.
    fn drain_buffer(&mut self) {
        if let Some(ref mut buf) = self.buffer {
            buf.flush_all(&mut self.messages);
        }
        self.buffer = None;
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

struct TurnBuffer {
    kind: BufferKind,
    content: String,
    /// Byte offset up to which content has been flushed to `messages`.
    /// Content is only flushed on paragraph boundaries (`\n\n`).
    rendered_len: usize,
    /// Index into `messages` for the slot this buffer writes into.
    /// `None` until the first flush.
    message_idx: Option<usize>,
}

enum BufferKind {
    Reasoning,
    Assistant,
}

impl TurnBuffer {
    fn new(kind: BufferKind) -> Self {
        Self {
            kind,
            content: String::new(),
            rendered_len: 0,
            message_idx: None,
        }
    }

    /// Appends `content` and flushes up to the last paragraph boundary.
    fn append(&mut self, content: &str, messages: &mut Vec<ChatMessage>) {
        self.content.push_str(content);

        let unrendered = &self.content[self.rendered_len..];
        let flush_end = match unrendered.rfind("\n\n") {
            Some(pos) => self.rendered_len + pos + 2,
            None => return,
        };

        self.flush_up_to(flush_end, messages);
    }

    /// Flushes all remaining buffered content to `messages`, trimming trailing whitespace.
    fn flush_all(&mut self, messages: &mut Vec<ChatMessage>) {
        // Trim trailing whitespace from the accumulated content before the final flush.
        let trimmed_end = self.content.trim_end().len();
        if self.rendered_len < trimmed_end {
            self.flush_up_to(trimmed_end, messages);
        }
    }

    fn flush_up_to(&mut self, end: usize, messages: &mut Vec<ChatMessage>) {
        let pending = self.content[self.rendered_len..end].to_owned();
        self.rendered_len = end;

        match self.message_idx {
            None => {
                let msg = match self.kind {
                    BufferKind::Reasoning => ChatMessage::Reasoning { content: pending },
                    BufferKind::Assistant => ChatMessage::Assistant { content: pending },
                };
                messages.push(msg);
                self.message_idx = Some(messages.len() - 1);
            }
            Some(idx) => match (&self.kind, messages.get_mut(idx)) {
                (BufferKind::Reasoning, Some(ChatMessage::Reasoning { content }))
                | (BufferKind::Assistant, Some(ChatMessage::Assistant { content })) => {
                    content.push_str(&pending);
                }
                _ => {}
            },
        }
    }
}
