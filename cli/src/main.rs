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
use ui::App;

const DEFAULT_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3444;

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

    let rt = tokio::runtime::Runtime::new()?;
    let client = rt.block_on(async {
        JsonRpcClient::connect_tcp(&addr, port).await
    })?;

    let mut app = App::new();

    if let Ok(messages) = rt.block_on(client.get_messages()) {
        app.messages = messages;
    }

    terminal.draw(|frame| {
        app.render(frame);
    })?;
    io::stdout().flush()?;

    loop {
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Enter => {
                    let input = app.input_buffer.clone();
                    if input.trim().is_empty() {
                        continue;
                    }
                    app.input_buffer.clear();
                    
                    if let Ok(messages) = rt.block_on(client.send_message(input)) {
                        app.messages = messages;
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
            io::stdout().flush()?;
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    execute!(io::stdout(), DisableBracketedPaste)?;
    execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
    Ok(())
}
