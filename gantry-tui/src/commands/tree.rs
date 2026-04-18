use super::{Command, CommandContext};
use crate::message::Msg;

pub struct Tree;

impl Command for Tree {
    fn name(&self) -> &'static str {
        "tree"
    }

    fn description(&self) -> &'static str {
        "Browse the message tree"
    }

    fn execute(&self, ctx: CommandContext) {
        match ctx.client {
            None => {
                let _ = ctx.msg_tx.try_send(Msg::SetStatus("Not connected".into()));
            }
            Some(client) => {
                let tx = ctx.msg_tx;
                ctx.rt_handle.spawn(async move {
                    match client.get_tree().await {
                        Ok(nodes) => {
                            let _ = tx.send(Msg::OpenTreeView(nodes)).await;
                        }
                        Err(e) => {
                            let _ = tx.send(Msg::SetStatus(e.to_string())).await;
                        }
                    }
                });
            }
        }
    }
}
