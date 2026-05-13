use ratatui::{style::Color, symbols::border};

use crate::model::Mode;

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
        horizontal_top: "-",
        horizontal_bottom: "-",
        vertical_left: " ",
        vertical_right: " ",
    }
}
