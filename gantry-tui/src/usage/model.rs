use gantry_core::{ContextWindow, Usage};

/// State for the context window usage overlay.
pub struct UsageView {
    pub context_window: ContextWindow,
    /// Accumulated token consumption across all nodes in the session.
    pub consumption: Usage,
}
