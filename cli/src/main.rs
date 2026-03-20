mod sse_client;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
};
use gantry_proto::client::JsonRpcClient;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use ui::App;
use sse_client::{ClientEvent, SseClient};

const DEFAULT_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3444;
const DEFAULT_SSE_PORT: u16 = 3445;

fn main() -> Result<()> {
    execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
    execute!(io::stdout(), EnableBracketedPaste)?;
    crossterm::terminal::enable_raw_mode()?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let addr = std::env::var("GANTRY_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let port: u16 = std::env::var("GANTRY_PORT")
        .unwrap_or_else(|_| DEFAULT_PORT.to_string())
        .parse()
        .unwrap_or(DEFAULT_PORT);
    let sse_port: u16 = std::env::var("GANTRY_SSE_PORT")
        .unwrap_or_else(|_| DEFAULT_SSE_PORT.to_string())
        .parse()
        .unwrap_or(DEFAULT_SSE_PORT);

    let rt = tokio::runtime::Runtime::new()?;

    let (sse, mut event_rx) = SseClient::new(&addr, sse_port);
    let sse_handle = rt.block_on(sse.connect());

    let client = rt.block_on(async {
        JsonRpcClient::connect_tcp(&addr, port).await
    })?;

    let mut app = App::new();
    let client_id = Arc::new(Mutex::new(String::new()));
    let mut streaming_message_id: Option<String> = None;
    let pending_id = Arc::new(Mutex::new(Option::<String>::None));

    fn process_event(
        event: ClientEvent,
        app: &mut App,
        client_id: &Arc<Mutex<String>>,
        streaming_message_id: &mut Option<String>,
        pending_id: &Arc<Mutex<Option<String>>>,
    ) {
        match event {
            ClientEvent::Init { client_id: cid, messages, pending_message, form } => {
                app.messages = messages;
                if let Some(pending) = pending_message {
                    app.add_user_message(pending.content.clone());
                    app.start_streaming_message();
                    *pending_id.lock().unwrap() = Some(pending.id.clone());
                }
                *client_id.lock().unwrap() = cid;
                if form.is_some() {
                    app.show_form();
                }
            }
            ClientEvent::MessageReceived { id, client_id: _, content: _ } => {
                *pending_id.lock().unwrap() = Some(id);
            }
            ClientEvent::StreamStart { message_id, pending_of: _ } => {
                *streaming_message_id = Some(message_id);
            }
            ClientEvent::Token { message_id: _, delta } => {
                app.append_to_streaming(&delta);
            }
            ClientEvent::StreamEnd { message_id: _, content: _ } => {
                app.finish_streaming();
                *streaming_message_id = None;
                *pending_id.lock().unwrap() = None;
            }
            ClientEvent::PendingCleared { pending_id: _ } => {
                *pending_id.lock().unwrap() = None;
            }
            ClientEvent::FormShown { id: _, options: _ } => {
                app.show_form();
            }
            ClientEvent::FormHidden { id: _, selected_by: _, selected: _ } => {
                app.hide_form();
            }
            ClientEvent::Error { message } => {
                app.messages.push(gantry_types::Message::new(
                    gantry_types::Role::Error,
                    message,
                ));
            }
            ClientEvent::Connected => {
                println!("Connected to SSE server");
            }
            ClientEvent::Disconnected => {
                println!("Disconnected from SSE server");
            }
        }
    }

    terminal.draw(|frame| {
        app.render(frame);
    })?;
    io::stdout().flush()?;

    let mut running = true;
    while running {
        while let Ok(event) = event_rx.try_recv() {
            process_event(event, &mut app, &client_id, &mut streaming_message_id, &pending_id);
            terminal.draw(|frame| {
                app.render(frame);
            })?;
            io::stdout().flush()?;
        }

        if event::poll(std::time::Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => {
                        running = false;
                        break;
                    }
                    KeyCode::Enter => {
                        let input = app.input_buffer.clone();
                        if input.trim().is_empty() {
                            continue;
                        }
                        app.input_buffer.clear();
                        app.add_user_message(input.clone());
                        app.start_streaming_message();

                        terminal.draw(|frame| {
                            app.render(frame);
                        })?;
                        io::stdout().flush()?;

                        let cid = client_id.lock().unwrap().clone();
                        if cid.is_empty() {
                            let messages = rt.block_on(client.stream_message(input));
                            match messages {
                                Ok(msgs) => {
                                    app.messages = msgs;
                                    app.finish_streaming();
                                }
                                Err(e) => {
                                    app.messages.push(gantry_types::Message::new(
                                        gantry_types::Role::Error,
                                        e.to_string(),
                                    ));
                                    app.finish_streaming();
                                }
                            }
                        } else {
                            let result = rt.block_on(client.send_message_sse(input, cid));
                            if let Ok(pending) = result {
                                *pending_id.lock().unwrap() = Some(pending.id);
                            } else if let Err(e) = result {
                                app.messages.push(gantry_types::Message::new(
                                    gantry_types::Role::Error,
                                    e.to_string(),
                                ));
                                app.finish_streaming();
                            }
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
                        running = false;
                        break;
                    }
                }

                terminal.draw(|frame| {
                    app.render(frame);
                })?;
                io::stdout().flush()?;
            }
        }
    }

    sse_handle.abort();
    crossterm::terminal::disable_raw_mode()?;
    execute!(io::stdout(), DisableBracketedPaste)?;
    execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
    Ok(())
}
