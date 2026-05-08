use super::{Command, CommandContext};
use crate::message::Msg;

pub struct Model;

impl Command for Model {
    /// Opens the model picker listing all available models across configured providers.
    fn execute(&self, ctx: CommandContext) {
        let tx = ctx.msg_tx;
        let app = ctx.app.clone();
        ctx.rt_handle.spawn(async move {
            match app.lock().await.list_models().await {
                Ok(selections) => {
                    let _ = tx.send(Msg::OpenModelPicker(selections)).await;
                }
                Err(e) => {
                    let _ = tx.send(Msg::SetStatus(format!("Failed to list models: {e}"))).await;
                }
            }
        });
    }
}
