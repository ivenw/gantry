use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::features::session_picker::SessionPickerState;
use crate::theme;
use crate::theme::title;
use crate::utils::highlight_matched_chars;
use crate::widgets::table::TableWidget;

const MAX_VISIBLE: usize = 10;

/// Overhead rows: top border + prompt row + blank separator + counter row + bottom border.
const CHROME_HEIGHT: u16 = 5;

const STYLE_TEXT: Style = Style::new().fg(Color::White);
const STYLE_MATCH: Style = Style::new().fg(Color::LightCyan);
const STYLE_SELECTED: Style = Style::new().fg(Color::LightCyan).bold();
const STYLE_ACTIVE: Style = Style::new().fg(Color::White);
const STYLE_AGE: Style = Style::new().fg(Color::DarkGray);

pub struct SessionPickerWidget<'a> {
    state: &'a SessionPickerState,
}

impl<'a> SessionPickerWidget<'a> {
    /// Creates a widget for the sessions browser overlay.
    pub fn new(state: &'a SessionPickerState) -> Self {
        Self { state }
    }

    /// Returns the total height needed to render the sessions list, capped at `MAX_VISIBLE` content rows.
    pub fn height(&self) -> u16 {
        CHROME_HEIGHT + self.state.picker.matched_count().clamp(1, MAX_VISIBLE) as u16
    }
}

impl Widget for SessionPickerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let picker = &self.state.picker;
        let count = picker.matched_count();

        let block = Block::default()
            .title(title("SESSIONS"))
            .borders(Borders::ALL)
            .border_set(theme::border_set())
            .border_style(Style::default().fg(Color::Gray));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let list_height = count.clamp(1, MAX_VISIBLE) as u16;
        let [prompt_area, _, list_area, counter_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(list_height),
                Constraint::Length(1),
            ])
            .areas(inner);

        Line::from(format!("> {}", picker.filter())).render(prompt_area, buf);

        if count == 0 {
            Line::styled("No matches", Style::default().fg(Color::DarkGray)).render(list_area, buf);
            return;
        }

        let selected = picker.cursor();
        let max_visible = (list_area.height as usize).min(MAX_VISIBLE);

        // Bottom-anchor scroll: the selected item sits at the bottom of the visible window
        // until doing so would extend past the end of the list, at which point the window
        // is pinned to show the last `max_visible` items.
        let start = selected
            .saturating_sub(max_visible - 1)
            .min(count.saturating_sub(max_visible));

        // name column (marker + name) + gap + age column (last, fills remainder)
        let rows: Vec<Vec<Line>> = picker
            .matched_items()
            .enumerate()
            .skip(start)
            .take(max_visible)
            .map(|(i, matched)| {
                let is_selected = i == selected;
                let is_active = matched.item.id == self.state.active_session_id;

                let name = &matched.item.first_message;
                let age = relative_time(&matched.item.timestamp);

                let name_line = if is_selected {
                    let marker = if is_active { "> " } else { "  " };
                    Line::from(vec![
                        Span::styled(marker, STYLE_SELECTED),
                        Span::styled(name.clone(), STYLE_SELECTED),
                    ])
                } else if is_active {
                    Line::from(vec![
                        Span::styled("> ", STYLE_TEXT),
                        Span::styled(name.clone(), STYLE_ACTIVE),
                    ])
                } else {
                    let mut spans = vec![Span::styled("  ", STYLE_TEXT)];
                    spans.extend(
                        highlight_matched_chars(
                            name,
                            matched.match_positions,
                            STYLE_TEXT,
                            STYLE_MATCH,
                        )
                        .spans,
                    );
                    Line::from(spans)
                };

                vec![name_line, Line::from(Span::styled(age, STYLE_AGE))]
            })
            .collect();

        TableWidget::new(vec![self.state.name_col_width], 4, rows).render(list_area, buf);

        theme::counter_line(selected + 1, count).render(counter_area, buf);
    }
}

/// Formats a timestamp as a compact relative age string.
fn relative_time(ts: &jiff::Timestamp) -> String {
    let now = jiff::Timestamp::now();
    let secs = now.duration_since(*ts).as_secs().max(0) as u64;

    const MIN: u64 = 60;
    const HOUR: u64 = 60 * MIN;
    const DAY: u64 = 24 * HOUR;
    const WEEK: u64 = 7 * DAY;

    if secs < MIN {
        format!("{}s", secs)
    } else if secs < HOUR {
        format!("{}m", secs / MIN)
    } else if secs < DAY {
        format!("{}h", secs / HOUR)
    } else if secs < WEEK {
        format!("{}d", secs / DAY)
    } else {
        format!("{}w", secs / WEEK)
    }
}
