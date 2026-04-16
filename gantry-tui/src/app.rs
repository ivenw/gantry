use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use gantry_core::{AppEvent, Message, Role};
use gantry_rpc::{JsonRpcClient, WsConnectionEvent};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::commands;
use crate::connection::{Connection, ReconnectSuccess};
use crate::views::AppView;

pub struct App {
    pub view: AppView,
    connection: Connection,
    rt: Runtime,
    pending_id: Arc<Mutex<Option<String>>>,
    stream_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    stream_result_tx: mpsc::Sender<Result<(), String>>,
    stream_result_rx: mpsc::Receiver<Result<(), String>>,
    effect_tx: mpsc::Sender<commands::CommandEffect>,
    effect_rx: mpsc::Receiver<commands::CommandEffect>,
    reconnect_tx: mpsc::SyncSender<ReconnectSuccess>,
    reconnect_rx: mpsc::Receiver<ReconnectSuccess>,
}

impl App {
    pub fn new(addr: String, port: u16, project_path: PathBuf) -> Result<Self> {
        let rt = tokio::runtime::Runtime::new()?;
        let (reconnect_tx, reconnect_rx) = mpsc::sync_channel::<ReconnectSuccess>(1);
        let (stream_result_tx, stream_result_rx) = mpsc::channel::<Result<(), String>>();
        let (effect_tx, effect_rx) = mpsc::channel::<commands::CommandEffect>();

        let mut view = AppView::new();

        let connection = if let Some((client, session_id, event_handle, event_rx)) =
            rt.block_on(Connection::try_connect_async(&addr, port, &project_path))
        {
            view.connected = true;
            Connection::new_connected(
                client,
                session_id,
                event_handle,
                event_rx,
                addr,
                port,
                project_path,
            )
        } else {
            view.set_status("Disconnected \u{2014} reconnecting...".to_string());
            let conn = Connection::new_disconnected(&rt, addr, port, project_path);
            conn.spawn_reconnect(&rt, reconnect_tx.clone());
            conn
        };

        Ok(Self {
            view,
            connection,
            rt,
            pending_id: Arc::new(Mutex::new(None)),
            stream_task: Arc::new(Mutex::new(None)),
            stream_result_tx,
            stream_result_rx,
            effect_tx,
            effect_rx,
            reconnect_tx,
            reconnect_rx,
        })
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        terminal.draw(|frame| self.view.render(frame))?;

        loop {
            // Process WebSocket events
            while let Ok(event) = self.connection.event_rx.try_recv() {
                match event {
                    WsConnectionEvent::Event(ev) => {
                        self.process_ws_event(ev);
                    }
                    WsConnectionEvent::Disconnected => {
                        if self.connection.is_connected() {
                            self.connection
                                .on_disconnected(&self.rt, &self.reconnect_tx);
                            self.view.connected = false;
                            self.view
                                .set_status("Disconnected \u{2014} reconnecting...".to_string());
                            if let Some(task) = self.stream_task.lock().unwrap().take() {
                                task.abort();
                            }
                            self.view.chat.finish_streaming();
                            *self.pending_id.lock().unwrap() = None;
                        }
                    }
                    WsConnectionEvent::Error(message) => {
                        self.view.add_error_message(message);
                    }
                }
                terminal.draw(|frame| self.view.render(frame))?;
            }

            // Process stream results
            while let Ok(result) = self.stream_result_rx.try_recv() {
                self.process_stream_result(result, terminal)?;
            }

            // Process command effects
            while let Ok(effect) = self.effect_rx.try_recv() {
                self.process_effect(effect);
                terminal.draw(|frame| self.view.render(frame))?;
            }

            // Process reconnect successes
            while let Ok(success) = self.reconnect_rx.try_recv() {
                let clear = success.clear_messages;
                self.connection.on_reconnect_success(success);
                self.view.connected = true;
                if clear {
                    self.view.reset_for_new_session();
                }
                self.view.status_message = None;
                terminal.draw(|frame| self.view.render(frame))?;
            }

            // Process keyboard input
            if crossterm::event::poll(std::time::Duration::from_millis(10))?
                && let Event::Key(key) = crossterm::event::read()?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if !self.handle_key(key, terminal)? {
                    break;
                }
                terminal.draw(|frame| self.view.render(frame))?;
            }
        }

        self.connection.event_handle.abort();
        Ok(())
    }

    fn process_ws_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Init(ev) => {
                self.view.chat.messages = ev.messages;
                if let Some(pending) = ev.pending_message {
                    self.view.chat.add_user_message(pending.content.clone());
                    self.view.chat.start_streaming_message();
                    *self.pending_id.lock().unwrap() = Some(pending.id.clone());
                }
                if ev.form.is_some() {
                    self.view.show_form();
                }
            }
            AppEvent::MessageReceived(ev) => {
                *self.pending_id.lock().unwrap() = Some(ev.id);
            }
            AppEvent::StreamStart(_) => {}
            AppEvent::Token(ev) => {
                self.view.chat.append_to_streaming(&ev.delta);
            }
            AppEvent::StreamEnd(_) => {
                self.view.chat.finish_streaming();
                *self.pending_id.lock().unwrap() = None;
            }
            AppEvent::PendingCleared(_) => {
                *self.pending_id.lock().unwrap() = None;
            }
            AppEvent::FormShown(_) => {
                self.view.show_form();
            }
            AppEvent::FormHidden(_) => {
                self.view.hide_form();
            }
            AppEvent::Error(ev) => {
                self.view.add_error_message(ev.message);
            }
        }
    }

    fn process_stream_result(
        &mut self,
        result: Result<(), String>,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        if let Err(ref err) = result {
            if self.connection.is_connected() && Connection::is_connection_error(err) {
                self.connection
                    .on_disconnected(&self.rt, &self.reconnect_tx);
                self.view.connected = false;
                self.view
                    .set_status("Disconnected \u{2014} reconnecting...".to_string());
            } else {
                self.view
                    .chat
                    .messages
                    .push(Message::new(Role::Error, err.clone()));
            }
            self.view.chat.finish_streaming();
            *self.pending_id.lock().unwrap() = None;
        }
        terminal.draw(|frame| self.view.render(frame))?;
        Ok(())
    }

    fn process_effect(&mut self, effect: commands::CommandEffect) {
        match effect {
            commands::CommandEffect::Status(msg) => {
                self.view.set_status(msg);
            }
            commands::CommandEffect::Apply(f) => {
                let mut ctx = commands::AppEffectContext {
                    app: &mut self.view,
                    client: &mut self.connection.client,
                    session_id: &self.connection.session_id,
                    event_handle: &mut self.connection.event_handle,
                    event_rx: &mut self.connection.event_rx,
                };
                f(&mut ctx);
            }
        }
    }

    /// Returns false if the app should quit.
    fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') => {
                return Ok(false);
            }
            KeyCode::Esc => {
                if self.view.status_message.is_some() {
                    self.view.clear_status();
                } else if self.view.is_command_picker_active() {
                    self.view.input.clear();
                    self.view.deactivate_command_picker();
                } else {
                    let pending = self.pending_id.lock().unwrap().clone();
                    if pending.is_some() {
                        if let Some(task) = self.stream_task.lock().unwrap().take() {
                            task.abort();
                        }
                        let pending_id_clone = pending.clone();
                        let addr_clone = self.connection.addr.clone();
                        let port = self.connection.port;
                        let session_id_clone =
                            self.connection.session_id.lock().unwrap().clone();
                        let project_path_clone = self.connection.project_path.clone();
                        self.rt.spawn(async move {
                            if let Ok(client) =
                                JsonRpcClient::connect_ws(&addr_clone, port).await
                                && client
                                    .bind_session(session_id_clone, project_path_clone)
                                    .await
                                    .is_ok()
                            {
                                let _ =
                                    client.interrupt_stream(pending_id_clone.unwrap()).await;
                            }
                        });
                        self.view.chat.finish_streaming();
                        terminal.draw(|frame| self.view.render(frame))?;
                    }
                }
            }
            KeyCode::Enter => {
                if self.view.status_message.is_some() {
                    self.view.clear_status();
                } else if self.view.is_command_picker_active() {
                    if let Some(cmd) = self.view.selected_command() {
                        let cmd_name = cmd.name.clone();
                        let ctx = commands::CommandContext {
                            rt: &self.rt,
                            client: self.connection.client.clone(),
                            project_path: self.connection.project_path.clone(),
                        };
                        if let Some(c) = commands::all_commands()
                            .into_iter()
                            .find(|c| c.name() == cmd_name)
                        {
                            c.execute(ctx, self.effect_tx.clone());
                        }
                    }
                    self.view.input.clear();
                    self.view.deactivate_command_picker();
                } else if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.view.input.push_char('\n');
                } else {
                    let input = self.view.input.value().to_string();
                    if input.trim().is_empty() || self.view.is_streaming() {
                        return Ok(true);
                    }
                    if input.starts_with('/') {
                        let filter = input.strip_prefix('/').unwrap_or("");
                        let has_match = AppView::available_commands()
                            .iter()
                            .any(|c| c.name.starts_with(filter));
                        if !has_match {
                            self.view.input.clear();
                            terminal.draw(|frame| self.view.render(frame))?;
                            return Ok(true);
                        }
                    }
                    if !self.view.connected {
                        self.view.set_status(
                            "Not connected to server \u{2014} reconnecting...".to_string(),
                        );
                        terminal.draw(|frame| self.view.render(frame))?;
                        return Ok(true);
                    }
                    self.view.input.clear();
                    self.view.chat.add_user_message(input.clone());
                    self.view.chat.start_streaming_message();
                    terminal.draw(|frame| self.view.render(frame))?;
                    self.send_message(input);
                }
            }
            KeyCode::Char(c) => {
                if self.view.status_message.is_some() {
                    self.view.clear_status();
                }
                if c == '/' && !AppView::available_commands().is_empty() {
                    self.view.input.push_char(c);
                    self.view.activate_command_picker();
                } else if self.view.is_command_picker_active() {
                    self.view.input.push_char(c);
                    let filter = {
                        let v = self.view.input.value();
                        if v.len() > 1 { v[1..].to_string() } else { String::new() }
                    };
                    self.view.update_command_filter(&filter);
                } else {
                    self.view.input.push_char(c);
                }
            }
            KeyCode::Backspace => {
                if self.view.status_message.is_some() {
                    self.view.clear_status();
                } else if self.view.is_command_picker_active() {
                    self.view.input.pop();
                    let filter = {
                        let v = self.view.input.value();
                        if v.len() > 1 { v[1..].to_string() } else { String::new() }
                    };
                    if filter.is_empty() {
                        self.view.deactivate_command_picker();
                    } else {
                        self.view.update_command_filter(&filter);
                    }
                } else {
                    self.view.input.pop();
                }
            }
            KeyCode::Up => {
                if self.view.is_command_picker_active() {
                    self.view.move_command_selection_up();
                    if let Some(cmd) = self.view.selected_command() {
                        let name = format!("/{}", cmd.name.clone());
                        self.view.input.set(name);
                    }
                }
            }
            KeyCode::Down => {
                if self.view.is_command_picker_active() {
                    self.view.move_command_selection_down();
                    if let Some(cmd) = self.view.selected_command() {
                        let name = format!("/{}", cmd.name.clone());
                        self.view.input.set(name);
                    }
                }
            }
            _ => {}
        }

        if let KeyCode::Char('c') = key.code
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            return Ok(false);
        }

        Ok(true)
    }

    fn send_message(&mut self, input: String) {
        let stream_result_tx = self.stream_result_tx.clone();
        let addr = self.connection.addr.clone();
        let port = self.connection.port;
        let stream_task = self.stream_task.clone();
        let session_id = self.connection.session_id.lock().unwrap().clone();
        let project_path = self.connection.project_path.clone();

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
            let _ = stream_result_tx.send(result);
        });
        *stream_task.lock().unwrap() = Some(task);
    }
}
