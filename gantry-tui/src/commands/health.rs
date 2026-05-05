use super::{Command, CommandContext};
use crate::message::Msg;

pub struct Health;

impl Command for Health {
    fn name(&self) -> &'static str {
        "health"
    }

    fn description(&self) -> &'static str {
        "Show session status"
    }

    fn execute(&self, ctx: CommandContext) {
        let project_path = ctx.rt_handle.block_on(async {
            ctx.app.lock().await.project_path.display().to_string()
        });
        let _ = ctx
            .msg_tx
            .try_send(Msg::SetStatus(format!("Session active: {}", project_path)));
    }
}
