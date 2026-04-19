use gantry_core::{Branch, BranchNode, Message, Role, SessionTree};

pub struct Model {
    pub session_id: String,
    pub connection_state: ConnectionState,
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

pub enum ConnectionState {
    Connected,
    Disconnected,
}

pub struct ChatModel {
    pub messages: Vec<Message>,
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
            session_id: String::new(),
            connection_state: ConnectionState::Disconnected,
            chat: ChatModel::new(),
            input: InputModel::new(),
            command_picker: None,
            tree_view: None,
            status_message: None,
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.connection_state, ConnectionState::Connected)
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

    pub fn update_command_filter(&mut self, filter: &str) {
        if let Some(ref mut picker) = self.command_picker {
            picker.filter = filter.to_string();
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
        let selected_idx = branch_rows(&tree.stem)
            .iter()
            .position(|(n, _)| Some(&n.id) == tree.current_leaf_id.as_ref())
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
            let count = branch_rows(&tv.tree.stem).len();
            if count > 0 {
                tv.selected_idx = (tv.selected_idx + 1).min(count - 1);
            }
        }
    }

    pub fn selected_tree_node(&self) -> Option<&BranchNode> {
        self.tree_view
            .as_ref()
            .and_then(|tv| branch_rows(&tv.tree.stem).into_iter().nth(tv.selected_idx))
            .map(|(n, _)| n)
    }
}

/// Flatten a `Branch` into a DFS-ordered list of `(node, depth)` pairs for row-indexed access.
pub fn branch_rows(branch: &Branch) -> Vec<(&BranchNode, usize)> {
    let mut rows = Vec::new();
    for node in &branch.nodes {
        rows.push((node, branch.depth));
        for sub in &node.branches {
            rows.extend(branch_rows(sub));
        }
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

    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(Message::new(Role::User, content));
    }

    pub fn add_error_message(&mut self, content: String) {
        self.messages.push(Message::new(Role::Error, content));
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
                    self.messages
                        .push(Message::new(Role::Assistant, String::new()));
                    self.streaming_message_pushed = true;
                }
                streaming.push_str(&line);
                if let Some(idx) = self.streaming_message_idx
                    && idx < self.messages.len()
                {
                    self.messages[idx].content.push_str(&line);
                }
            }
        }
    }

    pub fn finish_streaming(&mut self) {
        if !self.streaming_buffer.is_empty()
            && let Some(ref mut streaming) = self.streaming_content
        {
            if !self.streaming_message_pushed {
                self.messages
                    .push(Message::new(Role::Assistant, String::new()));
                self.streaming_message_pushed = true;
            }
            streaming.push_str(&self.streaming_buffer);
            if let Some(idx) = self.streaming_message_idx
                && idx < self.messages.len()
            {
                self.messages[idx].content.push_str(&self.streaming_buffer);
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
    pub fn filtered_commands(&self) -> Vec<CommandEntry> {
        if self.filter.is_empty() {
            return self.commands.clone();
        }
        self.commands
            .iter()
            .filter(|c| c.name.starts_with(&self.filter))
            .cloned()
            .collect()
    }
}
