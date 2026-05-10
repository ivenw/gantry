use super::{Command, CommandContext};
use crate::message::Msg;

pub struct Usage;

impl Command for Usage {
    /// Opens the context window usage overlay, or sets a status message if no data is available.
    fn execute(&self, ctx: CommandContext) {
        let tx = ctx.msg_tx;
        let app = ctx.app.clone();
        ctx.rt_handle.spawn(async move {
            match app.lock().await.context_window() {
                Some(cw) => {
                    let _ = tx.send(Msg::OpenUsageView(cw)).await;
                }
                None => {
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
