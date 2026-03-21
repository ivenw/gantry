use crate::{FormState, Message, ModelSelection, PendingMessage};

#[derive(Clone)]
pub struct ConversationState {
    pub messages: Vec<Message>,
    pub pending_message: Option<PendingMessage>,
    pub active_form: Option<FormState>,
    pub active_selection: ModelSelection,
}

impl ConversationState {
    pub fn new(active_selection: ModelSelection) -> Self {
        Self {
            messages: Vec::new(),
            pending_message: None,
            active_form: None,
            active_selection,
        }
    }
}
