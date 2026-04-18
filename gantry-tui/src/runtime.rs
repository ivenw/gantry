use anyhow::Result;
use crossterm::event::{Event, KeyEventKind};
use gantry_rpc::{JsonRpcClient, WsConnectionEvent};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::task::JoinHandle;

use crate::connection;
use crate::message::Msg;
use crate::model::{ConnectionState, Model};
use crate::update::update;
use crate::views;

pub struct Runtime {
    model: Model,
    rt: tokio::runtime::Runtime,
    msg_tx: Sender<Msg>,
    msg_rx: Receiver<Msg>,
    // Connection config — used by Runtime to spawn connections, not needed by Model/update/views
    addr: String,
    port: u16,
    project_path: PathBuf,
    // Live async handles
    client: Option<Arc<JsonRpcClient>>,
    event_task: Option<JoinHandle<()>>,
    stream_task: Option<JoinHandle<()>>,
    reconnect_pending: bool,
}

impl Runtime {
    pub fn new(addr: String, port: u16, project_path: PathBuf) -> Result<Self> {
        let rt = tokio::runtime::Runtime::new()?;
        let (msg_tx, msg_rx) = channel::<Msg>(256);

        let mut model = Model::new();

        let (client, event_task) = if let Some((client, session_id, event_handle, event_rx)) =
            rt.block_on(connection::try_connect_async(&addr, port, &project_path))
        {
            model.connection_state = ConnectionState::Connected;
            model.session_id = session_id;
            let client = Arc::new(client);
            let event_task = spawn_ws_forwarder_inner(&rt, event_rx, msg_tx.clone());
            drop(event_handle);
            (Some(client), Some(event_task))
        } else {
            model.status_message = Some("Disconnected \u{2014} reconnecting...".into());
            (None, None)
        };

        let mut runtime = Self {
            model,
            rt,
            msg_tx,
            msg_rx,
            addr,
            port,
            project_path,
            client,
            event_task,
            stream_task: None,
            reconnect_pending: false,
        };

        if runtime.client.is_none() {
            runtime.spawn_reconnect();
        }

        Ok(runtime)
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        terminal.draw(|f| views::render(f, &self.model))?;

        loop {
            if crossterm::event::poll(Duration::from_millis(10))?
                && let Event::Key(key) = crossterm::event::read()?
                && key.kind == KeyEventKind::Press
            {
                let _ = self.msg_tx.try_send(Msg::Key(key));
            }

            let mut needs_redraw = false;
            while let Ok(msg) = self.msg_rx.try_recv() {
                needs_redraw = true;
                if self.dispatch(msg) {
                    return Ok(());
                }
            }

            if needs_redraw {
                terminal.draw(|f| views::render(f, &self.model))?;
            }
        }
    }

    fn dispatch(&mut self, msg: Msg) -> bool {
        let is_quit = matches!(msg, Msg::Quit);
        let chained = self.process(msg);
        if let Some(chained) = chained {
            self.process(chained);
        }
        is_quit
    }

    fn process(&mut self, msg: Msg) -> Option<Msg> {
        match msg {
            Msg::ReconnectSuccess {
                client,
                session_id,
                event_handle,
                event_rx,
                clear_messages,
            } => {
                if let Some(old) = self.event_task.take() {
                    old.abort();
                }
                event_handle.abort();
                self.client = Some(Arc::new(client));
                self.reconnect_pending = false;
                self.spawn_ws_forwarder(event_rx);
                return update(
                    &mut self.model,
                    Msg::ReconnectSuccess {
                        client: (**self.client.as_ref().unwrap()).clone(),
                        session_id,
                        event_handle: self.rt.spawn(async {}),
                        event_rx: tokio::sync::mpsc::channel(1).1,
                        clear_messages,
                    },
                );
            }
            Msg::NewSession {
                client,
                session_id,
                event_handle,
                event_rx,
            } => {
                if let Some(old) = self.event_task.take() {
                    old.abort();
                }
                event_handle.abort();
                self.client = Some(Arc::clone(&client));
                self.spawn_ws_forwarder(event_rx);
                return update(
                    &mut self.model,
                    Msg::NewSession {
                        client,
                        session_id,
                        event_handle: self.rt.spawn(async {}),
                        event_rx: tokio::sync::mpsc::channel(1).1,
                    },
                );
            }
            Msg::ExecuteCommand(ref cmd) => {
                self.execute_command(cmd.clone());
                return None;
            }
            Msg::InterruptStream => {
                self.spawn_interrupt_stream();
                self.model.chat.finish_streaming();
                return None;
            }
            Msg::SendMessage(ref input) => {
                self.spawn_send_message(input.clone());
            }
            Msg::WsDisconnected => {
                if let Some(task) = self.stream_task.take() {
                    task.abort();
                }
                if !self.reconnect_pending {
                    self.client = None;
                    self.spawn_reconnect();
                }
            }
            _ => {}
        }
        update(&mut self.model, msg)
    }

    fn spawn_ws_forwarder(&mut self, event_rx: Receiver<WsConnectionEvent>) {
        if let Some(old) = self.event_task.take() {
            old.abort();
        }
        let task = spawn_ws_forwarder_inner(&self.rt, event_rx, self.msg_tx.clone());
        self.event_task = Some(task);
    }

    fn spawn_send_message(&mut self, input: String) {
        if let Some(old) = self.stream_task.take() {
            old.abort();
        }
        let tx = self.msg_tx.clone();
        let addr = self.addr.clone();
        let port = self.port;
        let session_id = self.model.session_id.clone();
        let project_path = self.project_path.clone();

        let task = self.rt.spawn(async move {
            let result = match JsonRpcClient::connect_ws(&addr, port).await {
                Ok(client) => {
                    if let Err(e) = client.bind_session(session_id, project_path).await {
                        Err(format!("failed to connect session: {}", e))
                    } else {
                        client
                            .stream_message(input)
                            .await
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }
                }
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(Msg::StreamResult(result)).await;
        });
        self.stream_task = Some(task);
    }

    fn spawn_interrupt_stream(&mut self) {
        if let Some(task) = self.stream_task.take() {
            task.abort();
        }
        let pending_id = match self.model.chat.pending_message_id.clone() {
            Some(id) => id,
            None => return,
        };
        let addr = self.addr.clone();
        let port = self.port;
        let session_id = self.model.session_id.clone();
        let project_path = self.project_path.clone();

        self.rt.spawn(async move {
            if let Ok(client) = JsonRpcClient::connect_ws(&addr, port).await
                && client.bind_session(session_id, project_path).await.is_ok()
            {
                let _ = client.interrupt_stream(pending_id).await;
            }
        });
    }

    fn spawn_reconnect(&mut self) {
        self.reconnect_pending = true;
        let addr = self.addr.clone();
        let port = self.port;
        let project_path = self.project_path.clone();
        let tx = self.msg_tx.clone();

        self.rt.spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if let Some((client, session_id, event_handle, event_rx)) =
                    connection::try_connect_async(&addr, port, &project_path).await
                {
                    let _ = tx
                        .send(Msg::ReconnectSuccess {
                            client,
                            session_id,
                            event_handle,
                            event_rx,
                            clear_messages: false,
                        })
                        .await;
                    return;
                }
            }
        });
    }

    fn execute_command(&mut self, cmd: std::sync::Arc<dyn crate::commands::Command>) {
        let ctx = crate::commands::CommandContext {
            client: self.client.clone(),
            project_path: self.project_path.clone(),
            msg_tx: self.msg_tx.clone(),
            rt_handle: self.rt.handle().clone(),
        };
        cmd.execute(ctx);
    }
}

fn spawn_ws_forwarder_inner(
    rt: &tokio::runtime::Runtime,
    mut event_rx: Receiver<WsConnectionEvent>,
    tx: Sender<Msg>,
) -> JoinHandle<()> {
    rt.spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            let msg = match ev {
                WsConnectionEvent::Event(e) => Msg::AppEvent(e),
                WsConnectionEvent::Disconnected => Msg::WsDisconnected,
                WsConnectionEvent::Error(e) => Msg::WsError(e),
            };
            if tx.send(msg).await.is_err() {
                break;
            }
        }
    })
}
