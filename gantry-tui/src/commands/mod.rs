/// Every command that can be invoked from the command picker.
#[derive(Clone, Copy, PartialEq, Eq)]
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

    /// Short name shown in the command picker filter.
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

    /// One-line description shown next to the name in the command picker.
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
}
