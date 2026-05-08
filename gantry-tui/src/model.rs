use gantry_core::{Branch, ModelSelection, ProviderAlias, ProviderConfig, SessionId, SessionTree, StoredCredential, UserId};

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
    pub providers_view: Option<ProvidersView>,
    pub model_picker_view: Option<ModelPickerView>,
    pub status_message: Option<String>,
}

pub struct TreeView {
    pub tree: SessionTree,
    /// Index into the DFS row order of the currently highlighted row.
    pub selected_idx: usize,
    /// First visible row index (scroll offset).
    pub scroll_offset: usize,
}

/// Top-level state for the providers overlay.
pub struct ProvidersView {
    pub providers: Vec<ProviderConfig>,
    pub sub: ProvidersSubView,
}

/// Which sub-screen of the providers overlay is active.
pub enum ProvidersSubView {
    /// The list of configured providers with add/remove actions.
    List { selected_idx: usize },
    /// Picking which provider type to add.
    TypePicker { selected_idx: usize },
    /// Picking the authentication method for GitHub Copilot.
    CopilotAuthPicker { selected_idx: usize },
    /// Filling in the fields for a new provider.
    Wizard(ProviderWizard),
}

/// State for the model picker overlay.
pub struct ModelPickerView {
    pub models: Vec<ModelSelection>,
    /// Index of the cursor row (keyboard highlight).
    pub selected_idx: usize,
    /// The model that was active when the picker was opened, used to mark the current selection.
    pub active_selection: Option<ModelSelection>,
}

/// A field in the provider wizard — a label, an editable string value, and whether it is required.
pub struct WizardField {
    pub label: &'static str,
    pub value: String,
    pub required: bool,
}

impl WizardField {
    pub fn required(label: &'static str) -> Self {
        Self { label, value: String::new(), required: true }
    }

    pub fn optional(label: &'static str) -> Self {
        Self { label, value: String::new(), required: false }
    }
}

/// Authentication method chosen for GitHub Copilot.
///
/// # How Copilot auth works
///
/// Accessing `api.githubcopilot.com` requires a short-lived Copilot token obtained by
/// exchanging a GitHub OAuth access token against `api.github.com/copilot_internal/v2/token`.
/// Not all OAuth tokens satisfy that endpoint — it requires a token issued by the GitHub
/// Copilot VS Code extension's registered OAuth app (client ID `Iv1.b507a08c87ecfe98`).
/// Tokens from `gh auth login` (even with the `copilot` scope) are issued under the gh CLI's
/// client ID and are rejected with 404.
///
/// The correct flow (used by tools like opencode and pi) is the OAuth device code flow:
/// 1. POST `github.com/login/device/code` with `client_id=Iv1.b507a08c87ecfe98&scope=read:user`
/// 2. Show the user the returned `user_code` and `verification_uri`
/// 3. Poll `github.com/login/oauth/access_token` until the user completes auth in a browser
/// 4. Store the resulting access token — it passes the `copilot_internal/v2/token` exchange
///
/// The `GhCli` variant currently calls `gh auth token`, which will only work if the user has
/// re-authed with the device code flow above. This variant should be replaced with an in-app
/// device code flow (TODO).
///
/// The `ApiKey` variant is reserved for Copilot API keys (Business/Enterprise plans only);
/// individual plan subscribers cannot generate these from `github.com/settings/copilot`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CopilotAuthKind {
    /// Obtain an OAuth token from the GitHub CLI (`gh auth token`).
    GhCli,
    /// Supply a GitHub Copilot API key directly (Business/Enterprise plans only).
    ApiKey,
}

impl CopilotAuthKind {
    /// Human-readable label shown in the auth picker.
    pub const fn label(self) -> &'static str {
        match self {
            Self::GhCli => "GitHub CLI (OAuth)",
            Self::ApiKey => "API Key",
        }
    }

    pub const ALL: &'static [CopilotAuthKind] = &[Self::GhCli, Self::ApiKey];
}

/// The provider type being configured in the wizard.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WizardProviderKind {
    Ollama,
    Copilot,
    OpenAiCompletions,
    OpenAiResponses,
}

impl WizardProviderKind {
    /// Human-readable label shown in the type picker.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama",
            Self::Copilot => "GitHub Copilot",
            Self::OpenAiCompletions => "OpenAI Completions",
            Self::OpenAiResponses => "OpenAI Responses",
        }
    }

    pub const ALL: &'static [WizardProviderKind] = &[
        Self::Ollama,
        Self::Copilot,
        Self::OpenAiCompletions,
        Self::OpenAiResponses,
    ];
}

/// State for filling in fields for a new provider.
pub struct ProviderWizard {
    pub kind: WizardProviderKind,
    /// Authentication method for Copilot; `None` for non-Copilot providers.
    pub copilot_auth: Option<CopilotAuthKind>,
    /// Editable fields, followed by a virtual Confirm entry (always last).
    pub fields: Vec<WizardField>,
    /// Index of the currently focused row (field or the confirm entry).
    pub focused_idx: usize,
    /// Cursor byte-offset within the focused field's value string.
    pub cursor: usize,
    /// Error message shown when the user attempts to confirm with invalid state.
    pub error: Option<String>,
}

impl ProviderWizard {
    /// Builds a wizard pre-populated with the correct fields for `kind`.
    ///
    /// For `WizardProviderKind::Copilot`, `copilot_auth` selects the credential flow.
    pub fn new(kind: WizardProviderKind, copilot_auth: Option<CopilotAuthKind>) -> Self {
        let fields = match kind {
            WizardProviderKind::Ollama => vec![
                WizardField::required("Alias"),
                WizardField::optional("Base URL"),
            ],
            WizardProviderKind::Copilot => match copilot_auth {
                Some(CopilotAuthKind::ApiKey) => vec![
                    WizardField::required("Alias"),
                    WizardField::required("API Key"),
                ],
                _ => vec![
                    WizardField::required("Alias"),
                    // Token is obtained from `gh auth token` on confirm.
                ],
            },
            WizardProviderKind::OpenAiCompletions => vec![
                WizardField::required("Alias"),
                WizardField::required("Base URL"),
                WizardField::required("API Key"),
            ],
            WizardProviderKind::OpenAiResponses => vec![
                WizardField::required("Alias"),
                WizardField::required("Base URL"),
                WizardField::required("API Key"),
            ],
        };
        Self { kind, copilot_auth, fields, focused_idx: 0, cursor: 0, error: None }
    }

    /// Returns the number of rows in the wizard (fields + confirm).
    pub fn row_count(&self) -> usize {
        self.fields.len() + 1
    }

    /// Returns true if the focused row is the Confirm entry.
    pub fn is_on_confirm(&self) -> bool {
        self.focused_idx == self.fields.len()
    }

    /// Validates fields and builds the `ProviderConfig` and optional `StoredCredential`.
    ///
    /// Returns an error string if any required field is empty.
    pub fn build(&self) -> Result<(ProviderConfig, Option<StoredCredential>), String> {
        for f in &self.fields {
            if f.required && f.value.trim().is_empty() {
                return Err(format!("'{}' is required", f.label));
            }
        }

        let alias = ProviderAlias::new(self.fields[0].value.trim());

        match self.kind {
            WizardProviderKind::Ollama => {
                let base_url = self.fields[1].value.trim();
                let config = ProviderConfig::Ollama(gantry_core::OllamaProviderConfig {
                    alias,
                    base_url: if base_url.is_empty() { None } else { Some(base_url.to_string()) },
                });
                Ok((config, None))
            }
            WizardProviderKind::Copilot => {
                let config = ProviderConfig::Copilot(gantry_core::CopilotProviderConfig { alias });
                let credential = match self.copilot_auth {
                    Some(CopilotAuthKind::ApiKey) => {
                        let api_key = self.fields[1].value.trim().to_string();
                        StoredCredential::ApiKey { value: api_key }
                    }
                    _ => {
                        let token = obtain_gh_token()?;
                        // TODO: implement token refresh via `gh auth token` when the access token
                        // expires, rather than storing a refresh_token or expires_at.
                        StoredCredential::OauthToken {
                            access_token: token,
                            refresh_token: String::new(),
                            expires_at: String::new(),
                        }
                    }
                };
                Ok((config, Some(credential)))
            }
            WizardProviderKind::OpenAiCompletions => {
                let base_url = self.fields[1].value.trim().to_string();
                let api_key = self.fields[2].value.trim().to_string();
                let config = ProviderConfig::OpenAiCompletions(gantry_core::OpenAiCompletionsProviderConfig {
                    alias,
                    base_url,
                });
                Ok((config, Some(StoredCredential::ApiKey { value: api_key })))
            }
            WizardProviderKind::OpenAiResponses => {
                let base_url = self.fields[1].value.trim().to_string();
                let api_key = self.fields[2].value.trim().to_string();
                let config = ProviderConfig::OpenAiResponses(gantry_core::OpenAiResponsesProviderConfig {
                    alias,
                    base_url,
                });
                Ok((config, Some(StoredCredential::ApiKey { value: api_key })))
            }
        }
    }
}

/// Invokes `gh auth token` and returns the trimmed token string.
///
/// Returns an error string if `gh` is not installed, not authenticated, or the command fails.
fn obtain_gh_token() -> Result<String, String> {
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "GitHub CLI (`gh`) not found — install it and run `gh auth login`".to_string()
            } else {
                format!("failed to run `gh auth token`: {e}")
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "`gh auth token` failed — run `gh auth login` first\n{}",
            stderr.trim()
        ));
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err("`gh auth token` returned an empty token — run `gh auth login`".to_string());
    }

    Ok(token)
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
            providers_view: None,
            model_picker_view: None,
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
            let count = picker.filtered_commands().len();
            if count > 0 {
                picker.selected_idx = picker.selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
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

    pub fn activate_model_picker_view(&mut self, models: Vec<ModelSelection>) {
        let active_selection = self.selection.clone();
        let selected_idx = active_selection
            .as_ref()
            .and_then(|s| models.iter().position(|m| m == s))
            .unwrap_or(0);
        self.model_picker_view = Some(ModelPickerView { models, selected_idx, active_selection });
    }

    pub fn deactivate_model_picker_view(&mut self) {
        self.model_picker_view = None;
    }

    pub fn move_model_picker_selection_up(&mut self) {
        if let Some(ref mut mv) = self.model_picker_view {
            if !mv.models.is_empty() {
                mv.selected_idx = mv.selected_idx.checked_sub(1).unwrap_or(mv.models.len() - 1);
            }
        }
    }

    pub fn move_model_picker_selection_down(&mut self) {
        if let Some(ref mut mv) = self.model_picker_view {
            if !mv.models.is_empty() {
                mv.selected_idx = (mv.selected_idx + 1) % mv.models.len();
            }
        }
    }

    /// Returns the currently highlighted model selection in the model picker, if any.
    pub fn selected_model_in_picker(&self) -> Option<&ModelSelection> {
        self.model_picker_view
            .as_ref()
            .and_then(|mv| mv.models.get(mv.selected_idx))
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
