mod ui;

use anyhow::{Result, anyhow};
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
};
use gantry_core::{AppEvent, Message, Role};
use gantry_rpc::{JsonRpcClient, WsConnectionEvent};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;
use ui::App;

const DEFAULT_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3444;

fn discover_project() -> Option<std::path::PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(".gantry").is_dir() {
            return Some(dir);
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return None,
        }
    }
}

struct ReconnectSuccess {
    client: JsonRpcClient,
    session_id: String,
    event_handle: JoinHandle<()>,
    event_rx: Receiver<WsConnectionEvent>,
    clear_messages: bool,
}

enum SessionMode {
    ResumeOrCreate,
    AlwaysCreate,
}

async fn try_connect_async(
    addr: &str,
    port: u16,
    project_path: &Path,
    mode: SessionMode,
) -> Option<(JsonRpcClient, String, JoinHandle<()>, Receiver<WsConnectionEvent>)> {
    let client = JsonRpcClient::connect_ws(addr, port).await.ok()?;

    let session_id = match mode {
        SessionMode::AlwaysCreate => client.create_session(project_path.to_path_buf()).await.ok()?,
        SessionMode::ResumeOrCreate => {
            let sessions = client.list_sessions(project_path.to_path_buf()).await.ok()?;
            if let Some(last) = sessions.last() {
                last.id.clone()
            } else {
                client.create_session(project_path.to_path_buf()).await.ok()?
            }
        }
    };

    client
        .connect_session(session_id.clone(), project_path.to_path_buf())
        .await
        .ok()?;

    let (handle, rx) = client.subscribe_events().await.ok()?;

    Some((client, session_id, handle, rx))
}

fn make_empty_subscription(rt: &Runtime) -> (JoinHandle<()>, Receiver<WsConnectionEvent>) {
    let (_, rx) = tokio::sync::mpsc::channel(1);
    let handle = rt.spawn(async {});
    (handle, rx)
}

fn spawn_reconnect_task(
    rt: &Runtime,
    addr: String,
    port: u16,
    project_path: PathBuf,
    tx: mpsc::SyncSender<ReconnectSuccess>,
) {
    rt.spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if let Some((client, session_id, event_handle, event_rx)) =
                try_connect_async(&addr, port, &project_path, SessionMode::ResumeOrCreate).await
            {
                let _ = tx.send(ReconnectSuccess {
                    client,
                    session_id,
                    event_handle,
                    event_rx,
                    clear_messages: false,
                });
                return;
            }
        }
    });
}

fn is_connection_error(err: &str) -> bool {
    err.contains("connection refused")
        || err.contains("failed to connect")
        || err.contains("broken pipe")
        || err.contains("WebSocket")
        || err.contains("os error")
}

pub fn run() -> Result<()> {
    let addr = std::env::var("GANTRY_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let port: u16 = std::env::var("GANTRY_PORT")
        .unwrap_or_else(|_| DEFAULT_PORT.to_string())
        .parse()
        .unwrap_or(DEFAULT_PORT);

    let project_path = discover_project().ok_or_else(|| {
        anyhow!("no gantry project found in current directory or any parent\nRun `gantry init` to register this project.")
    })?;

    let rt = tokio::runtime::Runtime::new()?;

    let (reconnect_tx, reconnect_rx) = mpsc::sync_channel::<ReconnectSuccess>(1);
    let session_id: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let mut client: Option<Arc<JsonRpcClient>> = None;
    let mut reconnect_pending = false;

    let (_terminal_guard, mut terminal) = TerminalGuard::enter()?;
    let mut app = App::new();

    let (mut event_handle, mut event_rx) = if let Some((c, sid, handle, rx)) = rt.block_on(
        try_connect_async(&addr, port, &project_path, SessionMode::ResumeOrCreate),
    ) {
        *session_id.lock().unwrap() = sid;
        client = Some(Arc::new(c));
        app.connected = true;
        (handle, rx)
    } else {
        app.set_status("Disconnected \u{2014} reconnecting...".to_string());
        spawn_reconnect_task(
            &rt,
            addr.clone(),
            port,
            project_path.clone(),
            reconnect_tx.clone(),
        );
        reconnect_pending = true;
        make_empty_subscription(&rt)
    };

    let (stream_result_tx, stream_result_rx) = mpsc::channel::<Result<(), String>>();
    let (command_result_tx, command_result_rx) = mpsc::channel::<String>();

    let pending_id = Arc::new(Mutex::new(Option::<String>::None));
    let stream_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(None));

    terminal.draw(|frame| {
        app.render(frame);
    })?;

    loop {
        while let Ok(event) = event_rx.try_recv() {
            match event {
                WsConnectionEvent::Event(ev) => {
                    process_app_event(ev, &mut app, &pending_id);
                }
                WsConnectionEvent::Disconnected => {
                    if app.connected {
                        app.connected = false;
                        client = None;
                        app.set_status("Disconnected \u{2014} reconnecting...".to_string());
                        if let Some(task) = stream_task.lock().unwrap().take() {
                            task.abort();
                        }
                        app.finish_streaming();
                        *pending_id.lock().unwrap() = None;
                        if !reconnect_pending {
                            reconnect_pending = true;
                            spawn_reconnect_task(
                                &rt,
                                addr.clone(),
                                port,
                                project_path.clone(),
                                reconnect_tx.clone(),
                            );
                        }
                    }
                }
                WsConnectionEvent::Error(message) => {
                    app.messages.push(Message::new(Role::Error, message));
                }
            }

            terminal.draw(|frame| {
                app.render(frame);
            })?;
        }

        while let Ok(result) = stream_result_rx.try_recv() {
            if let Err(ref err) = result {
                if app.connected && is_connection_error(err) {
                    app.connected = false;
                    client = None;
                    app.set_status("Disconnected \u{2014} reconnecting...".to_string());
                    if !reconnect_pending {
                        reconnect_pending = true;
                        spawn_reconnect_task(
                            &rt,
                            addr.clone(),
                            port,
                            project_path.clone(),
                            reconnect_tx.clone(),
                        );
                    }
                } else {
                    app.messages.push(Message::new(Role::Error, err.clone()));
                }
                app.finish_streaming();
                *pending_id.lock().unwrap() = None;
            }

            terminal.draw(|frame| {
                app.render(frame);
            })?;
        }

        while let Ok(status) = command_result_rx.try_recv() {
            app.set_status(status);
            terminal.draw(|frame| {
                app.render(frame);
            })?;
        }

        while let Ok(success) = reconnect_rx.try_recv() {
            reconnect_pending = false;
            event_handle.abort();
            event_handle = success.event_handle;
            event_rx = success.event_rx;
            *session_id.lock().unwrap() = success.session_id;
            client = Some(Arc::new(success.client));
            app.connected = true;
            if success.clear_messages {
                app.reset_for_new_session();
            }
            app.status_message = None;
            terminal.draw(|frame| app.render(frame))?;
        }

        if event::poll(std::time::Duration::from_millis(10))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char('q') => {
                    break;
                }
                KeyCode::Esc => {
                    if app.status_message.is_some() {
                        app.clear_status();
                    } else if app.is_command_picker_active() {
                        app.input_buffer.clear();
                        app.deactivate_command_picker();
                    } else {
                        let pending = pending_id.lock().unwrap().clone();
                        if pending.is_some() {
                            if let Some(task) = stream_task.lock().unwrap().take() {
                                task.abort();
                            }
                            let pending_id_clone = pending.clone();
                            let addr_clone = addr.clone();
                            let session_id_clone = session_id.lock().unwrap().clone();
                            let project_path_clone = project_path.clone();
                            rt.spawn(async move {
                                if let Ok(client) =
                                    JsonRpcClient::connect_ws(&addr_clone, port).await
                                    && client
                                        .connect_session(session_id_clone, project_path_clone)
                                        .await
                                        .is_ok()
                                {
                                    let _ =
                                        client.interrupt_stream(pending_id_clone.unwrap()).await;
                                }
                            });
                            app.finish_streaming();
                            terminal.draw(|frame| {
                                app.render(frame);
                            })?;
                        }
                    }
                }
                KeyCode::Enter => {
                    if app.status_message.is_some() {
                        app.clear_status();
                    } else if app.is_command_picker_active() {
                        if let Some(cmd) = app.selected_command() {
                            let cmd_name = cmd.name.clone();
                            execute_command(
                                &cmd_name,
                                &rt,
                                &client,
                                command_result_tx.clone(),
                                reconnect_tx.clone(),
                                addr.clone(),
                                port,
                                project_path.clone(),
                            );
                        }
                        app.input_buffer.clear();
                        app.deactivate_command_picker();
                    } else if key.modifiers.contains(KeyModifiers::SHIFT) {
                        app.input_buffer.push('\n');
                    } else {
                        let input = app.input_buffer.clone();
                        if input.trim().is_empty() || app.is_streaming() {
                            continue;
                        }
                        if input.starts_with('/') {
                            let filter = input.strip_prefix('/').unwrap_or("");
                            let has_match = App::available_commands()
                                .iter()
                                .any(|c| c.name.starts_with(filter));
                            if !has_match {
                                app.input_buffer.clear();
                                terminal.draw(|frame| app.render(frame))?;
                                continue;
                            }
                        }
                        if !app.connected {
                            app.set_status(
                                "Not connected to server \u{2014} reconnecting...".to_string(),
                            );
                            terminal.draw(|frame| app.render(frame))?;
                            continue;
                        }
                        app.input_buffer.clear();
                        app.add_user_message(input.clone());
                        app.start_streaming_message();

                        terminal.draw(|frame| {
                            app.render(frame);
                        })?;

                        let stream_result_tx = stream_result_tx.clone();
                        let addr_for_request = addr.clone();
                        let stream_task = stream_task.clone();
                        let session_id_for_task = session_id.lock().unwrap().clone();
                        let project_path_for_task = project_path.clone();
                        let task = rt.spawn(async move {
                            let result = match JsonRpcClient::connect_ws(&addr_for_request, port)
                                .await
                            {
                                Ok(client) => {
                                    if let Err(e) = client
                                        .connect_session(session_id_for_task, project_path_for_task)
                                        .await
                                    {
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
                KeyCode::Char(c) => {
                    if app.status_message.is_some() {
                        app.clear_status();
                    }
                    if c == '/' && !App::available_commands().is_empty() {
                        app.input_buffer.push(c);
                        app.activate_command_picker();
                    } else if app.is_command_picker_active() {
                        app.input_buffer.push(c);
                        let filter = if app.input_buffer.len() > 1 {
                            app.input_buffer[1..].to_string()
                        } else {
                            String::new()
                        };
                        app.update_command_filter(&filter);
                    } else {
                        app.input_buffer.push(c);
                    }
                }
                KeyCode::Backspace => {
                    if app.status_message.is_some() {
                        app.clear_status();
                    } else if app.is_command_picker_active() {
                        app.input_buffer.pop();
                        let filter = if app.input_buffer.len() > 1 {
                            app.input_buffer[1..].to_string()
                        } else {
                            String::new()
                        };
                        if filter.is_empty() {
                            app.deactivate_command_picker();
                        } else {
                            app.update_command_filter(&filter);
                        }
                    } else {
                        app.input_buffer.pop();
                    }
                }
                KeyCode::Up => {
                    if app.is_command_picker_active() {
                        app.move_command_selection_up();
                        if let Some(cmd) = app.selected_command() {
                            app.input_buffer = format!("/{}", cmd.name);
                        }
                    }
                }
                KeyCode::Down => {
                    if app.is_command_picker_active() {
                        app.move_command_selection_down();
                        if let Some(cmd) = app.selected_command() {
                            app.input_buffer = format!("/{}", cmd.name);
                        }
                    }
                }
                _ => {}
            }

            if let KeyCode::Char('c') = key.code
                && key.modifiers.contains(KeyModifiers::CONTROL)
            {
                break;
            }

            terminal.draw(|frame| {
                app.render(frame);
            })?;
        }
    }

    event_handle.abort();
    Ok(())
}

fn process_app_event(event: AppEvent, app: &mut App, pending_id: &Arc<Mutex<Option<String>>>) {
    match event {
        AppEvent::Init(ev) => {
            app.messages = ev.messages;
            if let Some(pending) = ev.pending_message {
                app.add_user_message(pending.content.clone());
                app.start_streaming_message();
                *pending_id.lock().unwrap() = Some(pending.id.clone());
            }
            if ev.form.is_some() {
                app.show_form();
            }
        }
        AppEvent::MessageReceived(ev) => {
            *pending_id.lock().unwrap() = Some(ev.id);
        }
        AppEvent::StreamStart(_) => {}
        AppEvent::Token(ev) => {
            app.append_to_streaming(&ev.delta);
        }
        AppEvent::StreamEnd(_) => {
            app.finish_streaming();
            *pending_id.lock().unwrap() = None;
        }
        AppEvent::PendingCleared(_) => {
            *pending_id.lock().unwrap() = None;
        }
        AppEvent::FormShown(_) => {
            app.show_form();
        }
        AppEvent::FormHidden(_) => {
            app.hide_form();
        }
        AppEvent::Error(ev) => {
            app.messages.push(Message::new(Role::Error, ev.message));
        }
    }
}

struct TerminalGuard {
    keyboard_enhancement_enabled: bool,
}

impl TerminalGuard {
    fn enter() -> Result<(Self, Terminal<CrosstermBackend<io::Stdout>>)> {
        execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
        crossterm::terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnableBracketedPaste)?;

        let keyboard_enhancement_enabled = matches!(
            crossterm::terminal::supports_keyboard_enhancement(),
            Ok(true)
        );
        if keyboard_enhancement_enabled {
            execute!(
                io::stdout(),
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                )
            )?;
        }

        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        Ok((
            Self {
                keyboard_enhancement_enabled,
            },
            terminal,
        ))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.keyboard_enhancement_enabled {
            let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
        }
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), DisableBracketedPaste);
        let _ = execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_command(
    name: &str,
    rt: &Runtime,
    client: &Option<Arc<JsonRpcClient>>,
    command_result_tx: mpsc::Sender<String>,
    reconnect_tx: mpsc::SyncSender<ReconnectSuccess>,
    addr: String,
    port: u16,
    project_path: PathBuf,
) {
    if name == "health" {
        match client {
            Some(c) => {
                let c = c.clone();
                let tx = command_result_tx.clone();
                rt.spawn(async move {
                    let start = std::time::Instant::now();
                    match c.ping().await {
                        Ok(_) => {
                            let latency = start.elapsed().as_millis();
                            let _ = tx.send(format!("Connected: {}ms", latency));
                        }
                        Err(e) => {
                            let _ = tx.send(format!("Ping failed: {}", e));
                        }
                    }
                });
            }
            None => {
                let _ = command_result_tx.send("Not connected".to_string());
            }
        }
    } else if name == "new" {
        rt.spawn(async move {
            if let Some((client, session_id, event_handle, event_rx)) =
                try_connect_async(&addr, port, &project_path, SessionMode::AlwaysCreate).await
            {
                let _ = reconnect_tx.send(ReconnectSuccess {
                    client,
                    session_id,
                    event_handle,
                    event_rx,
                    clear_messages: true,
                });
            }
        });
    }
}
