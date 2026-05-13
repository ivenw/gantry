use gantry_core::{PathSearchResult, SkillSearchResult};

/// A fuzzy-find picker for file/directory or skill attachments.
pub struct AttachmentPickerState {
    pub kind: AttachmentPickerKind,
    pub filter: String,
    pub selected_idx: usize,
    /// Maximum name width across all results in the current result set; recomputed when results change.
    ///
    /// For path pickers this is always 0 (single column). For skill pickers it stabilises the
    /// name column width across scroll.
    pub name_col_width: u16,
}

/// Discriminates between path and skill attachment pickers.
pub enum AttachmentPickerKind {
    Path(Vec<PathSearchResult>),
    Skill(Vec<SkillSearchResult>),
}

impl AttachmentPickerState {
    /// Creates a new path picker with the given search results.
    pub fn new_path(results: Vec<PathSearchResult>) -> Self {
        Self {
            kind: AttachmentPickerKind::Path(results),
            filter: String::new(),
            selected_idx: 0,
            name_col_width: 0,
        }
    }

    /// Creates a new skill picker with the given search results.
    pub fn new_skill(results: Vec<SkillSearchResult>) -> Self {
        let name_col_width = results
            .iter()
            .map(|r| r.skill.metadata.name.chars().count() as u16)
            .max()
            .unwrap_or(0);
        Self {
            kind: AttachmentPickerKind::Skill(results),
            filter: String::new(),
            selected_idx: 0,
            name_col_width,
        }
    }

    /// Replaces the path results and recomputes stable column widths.
    pub fn set_path_results(&mut self, results: Vec<PathSearchResult>) {
        self.kind = AttachmentPickerKind::Path(results);
        self.name_col_width = 0;
        self.selected_idx = 0;
    }

    /// Replaces the skill results and recomputes the stable name column width.
    pub fn set_skill_results(&mut self, results: Vec<SkillSearchResult>) {
        self.name_col_width = results
            .iter()
            .map(|r| r.skill.metadata.name.chars().count() as u16)
            .max()
            .unwrap_or(0);
        self.kind = AttachmentPickerKind::Skill(results);
        self.selected_idx = 0;
    }

    /// Returns the number of items currently displayed.
    pub fn len(&self) -> usize {
        match &self.kind {
            AttachmentPickerKind::Path(results) => results.len(),
            AttachmentPickerKind::Skill(results) => results.len(),
        }
    }

    /// Returns true if the picker has no results.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
