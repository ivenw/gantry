use unicode_width::UnicodeWidthStr;

use crate::picker::Picker;

pub struct CommandPickerState {
    pub picker: Picker<KnownCommand>,
    /// Maximum command name width across the full unfiltered list, used to size the name column.
    /// Computed once from the unfiltered list so column width stays stable as the filter changes.
    pub cmd_col_width: u16,
}

impl CommandPickerState {
    /// Creates a `CommandPickerState` populated with all known commands.
    pub fn new() -> Self {
        let commands: Vec<_> = KnownCommand::ALL.iter().copied().collect();
        let cmd_col_width = commands
            .iter()
            .map(|c| c.name().width() as u16)
            .max()
            .unwrap_or(0);
        let picker = Picker::new(commands, |c| c.name());
        Self {
            picker,
            cmd_col_width,
        }
    }

    /// Appends a character to the filter and recomputes filtered results.
    pub fn push_filter(&mut self, c: char) {
        self.picker.filter.push(c);
        self.picker.refilter(|c| c.name());
    }

    /// Removes the last character from the filter and recomputes filtered results.
    pub fn pop_filter(&mut self) {
        self.picker.filter.pop();
        self.picker.refilter(|c| c.name());
    }
}

/// Every command that can be invoked from the command picker.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KnownCommand {
    Debug,
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
        KnownCommand::Debug,
        KnownCommand::Model,
        KnownCommand::New,
        KnownCommand::Providers,
        KnownCommand::Quit,
        KnownCommand::Sessions,
        KnownCommand::Tree,
        KnownCommand::Usage,
    ];

    /// Short name shown in the command picker filter.
    pub const fn name(&self) -> &'static str {
        match self {
            KnownCommand::Debug => "debug",
            KnownCommand::Model => "model",
            KnownCommand::New => "new",
            KnownCommand::Providers => "providers",
            KnownCommand::Quit => "quit",
            KnownCommand::Sessions => "sessions",
            KnownCommand::Tree => "tree",
            KnownCommand::Usage => "usage",
        }
    }

    /// One-line description shown next to the name in the command picker.
    pub const fn description(&self) -> &'static str {
        match self {
            KnownCommand::Debug => "Stream a mock response for UI testing",
            KnownCommand::Model => "Pick the active model",
            KnownCommand::New => "Start a new session",
            KnownCommand::Providers => "Manage configured providers",
            KnownCommand::Quit => "Quit the application",
            KnownCommand::Sessions => "Browse and resume sessions",
            KnownCommand::Tree => "Browse the message tree",
            KnownCommand::Usage => "Show context window usage",
        }
    }
}
