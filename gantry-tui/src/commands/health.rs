use super::{Command, CommandContext, CommandEffect};
use std::sync::mpsc;

pub struct Health;

impl Command for Health {
    fn name(&self) -> &'static str {
        "health"
    }

    fn description(&self) -> &'static str {
        "Check connection to server"
    }

    fn execute(&self, ctx: CommandContext, tx: mpsc::Sender<CommandEffect>) {
        match ctx.client {
            None => {
                let _ = tx.send(CommandEffect::Status("Not connected".into()));
            }
            Some(client) => {
                ctx.rt.spawn(async move {
                    let start = std::time::Instant::now();
                    let effect = match client.ping().await {
                        Ok(_) => CommandEffect::Status(format!(
                            "Connected: {}ms",
                            start.elapsed().as_millis()
                        )),
                        Err(e) => CommandEffect::Status(format!("Ping failed: {}", e)),
                    };
                    let _ = tx.send(effect);
                });
            }
        }
    }
}
