pub mod app;
pub mod status_message;
pub mod statusline;
pub mod table;

pub use app::render;

use crate::chat::ChatViewState;
use statusline::StatuslineViewState;

#[derive(Default)]
pub struct ViewState {
    pub chat: ChatViewState,
    pub statusline: StatuslineViewState,
}
