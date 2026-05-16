//! Feature vertical modules, each containing state, widget, and supporting types.

pub mod agent_statusline;
pub mod app_statusline;
pub mod attachment_picker;
pub mod chat;
pub mod command_picker;
pub mod input;
pub mod model_picker;
pub mod provider_config;
pub mod session_picker;
pub mod tree;
pub mod usage;

// Re-export commonly used types at the feature level for ergonomic access.
pub use agent_statusline::{AgentStatuslineWidget, AgentStatuslineWidgetState};
pub use app_statusline::AppStatuslineWidget;
pub use attachment_picker::{AttachmentPickerKind, AttachmentPickerState, AttachmentPickerWidget};
pub use chat::{ChatMessage, ChatState, ChatWidgetState};
pub use command_picker::{CommandPickerState, CommandPickerWidget, KnownCommand};
pub use input::{InputState, InputWidget, prev_char_boundary};
pub use model_picker::{ModelPickerState, ModelPickerWidget, format_context_length};
pub use provider_config::{
    CopilotAuthKind, ProviderConfigWidget, ProviderWizard, ProvidersConfigState, ProvidersSubView,
    WizardProviderKind,
};
pub use session_picker::{SessionPickerState, SessionPickerWidget};
pub use tree::{TreeState, TreeWidget, branch_rows};
pub use usage::{UsageState, UsageWidget};
