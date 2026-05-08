use gantry_core::{Branch, ModelSelection, SessionId, SessionTree, UserId};

/// The top-level editing mode, analogous to Vim's modal editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Navigation/command mode. Typing does not enter text into the input buffer.
    Normal,
    /// Text entry mode. Keys are forwarded to the input buffer.
    Insert,
}

pub struct Model {
    pub session_id: Option<SessionId>,
    pub selection: Option<ModelSelection>,
    pub mode: InputMode,
    pub chat: ChatModel,
    pub input: InputModel,
    pub command_picker: Option<CommandPicker>,
    pub tree_view: Option<TreeView>,
    pub status_message: Option<String>,
}

pub struct TreeView {
    pub tree: SessionTree,
    /// Index into the DFS row order of the currently highlighted row.
    pub selected_idx: usize,
    /// First visible row index (scroll offset).
    pub scroll_offset: usize,
}

/// A simplified message representation used for rendering in the TUI.
#[derive(Debug, Clone)]
pub enum ChatMessage {
    User {
        sender: Option<UserId>,
        content: String,
    },
    Assistant {
        content: String,
    },
    ToolResult {
        tool_name: String,
        content: String,
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

pub struct ChatModel {
    pub messages: Vec<ChatMessage>,
    pub pending_message_id: Option<String>,
    pub streaming_content: Option<String>,
    pub streaming_message_idx: Option<usize>,
    pub streaming_buffer: String,
    /// False until the first content is flushed — delays the assistant message from appearing.
    pub streaming_message_pushed: bool,
    /// Number of lines scrolled up from the bottom (0 = pinned to bottom).
    pub scroll_offset: u16,
    /// True while the user has manually scrolled up; suppresses auto-scroll-to-bottom.
    pub user_is_scrolling: bool,
}

pub struct InputModel {
    pub value: String,
    pub cursor: usize,
}

pub struct CommandPicker {
    pub commands: Vec<CommandEntry>,
    pub filter: String,
    pub selected_idx: usize,
}

#[derive(Clone)]
pub struct CommandEntry {
    pub name: String,
    pub description: String,
    pub command: std::sync::Arc<dyn crate::commands::Command>,
}

impl Model {
    pub fn new() -> Self {
        Self {
            session_id: None,
            selection: None,
            mode: InputMode::Normal,
            chat: ChatModel::new(),
            input: InputModel::new(),
            command_picker: None,
            tree_view: None,
            status_message: None,
        }
    }

    pub fn is_streaming(&self) -> bool {
        self.chat.streaming_content.is_some()
    }

    pub fn is_command_picker_active(&self) -> bool {
        self.command_picker.is_some()
    }

    // Command picker mutations
    pub fn activate_command_picker(&mut self, commands: Vec<CommandEntry>) {
        self.command_picker = Some(CommandPicker {
            commands,
            filter: String::new(),
            selected_idx: 0,
        });
    }

    pub fn deactivate_command_picker(&mut self) {
        self.command_picker = None;
    }

    /// Appends a character to the command picker's filter string.
    pub fn command_picker_filter_push(&mut self, c: char) {
        if let Some(ref mut picker) = self.command_picker {
            picker.filter.push(c);
            picker.selected_idx = 0;
        }
    }

    /// Removes the last character from the command picker's filter string.
    pub fn command_picker_filter_pop(&mut self) {
        if let Some(ref mut picker) = self.command_picker {
            picker.filter.pop();
            picker.selected_idx = 0;
        }
    }

    pub fn move_command_selection_up(&mut self) {
        if let Some(ref mut picker) = self.command_picker {
            picker.selected_idx = picker.selected_idx.saturating_sub(1);
        }
    }

    pub fn move_command_selection_down(&mut self) {
        if let Some(ref mut picker) = self.command_picker {
            let count = picker.filtered_commands().len();
            if count > 0 {
                picker.selected_idx = (picker.selected_idx + 1) % count;
            }
        }
    }

    pub fn selected_command(&self) -> Option<CommandEntry> {
        self.command_picker
            .as_ref()
            .and_then(|p| p.filtered_commands().get(p.selected_idx).cloned())
    }

    // Tree view mutations

    pub fn is_tree_view_active(&self) -> bool {
        self.tree_view.is_some()
    }

    pub fn activate_tree_view(&mut self, tree: SessionTree) {
        let selected_idx = branch_rows(&tree.stem, 0)
            .iter()
            .position(|(b, _)| b.node.id == tree.current_leaf_id)
            .unwrap_or(0);
        self.tree_view = Some(TreeView {
            tree,
            selected_idx,
            scroll_offset: 0,
        });
    }

    pub fn deactivate_tree_view(&mut self) {
        self.tree_view = None;
    }

    pub fn move_tree_selection_up(&mut self) {
        if let Some(ref mut tv) = self.tree_view {
            tv.selected_idx = tv.selected_idx.saturating_sub(1);
        }
    }

    pub fn move_tree_selection_down(&mut self) {
        if let Some(ref mut tv) = self.tree_view {
            let count = branch_rows(&tv.tree.stem, 0).len();
            if count > 0 {
                tv.selected_idx = (tv.selected_idx + 1).min(count - 1);
            }
        }
    }

    pub fn selected_tree_node(&self) -> Option<&Branch> {
        self.tree_view
            .as_ref()
            .and_then(|tv| {
                branch_rows(&tv.tree.stem, 0)
                    .into_iter()
                    .nth(tv.selected_idx)
            })
            .map(|(n, _)| n)
    }
}

/// Flattens a `Branch` tree into a DFS-ordered list of `(branch, depth)` pairs for row-indexed access.
pub fn branch_rows(branch: &Branch, depth: usize) -> Vec<(&Branch, usize)> {
    let mut rows = vec![(branch, depth)];
    for sub in &branch.branches {
        rows.extend(branch_rows(sub, depth + 1));
    }
    rows
}

impl ChatModel {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            pending_message_id: None,
            streaming_content: None,
            streaming_message_idx: None,
            streaming_buffer: String::new(),
            streaming_message_pushed: false,
            scroll_offset: 0,
            user_is_scrolling: false,
        }
    }

    /// Adds a user message with no sender (single-user session).
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(ChatMessage::User {
            sender: None,
            content,
        });
    }

    pub fn start_streaming_message(&mut self) {
        self.streaming_content = Some(String::new());
        self.streaming_message_idx = Some(self.messages.len());
        self.streaming_message_pushed = false;
        // The actual message is not pushed until the first content is flushed,
        // so the assistant prefix doesn't appear before any text arrives.
    }

    pub fn append_to_streaming(&mut self, content: &str) {
        if self.streaming_content.is_none() {
            return;
        }
        self.streaming_buffer.push_str(content);
        while let Some(newline_idx) = self.streaming_buffer.find('\n') {
            let line: String = self.streaming_buffer.drain(..=newline_idx).collect();
            if let Some(ref mut streaming) = self.streaming_content {
                // Push the message on first flush.
                if !self.streaming_message_pushed {
                    self.messages.push(ChatMessage::Assistant {
                        content: String::new(),
                    });
                    self.streaming_message_pushed = true;
                }
                streaming.push_str(&line);
                if let Some(idx) = self.streaming_message_idx
                    && idx < self.messages.len()
                    && let ChatMessage::Assistant { ref mut content } = self.messages[idx]
                {
                    content.push_str(&line);
                }
            }
        }
    }

    /// Cancels an in-progress stream, rolling back the optimistic user message and any
    /// partial assistant content. Returns the rolled-back user message text so the caller
    /// can restore it to the input.
    pub fn cancel_streaming(&mut self) -> Option<String> {
        // Remove any partial assistant message that was pushed during streaming.
        if self.streaming_message_pushed {
            if let Some(idx) = self.streaming_message_idx {
                if idx < self.messages.len() {
                    self.messages.remove(idx);
                }
            }
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
        self.streaming_content = None;
        self.streaming_message_idx = None;
        self.streaming_buffer.clear();
        self.streaming_message_pushed = false;
        self.pending_message_id = None;
        restored
    }

    pub fn finish_streaming(&mut self) {
        if !self.streaming_buffer.is_empty()
            && let Some(ref mut streaming) = self.streaming_content
        {
            if !self.streaming_message_pushed {
                self.messages.push(ChatMessage::Assistant {
                    content: String::new(),
                });
                self.streaming_message_pushed = true;
            }
            streaming.push_str(&self.streaming_buffer);
            if let Some(idx) = self.streaming_message_idx
                && idx < self.messages.len()
                && let ChatMessage::Assistant { ref mut content } = self.messages[idx]
            {
                content.push_str(&self.streaming_buffer);
            }
        }
        self.streaming_content = None;
        self.streaming_message_idx = None;
        self.streaming_buffer.clear();
        self.streaming_message_pushed = false;
        self.pending_message_id = None;
    }

    pub fn reset(&mut self) {
        self.messages.clear();
        self.streaming_content = None;
        self.streaming_message_idx = None;
        self.streaming_buffer.clear();
        self.streaming_message_pushed = false;
        self.pending_message_id = None;
        self.scroll_offset = 0;
        self.user_is_scrolling = false;
    }
}

impl InputModel {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
        }
    }

    pub fn insert(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn delete_before_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_char_boundary();
        self.value.drain(prev..self.cursor);
        self.cursor = prev;
    }

    pub fn move_left(&mut self) {
        self.cursor = self.prev_char_boundary();
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            let c = self.value[self.cursor..].chars().next().unwrap();
            self.cursor += c.len_utf8();
        }
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    fn prev_char_boundary(&self) -> usize {
        let mut pos = self.cursor;
        while pos > 0 {
            pos -= 1;
            if self.value.is_char_boundary(pos) {
                return pos;
            }
        }
        0
    }
}

impl CommandPicker {
    /// Returns commands whose names contain every character in `filter` as a subsequence.
    pub fn filtered_commands(&self) -> Vec<CommandEntry> {
        if self.filter.is_empty() {
            return self.commands.clone();
        }
        self.commands
            .iter()
            .filter(|c| fuzzy_match(&c.name, &self.filter))
            .cloned()
            .collect()
    }
}

/// Returns true if every character in `needle` appears in `haystack` in order.
fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|n| chars.any(|h| h == n))
}
