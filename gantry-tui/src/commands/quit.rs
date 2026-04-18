use crate::commands::{Command, CommandContext};
use crate::message::Msg;

pub struct Quit;

impl Command for Quit {
    fn name(&self) -> &'static str {
        "quit"
    }

    fn description(&self) -> &'static str {
        "Quit the application"
    }

    fn execute(&self, ctx: CommandContext) {
        let _ = ctx.msg_tx.blocking_send(Msg::Quit);
    }
}
