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
