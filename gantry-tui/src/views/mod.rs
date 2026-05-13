pub mod app;
pub mod table;

pub use app::render;

use crate::chat::ChatViewState;
use crate::statusline::AgentStatuslineState;

#[derive(Default)]
pub struct ViewState {
    pub chat: ChatViewState,
    pub agent_statusline: AgentStatuslineState,
}
