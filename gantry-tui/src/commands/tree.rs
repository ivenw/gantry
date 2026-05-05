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
        let tx = ctx.msg_tx;
        let handle = ctx.handle.clone();
        ctx.rt_handle.spawn(async move {
            match handle.get_tree().await {
                Some(tree) => {
                    let _ = tx.send(Msg::OpenTreeView(tree)).await;
                }
                None => {
                    let _ = tx.send(Msg::SetStatus("No messages yet".into())).await;
                }
            }
        });
    }
}
