#![allow(dead_code)]

mod agent_statusline;
mod app_statusline;
mod attachment_picker;
mod chat;
mod command_picker;
mod commands;
pub mod effects;
mod input;
mod message;
mod model;
mod model_picker;
mod picker;
mod provider_config;
mod runtime;
mod session_picker;
pub mod theme;
mod tree;
mod usage;
mod utils;
mod view;
mod widgets;

use anyhow::{Context, Result};
use crossterm::{
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
};
use gantry_core::{
    App, CredentialsRepository, GlobalGantryDir, ProjectRootDir, ProviderClientRegistry,
    ProviderConfigRepository,
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::Arc;
use tokio::sync::Mutex;

pub fn run() -> Result<()> {
    let global_config_dir = GlobalGantryDir::new()?;
    let cwd = std::env::current_dir().context("failed to determine current directory")?;
    let project_root_dir = ProjectRootDir::new(&cwd)?;

    let providers = ProviderConfigRepository::load(&global_config_dir.config_file())?;
    let credentials = CredentialsRepository::load(&global_config_dir.credentials_file())?;
    let registry = ProviderClientRegistry::new(providers, credentials)?;
    let app = App::new(global_config_dir, project_root_dir, cwd.clone(), registry)?;
    let app = Arc::new(Mutex::new(app));

    let (_terminal_guard, mut terminal) = TerminalGuard::enter()?;
    let mut runtime = runtime::Runtime::new(app, cwd)?;
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
                        | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
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
