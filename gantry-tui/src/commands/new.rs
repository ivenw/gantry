use super::{Command, CommandContext};
use crate::message::Msg;

pub struct New;

impl Command for New {
    fn execute(&self, ctx: CommandContext) {
        let tx = ctx.msg_tx;
        ctx.rt_handle.spawn(async move {
            let _ = tx.send(Msg::NewSession).await;
        });
    }
}
