pub mod new;
pub mod quit;
pub mod tree;

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
    New,
    Quit,
    Tree,
}

impl KnownCommand {
    pub const ALL: &[KnownCommand] = &[
        KnownCommand::New,
        KnownCommand::Quit,
        KnownCommand::Tree,
    ];

    pub const fn name(&self) -> &'static str {
        match self {
            KnownCommand::New => "new",
            KnownCommand::Quit => "quit",
            KnownCommand::Tree => "tree",
        }
    }

    pub const fn description(&self) -> &'static str {
        match self {
            KnownCommand::New => "Start a new session",
            KnownCommand::Quit => "Quit the application",
            KnownCommand::Tree => "Browse the message tree",
        }
    }

    /// Constructs the concrete [`Command`] implementation for this variant.
    pub fn into_command(self) -> Box<dyn Command> {
        match self {
            KnownCommand::New => Box::new(new::New),
            KnownCommand::Quit => Box::new(quit::Quit),
            KnownCommand::Tree => Box::new(tree::Tree),
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

