use nucleo_matcher::{
    Config, Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

/// Generic fuzzy-find picker: holds a list of items, a filter string, a cursor index,
/// and a cached matched view recomputed on every filter change.
///
/// The key function passed at construction is stored and used implicitly by `set_filter`
/// and `set_items` — callers never need to pass it again after `new`.
pub struct Picker<T> {
    items: Vec<T>,
    filter: String,
    /// Index of the highlighted row within `matched`.
    cursor: usize,
    /// Cached match results (item index, match positions); recomputed on every filter mutation.
    matched: Vec<(usize, Vec<u32>)>,
    key_fn: Box<dyn Fn(&T) -> &str>,
}

impl<T> Picker<T> {
    /// Creates a new picker from the given items, with an empty filter.
    ///
    /// `key_fn` extracts the string nucleo matches against. It is called immediately to
    /// populate `matched` and stored for implicit use by filter-mutation methods.
    pub fn new(items: Vec<T>, key_fn: impl Fn(&T) -> &str + 'static) -> Self {
        let mut picker = Self {
            items,
            filter: String::new(),
            cursor: 0,
            matched: Vec::new(),
            key_fn: Box::new(key_fn),
        };
        picker.rematch();
        picker
    }

    /// Returns the current filter string.
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Returns the number of items that pass the current filter.
    pub fn matched_count(&self) -> usize {
        self.matched.len()
    }

    /// Returns the index of the currently highlighted row within `matched`.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Replaces the filter string and recomputes matched results.
    pub fn set_filter(&mut self, filter: &str) {
        self.filter.clear();
        self.filter.push_str(filter);
        self.rematch();
    }

    /// Replaces all items and recomputes matched results.
    ///
    /// `cursor` is reset to 0.
    pub fn set_items(&mut self, items: Vec<T>) {
        self.items = items;
        self.rematch();
    }

    /// Moves `cursor` to the position of the first matched item satisfying `pred`.
    ///
    /// Does nothing if no item matches.
    pub fn set_selected(&mut self, pred: impl Fn(&T) -> bool) {
        if let Some(idx) = self.matched.iter().position(|(i, _)| pred(&self.items[*i])) {
            self.cursor = idx;
        }
    }

    /// Moves `cursor` to the position of the last matched item satisfying `pred`.
    ///
    /// Does nothing if no item matches.
    pub fn set_selected_last(&mut self, pred: impl Fn(&T) -> bool) {
        if let Some(idx) = self
            .matched
            .iter()
            .rposition(|(i, _)| pred(&self.items[*i]))
        {
            self.cursor = idx;
        }
    }

    /// Moves the cursor up by one, wrapping around.
    pub fn move_up(&mut self) {
        let count = self.matched.len();
        if count > 0 {
            self.cursor = self.cursor.checked_sub(1).unwrap_or(count - 1);
        }
    }

    /// Moves the cursor down by one, wrapping around.
    pub fn move_down(&mut self) {
        let count = self.matched.len();
        if count > 0 {
            self.cursor = (self.cursor + 1) % count;
        }
    }

    /// Returns the currently highlighted item, if any.
    pub fn selected(&self) -> Option<&T> {
        self.matched.get(self.cursor).map(|(i, _)| &self.items[*i])
    }

    /// Returns an iterator over matched items, pairing each with its match positions.
    pub fn matched_items(&self) -> impl Iterator<Item = MatchedItem<'_, T>> {
        self.matched.iter().map(|(i, indices)| MatchedItem {
            item: &self.items[*i],
            match_positions: indices.as_slice(),
        })
    }

    /// Recomputes `matched` from the current `filter` string.
    ///
    /// When the filter is empty all items are included in their original order with empty
    /// match positions. Otherwise entries are sorted by descending nucleo score and non-matching
    /// entries are excluded. `cursor` is reset to 0.
    fn rematch(&mut self) {
        self.cursor = 0;

        if self.filter.is_empty() {
            self.matched = self
                .items
                .iter()
                .enumerate()
                .map(|(i, _)| (i, Vec::new()))
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
        let mut scored: Vec<(u32, usize, Vec<u32>)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let mut indices = Vec::new();
                let score = pattern.indices(
                    nucleo_matcher::Utf32Str::new((self.key_fn)(item), &mut buf),
                    &mut matcher,
                    &mut indices,
                )?;
                indices.sort_unstable();
                Some((score, i, indices))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        self.matched = scored
            .into_iter()
            .map(|(_, i, indices)| (i, indices))
            .collect();
    }
}

/// A resolved match pairing an item reference with its match positions.
pub struct MatchedItem<'a, T> {
    pub item: &'a T,
    /// Character positions of matched chars within the key string. Empty when unfiltered.
    pub match_positions: &'a [u32],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str_picker(items: &[&'static str]) -> Picker<&'static str> {
        Picker::new(items.to_vec(), |s| s)
    }

    // --- Picker ---

    #[test]
    fn new_includes_all_items_unfiltered() {
        let p = str_picker(&["alpha", "beta", "gamma"]);
        assert_eq!(p.matched_count(), 3);
        assert_eq!(p.cursor(), 0);
        assert_eq!(p.filter(), "");
    }

    #[test]
    fn new_empty_items() {
        let p = str_picker(&[]);
        assert_eq!(p.matched_count(), 0);
        assert!(p.selected().is_none());
    }

    #[test]
    fn set_filter_narrows_results() {
        let mut p = str_picker(&["alpha", "beta", "gamma"]);
        p.set_filter("al");
        assert_eq!(p.matched_count(), 1);
        assert_eq!(*p.selected().unwrap(), "alpha");
    }

    #[test]
    fn set_filter_empty_restores_all() {
        let mut p = str_picker(&["alpha", "beta", "gamma"]);
        p.set_filter("al");
        p.set_filter("");
        assert_eq!(p.matched_count(), 3);
    }

    #[test]
    fn set_filter_resets_cursor() {
        let mut p = str_picker(&["alpha", "beta", "gamma"]);
        p.move_down();
        p.set_filter("beta");
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn set_filter_no_matches() {
        let mut p = str_picker(&["alpha", "beta"]);
        p.set_filter("zzz");
        assert_eq!(p.matched_count(), 0);
        assert!(p.selected().is_none());
    }

    #[test]
    fn set_items_replaces_list_and_resets() {
        let mut p = str_picker(&["alpha", "beta"]);
        p.move_down();
        p.set_items(vec!["foo", "bar", "baz"]);
        assert_eq!(p.matched_count(), 3);
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn move_down_wraps_around() {
        let mut p = str_picker(&["a", "b", "c"]);
        p.move_down();
        p.move_down();
        p.move_down(); // back to 0
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn move_up_wraps_around() {
        let mut p = str_picker(&["a", "b", "c"]);
        p.move_up(); // 0 → 2
        assert_eq!(p.cursor(), 2);
    }

    #[test]
    fn move_on_empty_list_is_noop() {
        let mut p = str_picker(&[]);
        p.move_down();
        p.move_up();
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn set_selected_moves_cursor_to_first_match() {
        let mut p = str_picker(&["alpha", "beta", "gamma"]);
        p.set_selected(|s| s.starts_with('g'));
        assert_eq!(*p.selected().unwrap(), "gamma");
    }

    #[test]
    fn set_selected_noop_when_no_match() {
        let mut p = str_picker(&["alpha", "beta", "gamma"]);
        p.move_down(); // cursor = 1
        p.set_selected(|s| s.starts_with('z'));
        assert_eq!(p.cursor(), 1); // unchanged
    }

    #[test]
    fn set_selected_last_moves_to_last_match() {
        let mut p = str_picker(&["alpha", "beta", "gamma"]);
        p.set_selected_last(|s| s.contains('a'));
        assert_eq!(*p.selected().unwrap(), "gamma");
    }

    #[test]
    fn matched_items_returns_all_unfiltered() {
        let p = str_picker(&["a", "b", "c"]);
        let items: Vec<_> = p.matched_items().collect();
        assert_eq!(items.len(), 3);
        assert!(items.iter().all(|m| m.match_positions.is_empty()));
    }

    #[test]
    fn matched_items_returns_match_positions_when_filtered() {
        let mut p = str_picker(&["hello", "world"]);
        p.set_filter("ell");
        let items: Vec<_> = p.matched_items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(*items[0].item, "hello");
        assert!(!items[0].match_positions.is_empty());
    }
}
