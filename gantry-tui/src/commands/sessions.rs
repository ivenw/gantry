use super::{Command, CommandContext};
use crate::message::Msg;

pub struct Sessions;

impl Command for Sessions {
    /// Opens the sessions browser overlay with the full session list.
    fn execute(&self, ctx: CommandContext) {
        let tx = ctx.msg_tx;
        let app = ctx.app.clone();
        ctx.rt_handle.spawn(async move {
            let app = app.lock().await;
            match app.list_sessions() {
                Ok(sessions) => {
                    let active_id = app.session_id().clone();
                    let _ = tx.send(Msg::OpenSessionsView(sessions, active_id)).await;
                }
                Err(e) => {
                    let _ = tx.send(Msg::SetStatus(format!("failed to list sessions: {e}"))).await;
                }
            }
        });
    }
}
