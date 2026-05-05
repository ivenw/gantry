pub mod health;
pub mod new;
pub mod quit;
pub mod tree;

use crate::message::Msg;
use gantry_core::App;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;

pub struct CommandContext {
    pub app: Arc<Mutex<App>>,
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
