pub mod health;
pub mod new;
pub mod quit;
pub mod tree;

use crate::message::Msg;
use gantry_core::{ChatService, SessionHandle, SessionManager};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::mpsc::Sender;

pub struct CommandContext {
    pub handle: Arc<SessionHandle>,
    pub chat_service: Arc<ChatService>,
    pub session_manager: Arc<SessionManager>,
    pub project_path: PathBuf,
    pub msg_tx: Sender<Msg>,
    pub rt_handle: Handle,
}

pub trait Command: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn execute(&self, ctx: CommandContext);
}

pub fn all_commands() -> Vec<Box<dyn Command>> {
    vec![
        Box::new(health::Health),
        Box::new(new::New),
        Box::new(quit::Quit),
        Box::new(tree::Tree),
    ]
}
