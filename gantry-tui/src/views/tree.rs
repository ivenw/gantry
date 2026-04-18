use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};

use crate::model::{TreeView, branch_rows};

pub struct TreeViewWidget<'a> {
    state: &'a TreeView,
}

impl<'a> TreeViewWidget<'a> {
    pub fn new(state: &'a TreeView) -> Self {
        Self { state }
    }
}

impl Widget for TreeViewWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::NONE);
        block.render(area, buf);

        let footer_height = 1u16;
        let inner = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        // TODO: Add a comment what this early return is for or remove it if it serves no practical
        // purpose.
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(footer_height)])
            .split(inner);

        let list_area = chunks[0];
        let footer_area = chunks[1];

        let viewport_height = list_area.height as usize;
        let rows = branch_rows(&self.state.tree.stem);
        let selected = self.state.selected_idx;

        let scroll = compute_scroll(self.state.scroll_offset, selected, viewport_height);

        let child_counts = build_child_counts(&rows);
        let is_last_sibling = build_last_sibling_flags(&rows);
        let is_branch_first = build_branch_first_flags(&rows);

        for (i, (node, depth)) in rows.iter().enumerate() {
            let row = i.wrapping_sub(scroll);
            if i < scroll || row >= viewport_height {
                continue;
            }
            let y = list_area.y + row as u16;

            let is_selected = i == selected;
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            let connector = build_connector(
                &rows,
                &child_counts,
                &is_last_sibling,
                &is_branch_first,
                i,
                *depth,
            );

            let leaf_marker = if self.state.tree.current_leaf_id.as_deref() == Some(&node.id) {
                CURRENT_LEAF_MARKER
            } else {
                " "
            };
            let role_label = match node.role {
                gantry_core::Role::User => "USER",
                gantry_core::Role::Assistant => "GNTR",
                gantry_core::Role::Error => "ERR",
            };
            // Col 0 is reserved for the leaf marker; connector and role start at col 1.
            let body = format!(" {}[{}]: ", connector, role_label);
            let body_width = body.chars().count();
            let max_width = list_area.width as usize;
            let content_budget = max_width.saturating_sub(body_width);
            let single_line: String = node
                .content
                .chars()
                .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
                .collect();
            let content = truncate_to_width(&single_line, content_budget);

            if is_selected {
                for x in 0..list_area.width {
                    if let Some(cell) = buf.cell_mut((list_area.x + x, y)) {
                        cell.set_style(style);
                    }
                }
            }
            buf.set_string(list_area.x, y, format!("{}{}", body, content), style);
            buf.set_string(
                list_area.x,
                y,
                leaf_marker,
                Style::default().fg(Color::Cyan),
            );
        }

        let footer = " ↑↓ navigate   Enter select   Esc cancel ";
        let footer_style = Style::default().fg(Color::DarkGray);
        buf.set_string(footer_area.x, footer_area.y, footer, footer_style);
    }
}

const TRUNCATION_SUFFIX: &str = "...";
const CURRENT_LEAF_MARKER: &str = ">";
const TREE_PIPE: &str = "|  ";
const TREE_INDENT: &str = "   ";
const TREE_BRANCH: &str = "+- ";
const TREE_LAST: &str = "\\- ";

fn truncate_to_width(s: &str, width: usize) -> String {
    let suffix_len = TRUNCATION_SUFFIX.chars().count();
    if width == 0 {
        return String::new();
    }
    let mut chars = s.chars();
    let preview: String = chars
        .by_ref()
        .take(width.saturating_sub(suffix_len))
        .collect();
    if chars.next().is_some() {
        format!("{}{}", preview, TRUNCATION_SUFFIX)
    } else {
        s.chars().take(width).collect()
    }
}

fn compute_scroll(current_scroll: usize, selected: usize, viewport: usize) -> usize {
    if selected < current_scroll {
        selected
    } else if selected >= current_scroll + viewport {
        selected.saturating_sub(viewport - 1)
    } else {
        current_scroll
    }
}

type Row<'a> = (&'a gantry_core::BranchNode, usize);

fn build_child_counts(rows: &[Row<'_>]) -> Vec<usize> {
    let mut counts = vec![0usize; rows.len()];
    for i in 0..rows.len() {
        let d = rows[i].1;
        let mut count = 0;
        #[allow(clippy::needless_range_loop)]
        for j in (i + 1)..rows.len() {
            if rows[j].1 <= d {
                break;
            }
            if rows[j].1 == d + 1 {
                count += 1;
            }
        }
        counts[i] = count;
    }
    counts
}

fn build_last_sibling_flags(rows: &[Row<'_>]) -> Vec<bool> {
    let mut flags = vec![false; rows.len()];
    for i in 0..rows.len() {
        let depth = rows[i].1;
        let mut is_last = true;
        #[allow(clippy::needless_range_loop)]
        for j in (i + 1)..rows.len() {
            if rows[j].1 < depth {
                break;
            }
            if rows[j].1 == depth {
                is_last = false;
                break;
            }
        }
        flags[i] = is_last;
    }
    flags
}

fn build_branch_first_flags(rows: &[Row<'_>]) -> Vec<bool> {
    let mut flags = vec![true; rows.len()];
    for i in 1..rows.len() {
        let depth = rows[i].1;
        // Look backwards for the nearest node at the same depth or shallower.
        // If we find one at the exact same depth before hitting a shallower node,
        // this node is not the first in its branch sequence.
        for j in (0..i).rev() {
            let d = rows[j].1;
            if d < depth {
                break;
            }
            if d == depth {
                flags[i] = false;
                break;
            }
        }
    }
    flags
}

fn build_connector(
    rows: &[Row<'_>],
    child_counts: &[usize],
    last_sibling_flags: &[bool],
    branch_first_flags: &[bool],
    idx: usize,
    depth: usize,
) -> String {
    if depth == 0 {
        return String::new();
    }

    let mut ancestors: Vec<(usize, bool)> = Vec::with_capacity(depth);
    let mut current = idx;
    for d in (0..depth).rev() {
        if let Some(anc) = rows[..current].iter().rposition(|(_, pd)| *pd == d) {
            let branched = child_counts[anc] >= 2;
            ancestors.push((anc, branched));
            current = anc;
        }
    }
    ancestors.reverse();

    let parent_idx = ancestors.last().map(|(i, _)| *i);
    let parent_branched = parent_idx.map(|p| child_counts[p] >= 2).unwrap_or(false);

    let mut result = String::new();

    let is_first = branch_first_flags[idx];

    for k in 0..ancestors.len().saturating_sub(1) {
        let (_, branched) = ancestors[k];
        let (child_on_path, _) = ancestors[k + 1];
        if branched && !last_sibling_flags[child_on_path] {
            result.push_str(TREE_PIPE);
        } else {
            result.push_str(TREE_INDENT);
        }
    }

    if is_first {
        if parent_branched {
            if last_sibling_flags[idx] {
                result.push_str(TREE_LAST);
            } else {
                result.push_str(TREE_BRANCH);
            }
        }
    } else {
        // Find the first sibling (first node in this branch sequence) to know if the
        // branch is still open, so we can pick pipe vs indent for the prefix segment.
        let first_sibling = rows[..idx]
            .iter()
            .rposition(|(_, d)| *d < depth)
            .map(|anc| anc + 1)
            .unwrap_or(0);
        let branch_open = !last_sibling_flags[first_sibling];
        if parent_branched {
            result.push_str(if branch_open { TREE_PIPE } else { TREE_INDENT });
        }
        result.push_str(TREE_PIPE);
    }

    result
}
