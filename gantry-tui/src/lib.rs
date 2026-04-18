mod commands;
mod connection;
mod message;
mod model;
mod runtime;
mod update;
mod views;

use anyhow::{Result, anyhow};
use crossterm::{
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

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

pub fn run() -> Result<()> {
    let addr = std::env::var("GANTRY_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let port: u16 = std::env::var("GANTRY_PORT")
        .unwrap_or_else(|_| DEFAULT_PORT.to_string())
        .parse()
        .unwrap_or(DEFAULT_PORT);

    let project_path = discover_project().ok_or_else(|| {
        anyhow!("no gantry project found in current directory or any parent\nRun `gantry init` to register this project.")
    })?;

    let (_terminal_guard, mut terminal) = TerminalGuard::enter()?;
    let mut runtime = runtime::Runtime::new(addr, port, project_path)?;
    runtime.run(&mut terminal)
}

struct TerminalGuard {
    keyboard_enhancement_enabled: bool,
}

impl TerminalGuard {
    fn enter() -> Result<(Self, Terminal<CrosstermBackend<io::Stdout>>)> {
        execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
        crossterm::terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnableBracketedPaste, EnableMouseCapture)?;

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
                        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
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
        let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture);
        let _ = execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    }
}
