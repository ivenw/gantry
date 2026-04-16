pub mod health;
pub mod new;

use crate::message::Msg;
use gantry_rpc::JsonRpcClient;
use std::sync::Arc;
use tokio::runtime::Handle;

pub struct CommandContext {
    pub client: Option<Arc<JsonRpcClient>>,
    pub project_path: std::path::PathBuf,
    pub msg_tx: tokio::sync::mpsc::Sender<Msg>,
    pub rt_handle: Handle,
}

pub trait Command: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn execute(&self, ctx: CommandContext);
}

pub fn all_commands() -> Vec<Box<dyn Command>> {
    vec![Box::new(health::Health), Box::new(new::New)]
}
