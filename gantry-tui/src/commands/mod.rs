pub mod health;
pub mod new;

use crate::views::app::AppView;
use gantry_rpc::{JsonRpcClient, WsConnectionEvent};
use std::sync::{Arc, Mutex, mpsc};
use tokio::runtime::Runtime;

pub struct CommandContext<'a> {
    pub rt: &'a Runtime,
    pub client: Option<Arc<JsonRpcClient>>,
    pub project_path: std::path::PathBuf,
}

pub struct AppEffectContext<'a> {
    pub app: &'a mut AppView,
    pub client: &'a mut Option<Arc<JsonRpcClient>>,
    pub session_id: &'a Arc<Mutex<String>>,
    pub event_handle: &'a mut tokio::task::JoinHandle<()>,
    pub event_rx: &'a mut tokio::sync::mpsc::Receiver<WsConnectionEvent>,
}

pub enum CommandEffect {
    Status(String),
    Apply(Box<dyn FnOnce(&mut AppEffectContext) + Send>),
}

pub trait Command: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn execute(&self, ctx: CommandContext, tx: mpsc::Sender<CommandEffect>);
}

pub fn all_commands() -> Vec<Box<dyn Command>> {
    vec![Box::new(health::Health), Box::new(new::New)]
}
