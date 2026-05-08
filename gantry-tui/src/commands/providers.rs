use super::{Command, CommandContext};
use crate::message::Msg;

pub struct Providers;

impl Command for Providers {
    /// Opens the providers overlay showing all currently configured providers.
    fn execute(&self, ctx: CommandContext) {
        let tx = ctx.msg_tx;
        let app = ctx.app.clone();
        ctx.rt_handle.spawn(async move {
            let providers = app.lock().await.list_providers().to_vec();
            let _ = tx.send(Msg::OpenProvidersView(providers)).await;
        });
    }
}
