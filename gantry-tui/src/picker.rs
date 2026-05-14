use nucleo_matcher::{
    Config, Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};
use ratatui::{
    style::Style,
    text::{Line, Span},
};

/// A filtered entry produced by `Picker::refilter`.
#[derive(Clone)]
pub struct FilteredItem {
    /// Index into `Picker::items`.
    pub idx: usize,
    /// Matched character indices into the key string from the last fuzzy filter. Empty when unfiltered.
    pub indices: Vec<u32>,
}

/// Generic fuzzy-find picker: holds a list of items, a filter string, a cursor index,
/// and a cached filtered view recomputed on every filter change.
pub struct Picker<T> {
    pub items: Vec<T>,
    pub filter: String,
    /// Index of the highlighted row within `filtered`.
    pub selected_idx: usize,
    /// Cached fuzzy-filtered results; recomputed on every call to `refilter`.
    pub filtered: Vec<FilteredItem>,
}

impl<T> Picker<T> {
    /// Creates a new picker from the given items, with an empty filter.
    ///
    /// `key_fn` is used to extract the string that nucleo matches against; it is called
    /// immediately to populate `filtered`.
    pub fn new(items: Vec<T>, key_fn: impl Fn(&T) -> &str) -> Self {
        let mut picker = Self {
            items,
            filter: String::new(),
            selected_idx: 0,
            filtered: Vec::new(),
        };
        picker.refilter(key_fn);
        picker
    }

    /// Recomputes `filtered` from the current `filter` string.
    ///
    /// When the filter is empty all items are included in their original order with empty
    /// `indices`. Otherwise entries are sorted by descending nucleo score and non-matching
    /// entries are excluded. `selected_idx` is reset to 0.
    pub fn refilter(&mut self, key_fn: impl Fn(&T) -> &str) {
        self.selected_idx = 0;

        if self.filter.is_empty() {
            self.filtered = self
                .items
                .iter()
                .enumerate()
                .map(|(idx, _)| FilteredItem {
                    idx,
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
        let mut scored: Vec<(u32, FilteredItem)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let mut indices = Vec::new();
                let score = pattern.indices(
                    nucleo_matcher::Utf32Str::new(key_fn(item), &mut buf),
                    &mut matcher,
                    &mut indices,
                )?;
                indices.sort_unstable();
                Some((score, FilteredItem { idx, indices }))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        self.filtered = scored.into_iter().map(|(_, e)| e).collect();
    }

    /// Moves the cursor up by one, wrapping around.
    pub fn move_up(&mut self) {
        let count = self.filtered.len();
        if count > 0 {
            self.selected_idx = self.selected_idx.checked_sub(1).unwrap_or(count - 1);
        }
    }

    /// Moves the cursor down by one, wrapping around.
    pub fn move_down(&mut self) {
        let count = self.filtered.len();
        if count > 0 {
            self.selected_idx = (self.selected_idx + 1) % count;
        }
    }

    /// Returns the currently highlighted item, if any.
    pub fn selected(&self) -> Option<&T> {
        self.filtered
            .get(self.selected_idx)
            .map(|e| &self.items[e.idx])
    }
}

/// Builds a `Line` from text and match indices, styling matched chars with `match_style`
/// and unmatched chars with `base_style`.
///
/// `indices` must be sorted in ascending order.
pub fn highlight_matched_chars<'a>(
    text: &str,
    indices: &[u32],
    base_style: Style,
    match_style: Style,
) -> Line<'a> {
    debug_assert!(
        indices.windows(2).all(|w| w[0] <= w[1]),
        "highlighted_line: indices must be sorted"
    );

    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut current_text = String::new();
    let mut current_is_match = false;

    for (char_idx, ch) in text.chars().enumerate() {
        let is_match = indices.binary_search(&(char_idx as u32)).is_ok();
        if char_idx == 0 {
            current_is_match = is_match;
        }
        if is_match != current_is_match {
            let style = if current_is_match {
                match_style
            } else {
                base_style
            };
            spans.push(Span::styled(std::mem::take(&mut current_text), style));
            current_is_match = is_match;
        }
        current_text.push(ch);
    }

    if !current_text.is_empty() {
        let style = if current_is_match {
            match_style
        } else {
            base_style
        };
        spans.push(Span::styled(current_text, style));
    }

    Line::from(spans)
}
