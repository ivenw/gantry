use unicode_width::UnicodeWidthStr;

use crate::picker::Picker;
use crate::commands::KnownCommand

pub struct CommandPickerState {
    pub picker: Picker<CommandEntry>,
    /// Maximum command name width across the full unfiltered list, used to size the name column.
    /// Computed once from the unfiltered list so column width stays stable as the filter changes.
    pub cmd_col_width: u16,
}

#[derive(Clone)]
pub struct CommandEntry {
    pub name: String,
    pub description: String,
    pub command: KnownCommand,
}

impl From<KnownCommand> for CommandEntry {
    fn from(k: crate::commands::KnownCommand) -> Self {
        Self {
            name: k.name().to_string(),
            description: k.description().to_string(),
            command: k,
        }
    }
}

// TODO: consolidate new and new_all
impl CommandPickerState {
    /// Creates a `CommandPickerState` populated with all known commands.
    pub fn new_all() -> Self {
        Self::new(
            crate::commands::KnownCommand::ALL
                .iter()
                .copied()
                .map(CommandEntry::from)
                .collect(),
        )
    }

    /// Creates a new `CommandPickerState` from the given entries.
    pub fn new(commands: Vec<CommandEntry>) -> Self {
        let cmd_col_width = commands
            .iter()
            .map(|c| c.name.width() as u16)
            .max()
            .unwrap_or(0);
        let picker = Picker::new(commands, |e| e.name.as_str());
        Self {
            picker,
            cmd_col_width,
        }
    }

    /// Appends a character to the filter and recomputes filtered results.
    pub fn push_filter(&mut self, c: char) {
        self.picker.filter.push(c);
        self.picker.refilter(|e| e.name.as_str());
    }

    /// Removes the last character from the filter and recomputes filtered results.
    pub fn pop_filter(&mut self) {
        self.picker.filter.pop();
        self.picker.refilter(|e| e.name.as_str());
    }
}
