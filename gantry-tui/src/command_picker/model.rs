use nucleo_matcher::{
    Config, Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

pub struct CommandPicker {
    pub commands: Vec<CommandEntry>,
    pub filter: String,
    pub selected_idx: usize,
    /// Cached fuzzy-filtered results; recomputed on every filter change.
    pub filtered: Vec<CommandEntry>,
    /// Maximum command name width across the full unfiltered list; stable for the lifetime of the picker.
    pub cmd_col_width: u16,
}

#[derive(Clone)]
pub struct CommandEntry {
    pub name: String,
    pub description: String,
    pub command: crate::commands::KnownCommand,
    /// Matched character indices into `name` from the last fuzzy filter. Empty when unfiltered.
    pub indices: Vec<u32>,
}

impl CommandPicker {
    /// Recomputes `self.filtered` from the current filter string.
    ///
    /// Call this after any mutation to `filter` or `commands`. When the filter is empty all
    /// commands are included in their original order. Otherwise entries are sorted by
    /// descending nucleo score and non-matching entries are excluded.
    pub fn refilter(&mut self) {
        if self.filter.is_empty() {
            self.filtered = self.commands.clone();
            return;
        }

        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::new(
            &self.filter,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );

        let mut buf = Vec::new();
        let mut scored: Vec<(u32, CommandEntry)> = self
            .commands
            .iter()
            .filter_map(|cmd| {
                let mut indices = Vec::new();
                let score = pattern.indices(
                    nucleo_matcher::Utf32Str::new(&cmd.name, &mut buf),
                    &mut matcher,
                    &mut indices,
                )?;
                indices.sort_unstable();
                let mut entry = cmd.clone();
                entry.indices = indices;
                Some((score, entry))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        self.filtered = scored.into_iter().map(|(_, cmd)| cmd).collect();
    }
}
