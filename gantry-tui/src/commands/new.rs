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
        let session_manager = ctx.session_manager.clone();
        let project_path = ctx.project_path.clone();
        let handle = ctx.handle.clone();
        ctx.rt_handle.spawn(async move {
            let selection = handle.get_active_selection().await;
            match session_manager
                .create_session(&project_path, selection)
                .await
            {
                Ok(session_id) => {
                    let _ = tx.send(Msg::NewSession(session_id)).await;
                }
                Err(e) => {
                    let _ = tx
                        .send(Msg::SetStatus(format!("Failed to create session: {}", e)))
                        .await;
                }
            }
        });
    }
}
