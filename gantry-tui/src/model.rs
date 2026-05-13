use std::path::Path;

use nucleo_matcher::{
    Config, Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

use gantry_core::{
    Branch, ContextWindow, InputToken, ModelSelection, PathSearchResult, ProviderConfig,
    SessionId, SessionInfo, SessionTree, SkillSearchResult, Usage,
};

use crate::chat::ChatModel;
use crate::providers::{ProvidersSubView, ProvidersView};
use crate::sessions::SessionsView;
use crate::tree::{TreeView, branch_rows};

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
    pub project_path: std::path::PathBuf,
    pub cwd: std::path::PathBuf,
    pub command_picker: Option<CommandPicker>,
    pub attachment_picker: Option<AttachmentPicker>,
    pub sessions_view: Option<SessionsView>,
    pub tree_view: Option<TreeView>,
    pub providers_view: Option<ProvidersView>,
    pub model_picker_view: Option<ModelPickerView>,
    pub usage_view: Option<UsageView>,
    pub status_message: Option<String>,
    /// Context window snapshot from the most recently completed stream.
    pub context_window: Option<ContextWindow>,
    /// Cached model list fetched on first open of the model picker.
    pub cached_models: Option<Vec<ModelSelection>>,
}

/// State for the context window usage overlay.
pub struct UsageView {
    pub context_window: ContextWindow,
    /// Accumulated token consumption across all nodes in the session.
    pub consumption: Usage,
}

/// State for the model picker overlay.
pub struct ModelPickerView {
    pub models: Vec<ModelSelection>,
    pub filter: String,
    /// Index of the cursor row (keyboard highlight).
    pub selected_idx: usize,
    /// The model that was active when the picker was opened, used to mark the current selection.
    pub active_selection: Option<ModelSelection>,
    /// Cached fuzzy-filtered results; recomputed on every filter change.
    pub filtered: Vec<ModelEntry>,
    /// Maximum model alias width across the full unfiltered list; stable for the lifetime of the picker.
    pub model_col_width: u16,
    /// Maximum provider alias width across the full unfiltered list; stable for the lifetime of the picker.
    pub provider_col_width: u16,
    /// Maximum context length label width across the full unfiltered list; stable for the lifetime of the picker.
    pub context_col_width: u16,
}

/// A filtered model entry with fuzzy-match highlight indices.
#[derive(Clone)]
pub struct ModelEntry {
    pub selection: ModelSelection,
    /// Matched character indices into the display label from the last fuzzy filter.
    pub indices: Vec<u32>,
    /// Whether this entry is the active (currently selected) model.
    pub is_active: bool,
}

pub struct InputModel {
    pub tokens: Vec<InputToken>,
    pub cursor: InputCursor,
}

/// Cursor position within the token sequence.
#[derive(Debug, Clone, PartialEq)]
pub enum InputCursor {
    /// Cursor is inside a `Text` token at the given byte offset.
    InText {
        token_idx: usize,
        byte_offset: usize,
    },
    /// Cursor is positioned on an attachment token (next backspace deletes it).
    AtAttachment { token_idx: usize },
}

pub struct CommandPicker {
    pub commands: Vec<CommandEntry>,
    pub filter: String,
    pub selected_idx: usize,
    /// Cached fuzzy-filtered results; recomputed on every filter change.
    pub filtered: Vec<CommandEntry>,
    /// Maximum command name width across the full unfiltered list; stable for the lifetime of the picker.
    pub cmd_col_width: u16,
}

#[derive(Clone)]
pub struct CommandEntry {
    pub name: String,
    pub description: String,
    pub command: crate::commands::KnownCommand,
    /// Matched character indices into `name` from the last fuzzy filter. Empty when unfiltered.
    pub indices: Vec<u32>,
}

/// A fuzzy-find picker for file/directory or skill attachments.
pub struct AttachmentPicker {
    pub kind: AttachmentPickerKind,
    pub filter: String,
    pub selected_idx: usize,
    /// Maximum name width across all results in the current result set; recomputed when results change.
    ///
    /// For path pickers this is always 0 (single column). For skill pickers it stabilises the
    /// name column width across scroll.
    pub name_col_width: u16,
}

/// Discriminates between path and skill attachment pickers.
pub enum AttachmentPickerKind {
    Path(Vec<PathSearchResult>),
    Skill(Vec<SkillSearchResult>),
}

impl AttachmentPicker {
    /// Creates a new path picker with the given search results.
    pub fn new_path(results: Vec<PathSearchResult>) -> Self {
        Self {
            kind: AttachmentPickerKind::Path(results),
            filter: String::new(),
            selected_idx: 0,
            name_col_width: 0,
        }
    }

    /// Creates a new skill picker with the given search results.
    pub fn new_skill(results: Vec<SkillSearchResult>) -> Self {
        let name_col_width = results
            .iter()
            .map(|r| r.skill.metadata.name.chars().count() as u16)
            .max()
            .unwrap_or(0);
        Self {
            kind: AttachmentPickerKind::Skill(results),
            filter: String::new(),
            selected_idx: 0,
            name_col_width,
        }
    }

    /// Replaces the path results and recomputes stable column widths.
    pub fn set_path_results(&mut self, results: Vec<PathSearchResult>) {
        self.kind = AttachmentPickerKind::Path(results);
        self.name_col_width = 0;
        self.selected_idx = 0;
    }

    /// Replaces the skill results and recomputes the stable name column width.
    pub fn set_skill_results(&mut self, results: Vec<SkillSearchResult>) {
        self.name_col_width = results
            .iter()
            .map(|r| r.skill.metadata.name.chars().count() as u16)
            .max()
            .unwrap_or(0);
        self.kind = AttachmentPickerKind::Skill(results);
        self.selected_idx = 0;
    }

    /// Returns the number of items currently displayed.
    pub fn len(&self) -> usize {
        match &self.kind {
            AttachmentPickerKind::Path(results) => results.len(),
            AttachmentPickerKind::Skill(results) => results.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Formats a context length in tokens as a compact string (e.g. `131072` → `"128k"`).
pub fn format_context_length(tokens: u32) -> String {
    format!("{}k", (tokens + 512) / 1024)
}

impl Model {
    pub fn new() -> Self {
        Self {
            session_id: None,
            selection: None,
            mode: InputMode::Normal,
            chat: ChatModel::new(),
            input: InputModel::new(),
            project_path: std::path::PathBuf::new(),
            cwd: std::path::PathBuf::new(),
            command_picker: None,
            attachment_picker: None,
            sessions_view: None,
            tree_view: None,
            providers_view: None,
            model_picker_view: None,
            usage_view: None,
            status_message: None,
            context_window: None,
            cached_models: None,
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
        let cmd_col_width = commands
            .iter()
            .map(|c| c.name.chars().count() as u16)
            .max()
            .unwrap_or(0);
        let mut picker = CommandPicker {
            commands,
            filter: String::new(),
            selected_idx: 0,
            filtered: Vec::new(),
            cmd_col_width,
        };
        picker.refilter();
        self.command_picker = Some(picker);
    }

    pub fn deactivate_command_picker(&mut self) {
        self.command_picker = None;
    }

    /// Appends a character to the command picker's filter string.
    pub fn command_picker_filter_push(&mut self, c: char) {
        if let Some(ref mut picker) = self.command_picker {
            picker.filter.push(c);
            picker.selected_idx = 0;
            picker.refilter();
        }
    }

    /// Removes the last character from the command picker's filter string.
    pub fn command_picker_filter_pop(&mut self) {
        if let Some(ref mut picker) = self.command_picker {
            picker.filter.pop();
            picker.selected_idx = 0;
            picker.refilter();
        }
    }

    /// Moves the command picker cursor up, wrapping from the first to the last entry.
    pub fn move_command_selection_up(&mut self) {
        if let Some(ref mut picker) = self.command_picker {
            let count = picker.filtered.len();
            if count > 0 {
                picker.selected_idx = picker.selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
    }

    /// Moves the command picker cursor down, wrapping from the last to the first entry.
    pub fn move_command_selection_down(&mut self) {
        if let Some(ref mut picker) = self.command_picker {
            let count = picker.filtered.len();
            if count > 0 {
                picker.selected_idx = (picker.selected_idx + 1) % count;
            }
        }
    }

    /// Returns the currently highlighted command in the command picker, if any.
    pub fn selected_command(&self) -> Option<CommandEntry> {
        self.command_picker
            .as_ref()
            .and_then(|p| p.filtered.get(p.selected_idx).cloned())
    }

    // Attachment picker mutations

    pub fn is_attachment_picker_active(&self) -> bool {
        self.attachment_picker.is_some()
    }

    /// Opens the path attachment picker, inserting `+` into the input to seed the filter display.
    pub fn activate_path_picker(&mut self, results: Vec<PathSearchResult>) {
        self.input.insert('+');
        self.attachment_picker = Some(AttachmentPicker::new_path(results));
    }

    /// Opens the skill attachment picker, inserting `/` into the input to seed the filter display.
    pub fn activate_skill_picker(&mut self, results: Vec<SkillSearchResult>) {
        self.input.insert('/');
        self.attachment_picker = Some(AttachmentPicker::new_skill(results));
    }

    /// Closes the attachment picker, leaving the typed sigil and filter text in the input as-is.
    pub fn deactivate_attachment_picker(&mut self) {
        self.attachment_picker = None;
    }

    /// Appends a character to the attachment picker filter string and mirrors it into the input.
    ///
    /// Search results are replaced by the caller via `Msg::RefineAttachmentPicker` after
    /// the new query is known; this method only updates the filter string and input.
    pub fn attachment_picker_filter_push(&mut self, c: char) {
        if let Some(ref mut picker) = self.attachment_picker {
            picker.filter.push(c);
            picker.selected_idx = 0;
        }
        self.input.insert(c);
    }

    /// Clears the attachment picker filter string and removes all filter characters from the input.
    pub fn attachment_picker_filter_clear(&mut self) {
        if let Some(ref mut picker) = self.attachment_picker {
            let len = picker.filter.len();
            picker.filter.clear();
            picker.selected_idx = 0;
            for _ in 0..len {
                self.input.delete_before_cursor();
            }
        }
    }

    /// Removes the last character from the attachment picker filter string and from the input.
    ///
    /// Returns `false` when the filter was already empty, signalling the caller to close the picker.
    pub fn attachment_picker_filter_pop(&mut self) -> bool {
        if let Some(ref mut picker) = self.attachment_picker {
            if picker.filter.is_empty() {
                // Remove the sigil from the input.
                self.input.delete_before_cursor();
                return false;
            }
            picker.filter.pop();
            picker.selected_idx = 0;
            self.input.delete_before_cursor();
            true
        } else {
            false
        }
    }

    /// Returns the current filter string of the attachment picker.
    pub fn attachment_picker_filter(&self) -> Option<&str> {
        self.attachment_picker.as_ref().map(|p| p.filter.as_str())
    }

    pub fn move_attachment_selection_up(&mut self) {
        if let Some(ref mut picker) = self.attachment_picker {
            let count = picker.len();
            if count > 0 {
                picker.selected_idx = picker.selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
    }

    pub fn move_attachment_selection_down(&mut self) {
        if let Some(ref mut picker) = self.attachment_picker {
            let count = picker.len();
            if count > 0 {
                picker.selected_idx = (picker.selected_idx + 1) % count;
            }
        }
    }

    /// Returns the selected attachment token, if any item is selected.
    pub fn selected_attachment(&self) -> Option<InputToken> {
        let picker = self.attachment_picker.as_ref()?;
        match &picker.kind {
            AttachmentPickerKind::Path(results) => {
                let path = &results.get(picker.selected_idx)?.path;
                Some(InputToken::Path(path.clone()))
            }
            AttachmentPickerKind::Skill(results) => {
                let skill = &results.get(picker.selected_idx)?.skill;
                Some(InputToken::Skill {
                    name: skill.metadata.name.clone(),
                    path: skill.skill_file.clone(),
                })
            }
        }
    }

    // Sessions view mutations

    pub fn is_sessions_view_active(&self) -> bool {
        self.sessions_view.is_some()
    }

    /// Opens the sessions browser, pre-selecting the currently active session.
    pub fn activate_sessions_view(
        &mut self,
        sessions: Vec<SessionInfo>,
        active_session_id: SessionId,
    ) {
        let selected_idx = sessions
            .iter()
            .rposition(|s| s.id == active_session_id)
            .unwrap_or(sessions.len().saturating_sub(1));
        self.sessions_view = Some(SessionsView {
            sessions,
            selected_idx,
            active_session_id,
        });
    }

    pub fn deactivate_sessions_view(&mut self) {
        self.sessions_view = None;
    }

    pub fn move_sessions_selection_up(&mut self) {
        if let Some(ref mut sv) = self.sessions_view
            && !sv.sessions.is_empty()
        {
            sv.selected_idx = sv
                .selected_idx
                .checked_sub(1)
                .unwrap_or(sv.sessions.len() - 1);
        }
    }

    pub fn move_sessions_selection_down(&mut self) {
        if let Some(ref mut sv) = self.sessions_view
            && !sv.sessions.is_empty()
        {
            sv.selected_idx = (sv.selected_idx + 1) % sv.sessions.len();
        }
    }

    /// Returns the session highlighted in the browser, if any.
    pub fn selected_session(&self) -> Option<&SessionInfo> {
        self.sessions_view
            .as_ref()
            .and_then(|sv| sv.sessions.get(sv.selected_idx))
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

    pub fn is_providers_view_active(&self) -> bool {
        self.providers_view.is_some()
    }

    pub fn activate_providers_view(&mut self, providers: Vec<ProviderConfig>) {
        self.providers_view = Some(ProvidersView {
            providers,
            sub: ProvidersSubView::List { selected_idx: 0 },
        });
    }

    pub fn deactivate_providers_view(&mut self) {
        self.providers_view = None;
    }

    pub fn is_model_picker_active(&self) -> bool {
        self.model_picker_view.is_some()
    }

    /// Opens the model picker with the given list of available models.
    pub fn activate_model_picker_view(&mut self, models: Vec<ModelSelection>) {
        let active_selection = self.selection.clone();
        let selected_idx = active_selection
            .as_ref()
            .and_then(|s| models.iter().position(|m| m == s))
            .unwrap_or(0);
        let model_col_width = models
            .iter()
            .map(|s| s.model_id.as_str().chars().count() as u16)
            .max()
            .unwrap_or(0);
        let provider_col_width = models
            .iter()
            .map(|s| s.provider_alias.as_str().chars().count() as u16)
            .max()
            .unwrap_or(0);
        let context_col_width = models
            .iter()
            .filter_map(|s| s.context_length)
            .map(|n| format_context_length(n).len() as u16)
            .max()
            .unwrap_or(0);
        let mut picker = ModelPickerView {
            models,
            filter: String::new(),
            selected_idx,
            active_selection,
            filtered: Vec::new(),
            model_col_width,
            provider_col_width,
            context_col_width,
        };
        picker.refilter();
        self.model_picker_view = Some(picker);
    }

    pub fn deactivate_model_picker_view(&mut self) {
        self.model_picker_view = None;
    }

    /// Returns `true` if the context window usage overlay is currently shown.
    pub fn is_usage_view_active(&self) -> bool {
        self.usage_view.is_some()
    }

    /// Activates the context window usage overlay with the given snapshot.
    pub fn activate_usage_view(&mut self, context_window: ContextWindow, consumption: Usage) {
        self.usage_view = Some(UsageView {
            context_window,
            consumption,
        });
    }

    /// Closes the context window usage overlay.
    pub fn deactivate_usage_view(&mut self) {
        self.usage_view = None;
    }

    /// Appends a character to the model picker filter string and resets the cursor to the top.
    pub fn model_picker_filter_push(&mut self, c: char) {
        if let Some(ref mut mv) = self.model_picker_view {
            mv.filter.push(c);
            mv.selected_idx = 0;
            mv.refilter();
        }
    }

    /// Removes the last character from the model picker filter string and resets the cursor.
    pub fn model_picker_filter_pop(&mut self) {
        if let Some(ref mut mv) = self.model_picker_view {
            mv.filter.pop();
            mv.selected_idx = 0;
            mv.refilter();
        }
    }

    /// Moves the model picker cursor up, wrapping from the first to the last entry.
    pub fn move_model_picker_selection_up(&mut self) {
        if let Some(ref mut mv) = self.model_picker_view {
            let count = mv.filtered.len();
            if count > 0 {
                mv.selected_idx = mv.selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
    }

    /// Moves the model picker cursor down, wrapping from the last to the first entry.
    pub fn move_model_picker_selection_down(&mut self) {
        if let Some(ref mut mv) = self.model_picker_view {
            let count = mv.filtered.len();
            if count > 0 {
                mv.selected_idx = (mv.selected_idx + 1) % count;
            }
        }
    }

    /// Returns the currently highlighted model selection in the model picker, if any.
    pub fn selected_model_in_picker(&self) -> Option<ModelSelection> {
        self.model_picker_view.as_ref().and_then(|mv| {
            mv.filtered
                .get(mv.selected_idx)
                .map(|e| e.selection.clone())
        })
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

/// Returns the byte offset of the character boundary immediately before `cursor` in `s`.
pub fn prev_char_boundary(s: &str, cursor: usize) -> usize {
    let mut pos = cursor;
    while pos > 0 {
        pos -= 1;
        if s.is_char_boundary(pos) {
            return pos;
        }
    }
    0
}


impl InputModel {
    pub fn new() -> Self {
        Self {
            tokens: vec![InputToken::Text(String::new())],
            cursor: InputCursor::InText {
                token_idx: 0,
                byte_offset: 0,
            },
        }
    }

    /// Inserts a character at the current cursor position within the active text token.
    ///
    /// If the cursor is on an attachment, the character is inserted into the text token
    /// immediately before the attachment.
    pub fn insert(&mut self, c: char) {
        match self.cursor.clone() {
            InputCursor::InText {
                token_idx,
                byte_offset,
            } => {
                if let InputToken::Text(ref mut text) = self.tokens[token_idx] {
                    text.insert(byte_offset, c);
                    self.cursor = InputCursor::InText {
                        token_idx,
                        byte_offset: byte_offset + c.len_utf8(),
                    };
                }
            }
            InputCursor::AtAttachment { token_idx } => {
                // Insert a new Text token before the attachment and place cursor in it.
                let new_text = c.to_string();
                let new_offset = c.len_utf8();
                self.tokens.insert(token_idx, InputToken::Text(new_text));
                self.cursor = InputCursor::InText {
                    token_idx,
                    byte_offset: new_offset,
                };
            }
        }
        self.normalize();
    }

    /// Deletes the character before the cursor, or the whole attachment token if the cursor is on one.
    pub fn delete_before_cursor(&mut self) {
        match self.cursor.clone() {
            InputCursor::InText {
                token_idx,
                byte_offset,
            } => {
                if byte_offset == 0 {
                    if token_idx == 0 {
                        return;
                    }
                    let prev_idx = token_idx - 1;
                    match &self.tokens[prev_idx] {
                        InputToken::Text(_) => {
                            // Should have been normalized; move into it.
                            let len = if let InputToken::Text(t) = &self.tokens[prev_idx] {
                                t.len()
                            } else {
                                unreachable!()
                            };
                            self.cursor = InputCursor::InText {
                                token_idx: prev_idx,
                                byte_offset: len,
                            };
                        }
                        _ => {
                            // Delete the attachment immediately rather than landing on it first.
                            self.tokens.remove(prev_idx);
                            let new_idx = prev_idx.saturating_sub(1);
                            let byte_offset = match self.tokens.get(new_idx) {
                                Some(InputToken::Text(t)) => t.len(),
                                _ => 0,
                            };
                            self.cursor = InputCursor::InText {
                                token_idx: new_idx,
                                byte_offset,
                            };
                            self.normalize();
                        }
                    }
                } else {
                    if let InputToken::Text(ref mut text) = self.tokens[token_idx] {
                        let prev = prev_char_boundary(text, byte_offset);
                        text.drain(prev..byte_offset);
                        self.cursor = InputCursor::InText {
                            token_idx,
                            byte_offset: prev,
                        };
                    }
                }
            }
            InputCursor::AtAttachment { token_idx } => {
                self.tokens.remove(token_idx);
                // Land in the text token that is now at token_idx (the one before the removed token
                // was merged into it by normalize, or the successor text token shifted down).
                let new_idx = token_idx.saturating_sub(1);
                let byte_offset = match self.tokens.get(new_idx) {
                    Some(InputToken::Text(t)) => t.len(),
                    _ => 0,
                };
                self.cursor = InputCursor::InText {
                    token_idx: new_idx,
                    byte_offset,
                };
                self.normalize();
            }
        }
    }

    /// Moves the cursor one position to the left.
    ///
    /// Attachment tokens are skipped transparently — the cursor only ever rests in Text tokens.
    pub fn move_left(&mut self) {
        let InputCursor::InText {
            token_idx,
            byte_offset,
        } = self.cursor.clone()
        else {
            return;
        };
        if byte_offset > 0 {
            if let InputToken::Text(ref text) = self.tokens[token_idx] {
                let prev = prev_char_boundary(text, byte_offset);
                self.cursor = InputCursor::InText {
                    token_idx,
                    byte_offset: prev,
                };
            }
            return;
        }
        // At the start of a text token — scan left for the previous text token, skipping attachments.
        let mut idx = token_idx;
        loop {
            if idx == 0 {
                return;
            }
            idx -= 1;
            if let InputToken::Text(t) = &self.tokens[idx] {
                self.cursor = InputCursor::InText {
                    token_idx: idx,
                    byte_offset: t.len(),
                };
                return;
            }
            // Non-text (attachment) token — keep scanning left.
        }
    }

    /// Moves the cursor one position to the right.
    ///
    /// Attachment tokens are skipped transparently — the cursor only ever rests in Text tokens.
    pub fn move_right(&mut self) {
        let InputCursor::InText {
            token_idx,
            byte_offset,
        } = self.cursor.clone()
        else {
            return;
        };
        if let InputToken::Text(ref text) = self.tokens[token_idx]
            && byte_offset < text.len()
        {
            let c = text[byte_offset..].chars().next().unwrap();
            self.cursor = InputCursor::InText {
                token_idx,
                byte_offset: byte_offset + c.len_utf8(),
            };
            return;
        }
        // At the end of a text token — scan right for the next text token, skipping attachments.
        let mut idx = token_idx;
        loop {
            idx += 1;
            if idx >= self.tokens.len() {
                return;
            }
            if matches!(self.tokens[idx], InputToken::Text(_)) {
                self.cursor = InputCursor::InText {
                    token_idx: idx,
                    byte_offset: 0,
                };
                return;
            }
            // Non-text (attachment) token — keep scanning right.
        }
    }

    /// Resets the input to an empty state.
    pub fn clear(&mut self) {
        self.tokens = vec![InputToken::Text(String::new())];
        self.cursor = InputCursor::InText {
            token_idx: 0,
            byte_offset: 0,
        };
    }

    /// Inserts an attachment token at the current cursor position.
    ///
    /// If the cursor is inside a text token, it is split at the cursor position. A new empty
    /// text token is appended after the attachment so the cursor always lands in a text token.
    pub fn insert_attachment(&mut self, token: InputToken) {
        match self.cursor.clone() {
            InputCursor::InText {
                token_idx,
                byte_offset,
            } => {
                let tail = if let InputToken::Text(ref mut text) = self.tokens[token_idx] {
                    text.split_off(byte_offset)
                } else {
                    String::new()
                };
                let attach_idx = token_idx + 1;
                self.tokens.insert(attach_idx, token);
                self.tokens
                    .insert(attach_idx + 1, InputToken::Text(format!(" {tail}")));
                self.cursor = InputCursor::InText {
                    token_idx: attach_idx + 1,
                    byte_offset: 1,
                };
            }
            InputCursor::AtAttachment { token_idx } => {
                self.tokens.insert(token_idx, token);
                let new_text_idx = token_idx + 1;
                self.tokens
                    .insert(new_text_idx, InputToken::Text(" ".to_string()));
                self.cursor = InputCursor::InText {
                    token_idx: new_text_idx,
                    byte_offset: 1,
                };
            }
        }
        self.normalize();
    }

    /// Replaces the trailing `sigil_and_filter_len` bytes before the cursor with an attachment token.
    ///
    /// Used when the user confirms a picker selection: the sigil + filter text already in the
    /// input is stripped and the chosen token is inserted in its place.
    pub fn replace_filter_with_attachment(
        &mut self,
        sigil_and_filter_len: usize,
        token: InputToken,
    ) {
        if let InputCursor::InText {
            token_idx,
            byte_offset,
        } = self.cursor.clone()
            && let InputToken::Text(ref mut text) = self.tokens[token_idx]
        {
            let strip_start = byte_offset.saturating_sub(sigil_and_filter_len);
            let tail = text.split_off(byte_offset);
            text.truncate(strip_start);
            let attach_idx = token_idx + 1;
            self.tokens.insert(attach_idx, token);
            self.tokens
                .insert(attach_idx + 1, InputToken::Text(format!(" {tail}")));
            self.cursor = InputCursor::InText {
                token_idx: attach_idx + 1,
                byte_offset: 1,
            };
            self.normalize();
        }
    }

    /// Returns a display string with attachment sigils (`+path`, `/skill`) inlined.
    /// Path tokens are shown relative to `project_root`.
    pub fn raw_display(&self, project_root: &Path) -> String {
        let mut out = String::new();
        for token in &self.tokens {
            match token {
                InputToken::Text(t) => out.push_str(t),
                InputToken::Path(p) => {
                    let rel = p.strip_prefix(project_root).unwrap_or(p);
                    out.push('+');
                    out.push_str(&rel.display().to_string());
                }
                InputToken::Skill { name, .. } => {
                    out.push('/');
                    out.push_str(name);
                }
            }
        }
        out
    }

    /// Returns whether the effective content (raw display) is blank.
    pub fn is_blank(&self) -> bool {
        // Path tokens are non-empty regardless of prefix stripping, so project_root is irrelevant here.
        self.raw_display(Path::new("")).trim().is_empty()
    }

    /// Returns the display string and the cursor's byte offset within it, suitable for rendering.
    /// Path tokens are shown relative to `project_root`.
    pub fn display_with_cursor(&self, project_root: &Path) -> (String, usize) {
        let mut out = String::new();
        let mut cursor_byte = 0usize;
        let mut found_cursor = false;

        for (idx, token) in self.tokens.iter().enumerate() {
            let token_start = out.len();
            match token {
                InputToken::Text(t) => {
                    if !found_cursor
                        && let InputCursor::InText {
                            token_idx,
                            byte_offset,
                        } = self.cursor
                        && token_idx == idx
                    {
                        cursor_byte = token_start + byte_offset;
                        found_cursor = true;
                    }
                    out.push_str(t);
                }
                InputToken::Path(p) => {
                    let rel = p.strip_prefix(project_root).unwrap_or(p);
                    let sigil = format!("+{}", rel.display());
                    if !found_cursor
                        && let InputCursor::AtAttachment { token_idx } = self.cursor
                        && token_idx == idx
                    {
                        cursor_byte = token_start;
                        found_cursor = true;
                    }
                    out.push_str(&sigil);
                }
                InputToken::Skill { name, .. } => {
                    let sigil = format!("/{}", name);
                    if !found_cursor
                        && let InputCursor::AtAttachment { token_idx } = self.cursor
                        && token_idx == idx
                    {
                        cursor_byte = token_start;
                        found_cursor = true;
                    }
                    out.push_str(&sigil);
                }
            }
        }

        if !found_cursor {
            cursor_byte = out.len();
        }

        (out, cursor_byte)
    }

    /// Replaces all tokens with a single text token containing `text`, placing the cursor at the end.
    pub fn set_text(&mut self, text: String) {
        let len = text.len();
        self.tokens = vec![InputToken::Text(text)];
        self.cursor = InputCursor::InText {
            token_idx: 0,
            byte_offset: len,
        };
    }

    /// Merges adjacent `Text` tokens and ensures the sequence starts and ends with a `Text` token.
    fn normalize(&mut self) {
        // Merge adjacent text tokens.
        let mut i = 0;
        while i + 1 < self.tokens.len() {
            if matches!(
                (&self.tokens[i], &self.tokens[i + 1]),
                (InputToken::Text(_), InputToken::Text(_))
            ) {
                let next = if let InputToken::Text(t) = self.tokens.remove(i + 1) {
                    t
                } else {
                    unreachable!()
                };
                if let InputToken::Text(ref mut cur) = self.tokens[i] {
                    // Update cursor byte_offset if it pointed into the merged token.
                    if let InputCursor::InText {
                        token_idx,
                        ref mut byte_offset,
                    } = self.cursor
                        && token_idx == i + 1
                    {
                        self.cursor = InputCursor::InText {
                            token_idx: i,
                            byte_offset: cur.len() + *byte_offset,
                        };
                    }
                    cur.push_str(&next);
                }
            } else {
                i += 1;
            }
        }

        // Ensure sequence starts with a Text token.
        if !matches!(self.tokens.first(), Some(InputToken::Text(_))) {
            self.tokens.insert(0, InputToken::Text(String::new()));
            // Shift cursor indices.
            match &mut self.cursor {
                InputCursor::InText { token_idx, .. } => *token_idx += 1,
                InputCursor::AtAttachment { token_idx } => *token_idx += 1,
            }
        }

        // Ensure sequence ends with a Text token.
        if !matches!(self.tokens.last(), Some(InputToken::Text(_))) {
            self.tokens.push(InputToken::Text(String::new()));
        }
    }
}

impl CommandPicker {
    /// Recomputes `self.filtered` from the current filter string.
    ///
    /// Call this after any mutation to `filter` or `commands`. When the filter is empty all
    /// commands are included in their original order. Otherwise entries are sorted by
    /// descending nucleo score and non-matching entries are excluded.
    pub fn refilter(&mut self) {
        if self.filter.is_empty() {
            self.filtered = self.commands.clone();
            return;
        }

        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::new(
            &self.filter,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );

        let mut buf = Vec::new();
        let mut scored: Vec<(u32, CommandEntry)> = self
            .commands
            .iter()
            .filter_map(|cmd| {
                let mut indices = Vec::new();
                let score = pattern.indices(
                    nucleo_matcher::Utf32Str::new(&cmd.name, &mut buf),
                    &mut matcher,
                    &mut indices,
                )?;
                indices.sort_unstable();
                let mut entry = cmd.clone();
                entry.indices = indices;
                Some((score, entry))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        self.filtered = scored.into_iter().map(|(_, cmd)| cmd).collect();
    }
}

impl ModelPickerView {
    /// Recomputes `self.filtered` from the current filter string.
    ///
    /// Call this after any mutation to `filter` or `models`. When the filter is empty all
    /// models are included in their original order. Otherwise entries are sorted by
    /// descending nucleo score and non-matching entries are excluded.
    pub fn refilter(&mut self) {
        let active = &self.active_selection;

        if self.filter.is_empty() {
            self.filtered = self
                .models
                .iter()
                .map(|s| ModelEntry {
                    is_active: active.as_ref() == Some(s),
                    selection: s.clone(),
                    indices: Vec::new(),
                })
                .collect();
            return;
        }

        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::new(
            &self.filter,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );

        let mut buf = Vec::new();
        let mut scored: Vec<(u32, ModelEntry)> = self
            .models
            .iter()
            .filter_map(|s| {
                let mut indices = Vec::new();
                let score = pattern.indices(
                    nucleo_matcher::Utf32Str::new(s.model_id.as_str(), &mut buf),
                    &mut matcher,
                    &mut indices,
                )?;
                indices.sort_unstable();
                Some((
                    score,
                    ModelEntry {
                        is_active: active.as_ref() == Some(s),
                        selection: s.clone(),
                        indices,
                    },
                ))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        self.filtered = scored.into_iter().map(|(_, e)| e).collect();
    }
}
