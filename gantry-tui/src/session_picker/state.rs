use gantry_core::{SessionId, SessionInfo};

/// State for the sessions browser overlay.
pub struct SessionPickerState {
    pub sessions: Vec<SessionInfo>,
    /// Index of the highlighted row.
    pub selected_idx: usize,
    /// The session that was active when the browser was opened.
    pub active_session_id: SessionId,
}
