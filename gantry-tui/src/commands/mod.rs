pub mod model;
pub mod new;
pub mod providers;
pub mod quit;
pub mod sessions;
pub mod tree;
pub mod usage;

use crate::message::Msg;
use gantry_core::App;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;

pub struct CommandContext {
    pub app: Arc<Mutex<App>>,
    pub msg_tx: Sender<Msg>,
    pub rt_handle: Handle,
}

pub trait Command: Send + Sync {
    fn execute(&self, ctx: CommandContext);
}

/// Compile-time registry of all available commands.
#[derive(Clone, Copy)]
pub enum KnownCommand {
    Model,
    New,
    Providers,
    Quit,
    Sessions,
    Tree,
    Usage,
}

impl KnownCommand {
    pub const ALL: &[KnownCommand] = &[
        KnownCommand::Model,
        KnownCommand::New,
        KnownCommand::Providers,
        KnownCommand::Quit,
        KnownCommand::Sessions,
        KnownCommand::Tree,
        KnownCommand::Usage,
    ];

    pub const fn name(&self) -> &'static str {
        match self {
            KnownCommand::Model => "model",
            KnownCommand::New => "new",
            KnownCommand::Providers => "providers",
            KnownCommand::Quit => "quit",
            KnownCommand::Sessions => "sessions",
            KnownCommand::Tree => "tree",
            KnownCommand::Usage => "usage",
        }
    }

    pub const fn description(&self) -> &'static str {
        match self {
            KnownCommand::Model => "Pick the active model",
            KnownCommand::New => "Start a new session",
            KnownCommand::Providers => "Manage configured providers",
            KnownCommand::Quit => "Quit the application",
            KnownCommand::Sessions => "Browse and resume sessions",
            KnownCommand::Tree => "Browse the message tree",
            KnownCommand::Usage => "Show context window usage",
        }
    }

    /// Constructs the concrete [`Command`] implementation for this variant.
    pub fn into_command(self) -> Box<dyn Command> {
        match self {
            KnownCommand::Model => Box::new(model::Model),
            KnownCommand::New => Box::new(new::New),
            KnownCommand::Providers => Box::new(providers::Providers),
            KnownCommand::Quit => Box::new(quit::Quit),
            KnownCommand::Sessions => Box::new(sessions::Sessions),
            KnownCommand::Tree => Box::new(tree::Tree),
            KnownCommand::Usage => Box::new(usage::Usage),
        }
    }
}

/// The length of the longest command name, computed at compile time.
pub const MAX_CMD_NAME_LEN: usize = max_name_len(KnownCommand::ALL);

const fn max_name_len(commands: &[KnownCommand]) -> usize {
    let mut max = 0;
    let mut i = 0;
    while i < commands.len() {
        let len = commands[i].name().len();
        if len > max {
            max = len;
        }
        i += 1;
    }
    max
}

