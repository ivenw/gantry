use ratatui::symbols::border;

/// Returns the border set used for UI panels.
pub fn border_set() -> border::Set<'static> {
    border::Set {
        top_left: "+",
        top_right: "+",
        bottom_left: "+",
        bottom_right: "+",
        horizontal_top: "-",
        horizontal_bottom: "-",
        vertical_left: "|",
        vertical_right: "|",
    }
}
