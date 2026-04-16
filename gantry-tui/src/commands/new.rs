use super::{Command, CommandContext, CommandEffect};
use std::sync::{Arc, mpsc};

pub struct New;

impl Command for New {
    fn name(&self) -> &'static str {
        "new"
    }

    fn description(&self) -> &'static str {
        "Start a new session"
    }

    fn execute(&self, ctx: CommandContext, tx: mpsc::Sender<CommandEffect>) {
        match ctx.client {
            None => {
                let _ = tx.send(CommandEffect::Status("Not connected".into()));
            }
            Some(client) => {
                let project_path = ctx.project_path;
                ctx.rt.spawn(async move {
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
                    let new_client = (*client).clone();
                    let _ = tx.send(CommandEffect::Apply(Box::new(move |ctx| {
                        ctx.event_handle.abort();
                        *ctx.event_handle = event_handle;
                        *ctx.event_rx = event_rx;
                        *ctx.session_id.lock().unwrap() = session_id;
                        *ctx.client = Some(Arc::new(new_client));
                        ctx.app.connected = true;
                        ctx.app.reset_for_new_session();
                        ctx.app.status_message = None;
                    })));
                });
            }
        }
    }
}
