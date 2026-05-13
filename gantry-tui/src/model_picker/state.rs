use gantry_core::ModelSelection;

use crate::picker::Picker;

/// State for the model picker overlay.
pub struct ModelPickerState {
    pub picker: Picker<ModelEntry>,
    /// The model that was active when the picker was opened, used to mark the current selection.
    pub active_selection: Option<ModelSelection>,
    /// Maximum model alias width across the full unfiltered list; stable for the lifetime of the picker.
    pub model_col_width: u16,
    /// Maximum provider alias width across the full unfiltered list; stable for the lifetime of the picker.
    pub provider_col_width: u16,
    /// Maximum context length label width across the full unfiltered list; stable for the lifetime of the picker.
    pub context_col_width: u16,
}

/// A model entry with metadata for rendering in the picker.
#[derive(Clone)]
pub struct ModelEntry {
    pub selection: ModelSelection,
    /// Whether this entry is the active (currently selected) model.
    pub is_active: bool,
}

/// Formats a context length in tokens as a compact string (e.g. `131072` → `"128k"`).
pub fn format_context_length(tokens: u32) -> String {
    format!("{}k", (tokens + 512) / 1024)
}

impl ModelPickerState {
    /// Creates a new `ModelPickerState` from the given model list and current active selection.
    pub fn new(models: Vec<ModelSelection>, active_selection: Option<ModelSelection>) -> Self {
        let model_col_width = models
            .iter()
            .map(|s| s.model_id.as_str().chars().count() as u16)
            .max()
            .unwrap_or(0);
        let provider_col_width = models
            .iter()
            .map(|s| s.provider_alias.as_str().chars().count() as u16)
            .max()
            .unwrap_or(0);
        let context_col_width = models
            .iter()
            .filter_map(|s| s.context_length)
            .map(|n| format_context_length(n).len() as u16)
            .max()
            .unwrap_or(0);

        let active = active_selection.clone();
        let entries: Vec<ModelEntry> = models
            .into_iter()
            .map(|s| ModelEntry {
                is_active: active.as_ref() == Some(&s),
                selection: s,
            })
            .collect();

        let picker = Picker::new(entries, |e| e.selection.model_id.as_str());
        Self {
            picker,
            active_selection,
            model_col_width,
            provider_col_width,
            context_col_width,
        }
    }

    /// Appends a character to the filter and recomputes filtered results.
    pub fn push_filter(&mut self, c: char) {
        self.picker.filter.push(c);
        self.picker.selected_idx = 0;
        self.picker.refilter(|e| e.selection.model_id.as_str());
    }

    /// Removes the last character from the filter and recomputes filtered results.
    pub fn pop_filter(&mut self) {
        self.picker.filter.pop();
        self.picker.selected_idx = 0;
        self.picker.refilter(|e| e.selection.model_id.as_str());
    }
}
