use gantry_contract::{FormState, Message, PendingMessage};

#[derive(Default, Clone)]
pub struct ConversationState {
    pub messages: Vec<Message>,
    pub pending_message: Option<PendingMessage>,
    pub active_form: Option<FormState>,
}
