use gantry_core::{SessionId, SessionInfo};

use crate::picker::Picker;

/// State for the sessions browser overlay.
pub struct SessionPickerState {
    pub picker: Picker<SessionInfo>,
    /// The session that was active when the browser was opened.
    pub active_session_id: SessionId,
    /// Maximum `first_message` display width across the full unfiltered list; stable for the lifetime of the picker.
    pub name_col_width: u16,
}

impl SessionPickerState {
    /// Creates a new `SessionPickerState` from the given session list and active session id.
    pub fn new(mut sessions: Vec<SessionInfo>, active_session_id: SessionId) -> Self {
        sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        // +2 for the "X " marker prefix prepended to each name in the widget.
        let name_col_width = sessions
            .iter()
            .map(|s| s.first_message.chars().count() as u16 + 2)
            .max()
            .unwrap_or(0);
        let picker = Picker::new(sessions, |s| s.first_message.as_str());
        Self {
            picker,
            active_session_id,
            name_col_width,
        }
    }
}
