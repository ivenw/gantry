use super::{Command, CommandContext};
use crate::message::Msg;

pub struct Health;

impl Command for Health {
    fn name(&self) -> &'static str {
        "health"
    }

    fn description(&self) -> &'static str {
        "Check connection to server"
    }

    fn execute(&self, ctx: CommandContext) {
        match ctx.client {
            None => {
                let _ = ctx.msg_tx.try_send(Msg::SetStatus("Not connected".into()));
            }
            Some(client) => {
                let tx = ctx.msg_tx;
                ctx.rt_handle.spawn(async move {
                    let start = std::time::Instant::now();
                    let msg = match client.ping().await {
                        Ok(_) => {
                            Msg::SetStatus(format!("Connected: {}ms", start.elapsed().as_millis()))
                        }
                        Err(e) => Msg::SetStatus(format!("Ping failed: {}", e)),
                    };
                    let _ = tx.send(msg).await;
                });
            }
        }
    }
}
