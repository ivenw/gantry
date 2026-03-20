mod ui;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Write};

use ui::App;

fn main() -> Result<()> {
    execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
    execute!(io::stdout(), EnableBracketedPaste)?;
    crossterm::terminal::enable_raw_mode()?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = App::new();

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
                    app.input_buffer.clear();
                    app.send_message(input);
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
