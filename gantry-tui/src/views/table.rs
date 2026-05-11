use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::Widget,
};

/// Specifies layout constraints for a single column.
#[derive(Default)]
pub struct ColumnSpec {
    /// Minimum number of spaces between this column and the next. Ignored for the last column.
    min_gap: u16,
    /// Maximum rendered width of this column in characters. Overflow is truncated.
    max_width: Option<u16>,
}

impl ColumnSpec {
    /// Creates a `ColumnSpec` with the given minimum gap and optional maximum width.
    pub fn new(min_gap: u16, max_width: Option<u16>) -> Self {
        Self { min_gap, max_width }
    }
}

/// A fixed-layout multi-column table widget with per-cell span styling.
///
/// Column widths are computed once at construction from the widest cell in each column,
/// clamped to `max_width` if set. Cells are provided in row-major order as `Vec<Line>`,
/// allowing arbitrary per-character styling (e.g. fuzzy-match highlights).
pub struct TableView<'a> {
    columns: Vec<ColumnSpec>,
    rows: Vec<Vec<Line<'a>>>,
    col_widths: Vec<u16>,
}

impl<'a> TableView<'a> {
    /// Creates a `TableView` from column specs and row-major cell data.
    ///
    /// Column widths are derived from the widest cell in each column, then clamped to
    /// `max_width`. Rows with fewer cells than columns are padded; extra cells are ignored.
    pub fn new(columns: Vec<ColumnSpec>, rows: Vec<Vec<Line<'a>>>) -> Self {
        let n = columns.len();
        let mut col_widths = vec![0u16; n];

        for row in &rows {
            debug_assert_eq!(
                row.len(),
                n,
                "row has {} cells but table has {} columns",
                row.len(),
                n
            );
            for (col_idx, cell) in row.iter().enumerate().take(n) {
                let w = line_width(cell) as u16;
                if w > col_widths[col_idx] {
                    col_widths[col_idx] = w;
                }
            }
        }

        for (col_idx, spec) in columns.iter().enumerate() {
            if let Some(max) = spec.max_width {
                col_widths[col_idx] = col_widths[col_idx].min(max);
            }
        }

        Self {
            columns,
            rows,
            col_widths,
        }
    }
}

impl Widget for TableView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        for (row_idx, row) in self.rows.iter().enumerate().take(area.height as usize) {
            let y = area.y + row_idx as u16;
            let mut x = area.x;

            for (col_idx, spec) in self.columns.iter().enumerate() {
                let col_width = self.col_widths[col_idx];
                let remaining = area.width.saturating_sub(x.saturating_sub(area.x));
                if remaining == 0 {
                    break;
                }

                let render_width = col_width.min(remaining);
                let cell = row.get(col_idx);
                let chars_written = render_cell(buf, x, y, render_width, cell);

                // Pad cell to full column width with spaces.
                for pad in chars_written..render_width as usize {
                    buf.set_string(x + pad as u16, y, " ", ratatui::style::Style::default());
                }

                x += render_width;

                // Gap after all columns except the last.
                if col_idx + 1 < self.columns.len() {
                    let gap = spec.min_gap.min(area.width.saturating_sub(x.saturating_sub(area.x)));
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
    let content_limit = if needs_ellipsis && max >= 3 { max - 3 } else { max };

    let mut written = 0usize;
    let mut ellipsis_style = ratatui::style::Style::default();
    'outer: for span in line.spans.iter() {
        for ch in span.content.chars() {
            if written >= content_limit {
                ellipsis_style = span.style;
                break 'outer;
            }
            buf.set_string(x + written as u16, y, &ch.to_string(), span.style);
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
            let style = if current_is_match { match_style } else { base_style };
            spans.push(Span::styled(current_text.clone(), style));
            current_text.clear();
            current_is_match = is_match;
        }
        current_text.push(ch);
    }

    if !current_text.is_empty() {
        let style = if current_is_match { match_style } else { base_style };
        spans.push(Span::styled(current_text, style));
    }

    Line::from(spans)
}
