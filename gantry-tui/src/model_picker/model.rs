use nucleo_matcher::{
    Config, Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

use gantry_core::ModelSelection;

/// State for the model picker overlay.
pub struct ModelPickerView {
    pub models: Vec<ModelSelection>,
    pub filter: String,
    /// Index of the cursor row (keyboard highlight).
    pub selected_idx: usize,
    /// The model that was active when the picker was opened, used to mark the current selection.
    pub active_selection: Option<ModelSelection>,
    /// Cached fuzzy-filtered results; recomputed on every filter change.
    pub filtered: Vec<ModelEntry>,
    /// Maximum model alias width across the full unfiltered list; stable for the lifetime of the picker.
    pub model_col_width: u16,
    /// Maximum provider alias width across the full unfiltered list; stable for the lifetime of the picker.
    pub provider_col_width: u16,
    /// Maximum context length label width across the full unfiltered list; stable for the lifetime of the picker.
    pub context_col_width: u16,
}

/// A filtered model entry with fuzzy-match highlight indices.
#[derive(Clone)]
pub struct ModelEntry {
    pub selection: ModelSelection,
    /// Matched character indices into the display label from the last fuzzy filter.
    pub indices: Vec<u32>,
    /// Whether this entry is the active (currently selected) model.
    pub is_active: bool,
}

/// Formats a context length in tokens as a compact string (e.g. `131072` → `"128k"`).
pub fn format_context_length(tokens: u32) -> String {
    format!("{}k", (tokens + 512) / 1024)
}

impl ModelPickerView {
    /// Recomputes `self.filtered` from the current filter string.
    ///
    /// Call this after any mutation to `filter` or `models`. When the filter is empty all
    /// models are included in their original order. Otherwise entries are sorted by
    /// descending nucleo score and non-matching entries are excluded.
    pub fn refilter(&mut self) {
        let active = &self.active_selection;

        if self.filter.is_empty() {
            self.filtered = self
                .models
                .iter()
                .map(|s| ModelEntry {
                    is_active: active.as_ref() == Some(s),
                    selection: s.clone(),
                    indices: Vec::new(),
                })
                .collect();
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
        let mut scored: Vec<(u32, ModelEntry)> = self
            .models
            .iter()
            .filter_map(|s| {
                let mut indices = Vec::new();
                let score = pattern.indices(
                    nucleo_matcher::Utf32Str::new(s.model_id.as_str(), &mut buf),
                    &mut matcher,
                    &mut indices,
                )?;
                indices.sort_unstable();
                Some((
                    score,
                    ModelEntry {
                        is_active: active.as_ref() == Some(s),
                        selection: s.clone(),
                        indices,
                    },
                ))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        self.filtered = scored.into_iter().map(|(_, e)| e).collect();
    }
}
