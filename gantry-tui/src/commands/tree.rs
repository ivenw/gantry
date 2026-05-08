use super::{Command, CommandContext};
use crate::message::Msg;

pub struct Tree;

impl Command for Tree {
    fn execute(&self, ctx: CommandContext) {
        let tx = ctx.msg_tx;
        let app = ctx.app.clone();
        ctx.rt_handle.spawn(async move {
            match app.lock().await.get_tree() {
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
