use ratatui::{
    style::{Color, Style},
    symbols::border,
    text::Line,
};

use crate::model::Mode;

const BORDER: &str = "-";

/// Returns the color associated with the given chat mode.
pub fn mode_color(mode: Mode) -> Color {
    match mode {
        Mode::Normal => Color::DarkGray,
        Mode::Insert => Color::LightGreen,
    }
}

/// Returns the border set used for UI panels.
pub fn border_set() -> border::Set<'static> {
    border::Set {
        top_left: "+",
        top_right: "+",
        bottom_left: "+",
        bottom_right: "+",
        horizontal_top: BORDER,
        horizontal_bottom: BORDER,
        vertical_left: " ",
        vertical_right: " ",
    }
}

pub fn title(value: &str) -> String {
    format!("{}[{}]", BORDER, value)
}

/// Returns a styled `(current/total)` counter line.
pub fn counter_line(current: usize, total: usize) -> Line<'static> {
    Line::styled(
        format!("({}/{})", current, total),
        Style::new().fg(Color::DarkGray),
    )
}
