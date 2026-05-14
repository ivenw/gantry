use unicode_width::UnicodeWidthChar;

use ratatui::{buffer::Buffer, layout::Rect, text::Line, widgets::Widget};

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
    let mut char_buf = [0u8; 4];
    'outer: for span in line.spans.iter() {
        for ch in span.content.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if written + ch_width > content_limit {
                ellipsis_style = span.style;
                break 'outer;
            }
            buf.set_string(
                x + written as u16,
                y,
                ch.encode_utf8(&mut char_buf),
                span.style,
            );
            written += ch_width;
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

/// Returns the total display-column width of a `Line`, accounting for wide Unicode characters.
fn line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .flat_map(|s| s.content.chars())
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}
