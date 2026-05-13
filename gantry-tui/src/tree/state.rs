use gantry_core::{Branch, SessionTree};

/// State for the session tree overlay.
pub struct TreeState {
    pub tree: SessionTree,
    /// Index into the DFS row order of the currently highlighted row.
    pub selected_idx: usize,
    /// First visible row index (scroll offset).
    pub scroll_offset: usize,
}

/// Flattens a `Branch` tree into a DFS-ordered list of `(branch, depth)` pairs for row-indexed access.
pub fn branch_rows(branch: &Branch, depth: usize) -> Vec<(&Branch, usize)> {
    let mut rows = vec![(branch, depth)];
    for sub in &branch.branches {
        rows.extend(branch_rows(sub, depth + 1));
    }
    rows
}
