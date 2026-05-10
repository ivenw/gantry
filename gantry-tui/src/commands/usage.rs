use super::{Command, CommandContext};
use crate::message::Msg;

pub struct Usage;

impl Command for Usage {
    /// Opens the context window usage overlay, or sets a status message if no data is available.
    fn execute(&self, ctx: CommandContext) {
        let tx = ctx.msg_tx;
        let app = ctx.app.clone();
        ctx.rt_handle.spawn(async move {
            let guard = app.lock().await;
            match guard.context_window() {
                Some(cw) => {
                    let consumption = guard.total_consumption().clone();
                    drop(guard);
                    let _ = tx.send(Msg::OpenUsageView(cw, consumption)).await;
                }
                None => {
                    drop(guard);
                    let _ = tx
                        .send(Msg::SetStatus(
                            "no context window data yet — send a message first".to_string(),
                        ))
                        .await;
                }
            }
        });
    }
}
