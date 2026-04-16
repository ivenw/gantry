use super::{Command, CommandContext};
use crate::message::Msg;
use std::sync::Arc;

pub struct New;

impl Command for New {
    fn name(&self) -> &'static str {
        "new"
    }

    fn description(&self) -> &'static str {
        "Start a new session"
    }

    fn execute(&self, ctx: CommandContext) {
        match ctx.client {
            None => {
                let _ = ctx.msg_tx.try_send(Msg::SetStatus("Not connected".into()));
            }
            Some(client) => {
                let project_path = ctx.project_path;
                let tx = ctx.msg_tx;
                ctx.rt_handle.spawn(async move {
                    let Ok(session_id) = client.create_session(project_path.clone()).await else {
                        return;
                    };
                    if client
                        .bind_session(session_id.clone(), project_path.clone())
                        .await
                        .is_err()
                    {
                        return;
                    }
                    let Ok((event_handle, event_rx)) = client.subscribe_events().await else {
                        return;
                    };
                    let new_client = Arc::new((*client).clone());
                    let _ = tx
                        .send(Msg::NewSession {
                            client: new_client,
                            session_id,
                            event_handle,
                            event_rx,
                        })
                        .await;
                });
            }
        }
    }
}
