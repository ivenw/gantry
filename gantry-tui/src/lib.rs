mod commands;
pub mod effects;
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
use gantry_core::{
    AgentFactory, App, ConfiguredModel, ModelId, OllamaProviderConfig, ProviderConfig,
    ProviderConfigCatalog, ProviderId,
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::Arc;
use tokio::sync::Mutex;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const DEFAULT_OLLAMA_MODEL: &str = "ministral-3:3b";

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
    let project_path = discover_project().ok_or_else(|| {
        anyhow!("no gantry project found in current directory or any parent\nRun `gantry init` to initialize this directory.")
    })?;

    let ollama_url =
        std::env::var("GANTRY_OLLAMA_URL").unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_string());
    let ollama_model =
        std::env::var("GANTRY_OLLAMA_MODEL").unwrap_or_else(|_| DEFAULT_OLLAMA_MODEL.to_string());
    let catalog = ProviderConfigCatalog {
        providers: vec![ProviderConfig::Ollama(OllamaProviderConfig {
            id: ProviderId::new("ollama"),
            base_url: ollama_url,
            models: vec![ConfiguredModel {
                id: ModelId::new("default"),
                provider_model_name: ollama_model,
            }],
            default_model: ModelId::new("default"),
        })],
        default_provider: ProviderId::new("ollama"),
    };

    let agent_factory = AgentFactory::new(catalog)?;
    let default_selection = agent_factory
        .default_selection()
        .expect("provider catalog must have a default selection");

    let app = App::new(&project_path, default_selection, agent_factory)?;
    let app = Arc::new(Mutex::new(app));

    let (_terminal_guard, mut terminal) = TerminalGuard::enter()?;
    let mut runtime = runtime::Runtime::new(app)?;
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
