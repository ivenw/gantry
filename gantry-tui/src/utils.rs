use ratatui::{
    style::Style,
    text::{Line, Span},
};

/// Builds a `Line` from text and match positions, styling matched chars with `match_style`
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

/// Returns the number of visual lines a string occupies when wrapped at `text_width` columns.
pub fn wrapped_line_count(value: &str, text_width: usize) -> usize {
    if value.is_empty() {
        return 1;
    }

    value
        .split('\n')
        .map(|line| {
            let char_count = line.chars().count();
            if char_count == 0 {
                1
            } else {
                char_count.div_ceil(text_width)
            }
        })
        .sum::<usize>()
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn highlight_no_indices_produces_single_base_span() {
        let base = Style::default().fg(Color::White);
        let highlight = Style::default().fg(Color::Red);
        let line = highlight_matched_chars("hello", &[], base, highlight);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content, "hello");
        assert_eq!(line.spans[0].style, base);
    }

    #[test]
    fn highlight_all_indices_produces_single_match_span() {
        let base = Style::default();
        let highlight = Style::default().fg(Color::Red);
        let line = highlight_matched_chars("hi", &[0, 1], base, highlight);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content, "hi");
        assert_eq!(line.spans[0].style, highlight);
    }

    #[test]
    fn highlight_mixed_indices_alternates_spans() {
        let base = Style::default().fg(Color::White);
        let highlight = Style::default().fg(Color::Red);
        // "hello" — match 'h' (0) and 'l' (2), rest unmatched
        let line = highlight_matched_chars("hello", &[0, 2], base, highlight);
        // spans: "h"(match), "e"(base), "l"(match), "lo"(base)
        assert_eq!(line.spans.len(), 4);
        assert_eq!(line.spans[0].content, "h");
        assert_eq!(line.spans[0].style, highlight);
        assert_eq!(line.spans[1].content, "e");
        assert_eq!(line.spans[1].style, base);
        assert_eq!(line.spans[2].content, "l");
        assert_eq!(line.spans[2].style, highlight);
        assert_eq!(line.spans[3].content, "lo");
        assert_eq!(line.spans[3].style, base);
    }

    #[test]
    fn highlight_empty_text_produces_no_spans() {
        let base = Style::default();
        let highlight = Style::default().fg(Color::Red);
        let line = highlight_matched_chars("", &[], base, highlight);
        assert!(line.spans.is_empty());
    }
}
