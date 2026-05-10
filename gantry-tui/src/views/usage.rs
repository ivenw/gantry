use gantry_core::ContextWindow;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

use crate::model::UsageView;

/// Colors for each usage bar segment, in render order: system prompt, messages, other, remaining.
const COLOR_SYSTEM: Color = Color::Cyan;
const COLOR_MESSAGES: Color = Color::Blue;
const COLOR_OTHER: Color = Color::Gray;
const COLOR_REMAINING: Color = Color::DarkGray;

pub struct UsageViewWidget<'a> {
    state: &'a UsageView,
}

impl<'a> UsageViewWidget<'a> {
    pub fn new(state: &'a UsageView) -> Self {
        Self { state }
    }

    /// Computes the total height needed to render the overlay at the given width.
    pub fn calc_height(&self) -> u16 {
        let cw = &self.state.context_window;
        let agent_file_rows = cw.agent_files_tokens.len() as u16;
        // borders(2) + bar(1) + blank(1) + header(1) + system_prompt(1) + base_prompt(1) + agent_files(N) + messages(1) + other(1) + remaining(1)
        2 + 1 + 1 + 1 + 1 + 1 + agent_file_rows + 1 + 1 + 1
    }
}

impl Widget for UsageViewWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Context Window Usage ")
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(Color::DarkGray));
        block.render(area, buf);

        let inner = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let cw = &self.state.context_window;
        let mut y = inner.y;

        render_bar(buf, inner.x, y, inner.width, cw);
        y += 1;

        // Blank separator between bar and table.
        y += 1;

        render_breakdown(buf, inner.x, y, inner.width, cw);
    }
}

/// Renders the colored usage bar spanning the full inner width.
///
/// Each segment is scaled relative to `context_length` so the remaining capacity is visible.
/// When `context_length` is unknown the bar fills entirely with the used segments.
fn render_bar(buf: &mut Buffer, x: u16, y: u16, width: u16, cw: &ContextWindow) {
    if width == 0 {
        return;
    }

    let ctx = cw.max_tokens as f32;
    let w = width as f32;

    let scale = |tokens: u32| -> u16 { ((tokens as f32 / ctx) * w).round() as u16 };

    let sys_cols = scale(cw.system_prompt_tokens());
    let msg_cols = scale(cw.messages_tokens);
    let other_cols = scale(cw.other_tokens);
    let rem_cols = width.saturating_sub(sys_cols + msg_cols + other_cols);

    let mut cursor = x;
    fill_bar(buf, cursor, y, sys_cols, COLOR_SYSTEM);
    cursor += sys_cols;
    fill_bar(buf, cursor, y, msg_cols, COLOR_MESSAGES);
    cursor += msg_cols;
    fill_bar(buf, cursor, y, other_cols, COLOR_OTHER);
    cursor += other_cols;
    fill_bar(buf, cursor, y, rem_cols, COLOR_REMAINING);
}

fn fill_bar(buf: &mut Buffer, x: u16, y: u16, width: u16, color: Color) {
    for col in x..x + width {
        buf.cell_mut((col, y)).map(|c| c.set_char(' ').set_bg(color));
    }
}

/// Renders the token breakdown table below the bar.
fn render_breakdown(buf: &mut Buffer, x: u16, y: u16, width: u16, cw: &ContextWindow) {
    let total = cw.total_tokens;
    let mut row = y;

    let header = format!(
        "{} / {} tokens used  ({:.1}%)",
        fmt_tokens(total),
        fmt_tokens(cw.max_tokens),
        cw.usage_fraction() * 100.0,
    );
    buf.set_string(x, row, &header, Style::default().fg(Color::White));
    row += 1;

    // System prompt section.
    let sys_tokens = cw.system_prompt_tokens();
    let sys_pct = cw.system_prompt_fraction() * 100.0;
    render_row(buf, x, row, width, "System prompt", sys_tokens, sys_pct, COLOR_SYSTEM, 0);
    row += 1;

    // Base prompt sub-row (indented).
    let bp_tokens = cw.base_prompt_tokens;
    let bp_pct = cw.base_prompt_fraction() * 100.0;
    render_row(buf, x, row, width, "Base prompt", bp_tokens, bp_pct, COLOR_SYSTEM, 2);
    row += 1;

    // Agent file sub-rows (indented).
    for (path, pct) in cw.agent_files_fraction() {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let tokens = cw
            .agent_files_tokens
            .iter()
            .find(|(p, _)| p == &path)
            .map(|(_, t)| *t)
            .unwrap_or(0);
        render_row(buf, x, row, width, name, tokens, pct * 100.0, COLOR_SYSTEM, 2);
        row += 1;
    }

    // Messages row.
    let msg_tokens = cw.messages_tokens;
    let msg_pct = cw.messages_fraction() * 100.0;
    render_row(buf, x, row, width, "Messages", msg_tokens, msg_pct, COLOR_MESSAGES, 0);
    row += 1;

    // Other row.
    let other_tokens = cw.other_tokens;
    let other_pct = cw.other_fraction() * 100.0;
    render_row(buf, x, row, width, "Other", other_tokens, other_pct, COLOR_OTHER, 0);

    render_row(
        buf,
        x,
        row + 1,
        width,
        "Remaining",
        cw.remaining_tokens(),
        cw.remaining_fraction() * 100.0,
        COLOR_REMAINING,
        0,
    );
}

/// Renders a single labeled row with a right-aligned token count and percentage.
fn render_row(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    label: &str,
    tokens: u32,
    pct: f32,
    color: Color,
    indent: u16,
) {
    let right_col = format!("{:>7}  {:>5.1}%", fmt_tokens(tokens), pct);
    let right_len = right_col.len() as u16;
    let label_x = x + indent;
    let right_x = x + width.saturating_sub(right_len);

    buf.set_string(
        label_x,
        y,
        label,
        Style::default().fg(color),
    );
    buf.set_string(
        right_x,
        y,
        &right_col,
        Style::default().fg(Color::Gray),
    );
}

/// Formats a token count, abbreviating values ≥ 1000 as e.g. `12.3k`.
fn fmt_tokens(n: u32) -> String {
    if n >= 1000 {
        let k = n as f64 / 1000.0;
        // One significant digit in the fractional part.
        format!("{:.1}k", k)
    } else {
        n.to_string()
    }
}
