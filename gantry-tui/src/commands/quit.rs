use super::{Command, CommandContext};
use crate::message::Msg;

pub struct Quit;

impl Command for Quit {
    fn execute(&self, ctx: CommandContext) {
        let _ = ctx.msg_tx.blocking_send(Msg::Quit);
    }
}
