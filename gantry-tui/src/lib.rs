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
use gantry_client::{JsonRpcClient, WsConnectionEvent};
use gantry_contract::{AppEvent, Message, Role};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::{Arc, Mutex, mpsc};
use ui::App;

const DEFAULT_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3444;

pub fn run() -> Result<()> {
    let addr = std::env::var("GANTRY_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let port: u16 = std::env::var("GANTRY_PORT")
        .unwrap_or_else(|_| DEFAULT_PORT.to_string())
        .parse()
        .unwrap_or(DEFAULT_PORT);

    let rt = tokio::runtime::Runtime::new()?;

    let client = rt
        .block_on(async { JsonRpcClient::connect_ws(&addr, port).await })
        .map_err(|e| {
            anyhow!(
                "failed to connect to gantry daemon at {}:{} ({})\nRun `gantry setup`, then start daemon with `gantry server`.",
                addr,
                port,
                e
            )
        })?;

    let (event_handle, mut event_rx) = rt.block_on(client.subscribe_events())?;
    let (stream_result_tx, stream_result_rx) = mpsc::channel::<Result<(), String>>();

    let (_terminal_guard, mut terminal) = TerminalGuard::enter()?;

    let mut app = App::new();
    let pending_id = Arc::new(Mutex::new(Option::<String>::None));

    terminal.draw(|frame| {
        app.render(frame);
    })?;

    loop {
        while let Ok(event) = event_rx.try_recv() {
            match event {
                WsConnectionEvent::Event(ev) => {
                    process_app_event(ev, &mut app, &pending_id);
                }
                WsConnectionEvent::Disconnected => {}
                WsConnectionEvent::Error(message) => {
                    app.messages.push(Message::new(Role::Error, message));
                }
            }

            terminal.draw(|frame| {
                app.render(frame);
            })?;
        }

        while let Ok(result) = stream_result_rx.try_recv() {
            if let Err(err) = result {
                app.messages.push(Message::new(Role::Error, err));
                app.finish_streaming();
            }

            terminal.draw(|frame| {
                app.render(frame);
            })?;
        }

        if event::poll(std::time::Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => {
                        break;
                    }
                    KeyCode::Enter => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            app.input_buffer.push('\n');
                        } else {
                            let input = app.input_buffer.clone();
                            if input.trim().is_empty() || app.is_streaming() {
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
                            rt.spawn(async move {
                                let result = match JsonRpcClient::connect_ws(&addr_for_request, port).await {
                                    Ok(client) => client
                                        .stream_message(input)
                                        .await
                                        .map(|_| ())
                                        .map_err(|e| e.to_string()),
                                    Err(e) => Err(e.to_string()),
                                };
                                let _ = stream_result_tx.send(result);
                            });
                        }
                    }
                    KeyCode::Char(c) => {
                        app.input_buffer.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input_buffer.pop();
                    }
                    _ => {}
                }

                if let KeyCode::Char('c') = key.code {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        break;
                    }
                }

                terminal.draw(|frame| {
                    app.render(frame);
                })?;
            }
        }
    }

    event_handle.abort();
    Ok(())
}

fn process_app_event(
    event: AppEvent,
    app: &mut App,
    pending_id: &Arc<Mutex<Option<String>>>,
) {
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

        let keyboard_enhancement_enabled =
            matches!(crossterm::terminal::supports_keyboard_enhancement(), Ok(true));
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
