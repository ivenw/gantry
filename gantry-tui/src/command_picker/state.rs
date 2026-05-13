use crate::picker::Picker;

pub struct CommandPickerState {
    pub picker: Picker<CommandEntry>,
    /// Maximum command name width across the full unfiltered list; stable for the lifetime of the picker.
    pub cmd_col_width: u16,
}

#[derive(Clone)]
pub struct CommandEntry {
    pub name: String,
    pub description: String,
    pub command: crate::commands::KnownCommand,
}

impl CommandPickerState {
    /// Creates a new `CommandPickerState` from the given entries.
    pub fn new(commands: Vec<CommandEntry>) -> Self {
        let cmd_col_width = commands
            .iter()
            .map(|c| c.name.chars().count() as u16)
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
        self.picker.selected_idx = 0;
        self.picker.refilter(|e| e.name.as_str());
    }

    /// Removes the last character from the filter and recomputes filtered results.
    pub fn pop_filter(&mut self) {
        self.picker.filter.pop();
        self.picker.selected_idx = 0;
        self.picker.refilter(|e| e.name.as_str());
    }
}
