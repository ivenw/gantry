pub mod app;
pub mod chat;
pub mod command_picker;
pub mod input;
pub mod model_picker;
pub mod providers;
pub mod sessions;
pub mod status_message;
pub mod statusline;
pub mod tree;

pub use app::render;

use chat::ChatViewState;
use statusline::StatuslineViewState;

#[derive(Default)]
pub struct ViewState {
    pub chat: ChatViewState,
    pub statusline: StatuslineViewState,
}
