use std::path::PathBuf;

use gantry_tools::DiffHunk;

use crate::metrics::{ContextWindow, Usage};

/// Out-of-band events emitted by tools and consumed by the TUI or other subscribers.
///
/// These are separate from the agent's tool result so subscribers receive rich data
/// (e.g. diff hunks) without the agent seeing it in its context.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Diff produced by a successful edit operation, keyed by the edited file path.
    EditDiff { path: PathBuf, hunks: Vec<DiffHunk> },
    /// Emitted after each completed assistant turn with updated token metrics.
    MetricsUpdated {
        context_window: Option<ContextWindow>,
        total_consumption: Usage,
    },
}
