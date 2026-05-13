use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::Widget,
};

/// A fixed-layout multi-column table widget with per-cell span styling.
///
/// Column widths are provided for the first N-1 columns; the last column fills the remaining
/// render area. A uniform gap is inserted between every column. Rows are passed pre-sliced
/// to the visible window — no internal scroll offset.
pub struct TableWidget<'a> {
    /// Widths for all columns except the last, which fills remaining space.
    col_widths: Vec<u16>,
    /// Uniform gap in characters between every adjacent pair of columns.
    gap: u16,
    rows: Vec<Vec<Line<'a>>>,
}

impl<'a> TableWidget<'a> {
    /// Creates a `TableView`.
    ///
    /// `col_widths` must contain exactly N-1 entries for an N-column table. Passing more widths
    /// than columns is allowed; extra entries are ignored. Passing a single-column table with an
    /// empty `col_widths` is valid — the sole column fills the entire area.
    pub fn new(col_widths: Vec<u16>, gap: u16, rows: Vec<Vec<Line<'a>>>) -> Self {
        Self {
            col_widths,
            gap,
            rows,
        }
    }
}

impl Widget for TableWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        for (row_idx, row) in self.rows.iter().enumerate().take(area.height as usize) {
            let y = area.y + row_idx as u16;
            let mut x = area.x;

            let num_cols = row.len();
            for col_idx in 0..num_cols {
                let remaining = area.width.saturating_sub(x.saturating_sub(area.x));
                if remaining == 0 {
                    break;
                }

                let is_last = col_idx + 1 == num_cols;
                let col_width = if is_last {
                    remaining
                } else {
                    self.col_widths
                        .get(col_idx)
                        .copied()
                        .unwrap_or(0)
                        .min(remaining)
                };

                let chars_written = render_cell(buf, x, y, col_width, row.get(col_idx));

                // Pad cell to full column width with spaces.
                for pad in chars_written..col_width as usize {
                    buf.set_string(x + pad as u16, y, " ", ratatui::style::Style::default());
                }

                x += col_width;

                if !is_last {
                    let gap = self
                        .gap
                        .min(area.width.saturating_sub(x.saturating_sub(area.x)));
                    for g in 0..gap {
                        buf.set_string(x + g, y, " ", ratatui::style::Style::default());
                    }
                    x += gap;
                }
            }
        }
    }
}

/// Renders a single cell into the buffer, truncating at `max_width`. Returns chars written.
///
/// When the cell content exceeds `max_width`, the last three characters are replaced with `...`
/// using the style of the span at that position.
fn render_cell(buf: &mut Buffer, x: u16, y: u16, max_width: u16, cell: Option<&Line<'_>>) -> usize {
    let Some(line) = cell else {
        return 0;
    };

    let max = max_width as usize;
    let needs_ellipsis = line_width(line) > max;
    // Reserve space for "..." only when truncation will actually occur.
    let content_limit = if needs_ellipsis && max >= 3 {
        max - 3
    } else {
        max
    };

    let mut written = 0usize;
    let mut ellipsis_style = ratatui::style::Style::default();
    'outer: for span in line.spans.iter() {
        for ch in span.content.chars() {
            if written >= content_limit {
                ellipsis_style = span.style;
                break 'outer;
            }
            buf.set_string(x + written as u16, y, ch.to_string(), span.style);
            written += 1;
        }
    }

    if needs_ellipsis && max >= 3 {
        for dot in [".", ".", "."] {
            buf.set_string(x + written as u16, y, dot, ellipsis_style);
            written += 1;
        }
    }

    written
}

/// Returns the total character width of a `Line` by summing span content lengths.
fn line_width(line: &Line<'_>) -> usize {
    line.spans.iter().map(|s| s.content.chars().count()).sum()
}

/// Builds a `Line` from text and match indices, styling matched chars with `match_style`
/// and unmatched chars with `base_style`.
pub fn highlighted_line<'a>(
    text: &str,
    indices: &[u32],
    base_style: ratatui::style::Style,
    match_style: ratatui::style::Style,
) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut current_text = String::new();
    let mut current_is_match = false;

    for (char_idx, ch) in text.chars().enumerate() {
        let is_match = indices.contains(&(char_idx as u32));
        if char_idx == 0 {
            current_is_match = is_match;
        }
        if is_match != current_is_match {
            let style = if current_is_match {
                match_style
            } else {
                base_style
            };
            spans.push(Span::styled(current_text.clone(), style));
            current_text.clear();
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
