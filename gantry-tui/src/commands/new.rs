use super::{Command, CommandContext};
use crate::message::Msg;

pub struct New;

impl Command for New {
    fn name(&self) -> &'static str {
        "new"
    }

    fn description(&self) -> &'static str {
        "Start a new session"
    }

    fn execute(&self, ctx: CommandContext) {
        let tx = ctx.msg_tx;
        ctx.rt_handle.spawn(async move {
            let _ = tx.send(Msg::NewSession).await;
        });
    }
}
