use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, BorderType, Borders, Widget},
};

#[derive(Clone, Debug)]
pub struct Command {
    pub name: String,
    pub description: String,
}

#[derive(Clone)]
pub struct CommandPickerView {
    commands: Vec<Command>,
    filter: String,
    selected_index: usize,
}

impl CommandPickerView {
    pub fn new(commands: Vec<Command>) -> Self {
        Self {
            commands,
            filter: String::new(),
            selected_index: 0,
        }
    }

    pub fn set_filter(&mut self, filter: &str) {
        self.filter = filter.to_string();
        self.selected_index = 0;
    }

    pub fn selected_command(&self) -> Option<&Command> {
        let filtered = self.filtered_commands();
        filtered.get(self.selected_index).copied()
    }

    pub fn move_selection_up(&mut self) {
        let filtered = self.filtered_commands();
        if !filtered.is_empty() {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
    }

    pub fn move_selection_down(&mut self) {
        let filtered = self.filtered_commands();
        if !filtered.is_empty() {
            self.selected_index = (self.selected_index + 1) % filtered.len();
        }
    }

    fn filtered_commands(&self) -> Vec<&Command> {
        if self.filter.is_empty() {
            return self.commands.iter().collect();
        }
        self.commands
            .iter()
            .filter(|cmd| cmd.name.starts_with(&self.filter))
            .collect()
    }

    pub fn calc_height(&self, width: u16) -> u16 {
        let filtered = self.filtered_commands();

        if filtered.is_empty() {
            return 3;
        }

        let text_width = (width - 4).max(1) as usize;
        let mut height = 0u16;

        for cmd in &filtered {
            let desc_len = cmd.description.len();
            let wrapped_lines = if desc_len == 0 {
                1
            } else {
                desc_len.div_ceil(text_width)
            };
            height += wrapped_lines.max(1) as u16;
        }

        (height + 2).max(3)
    }
}

impl Widget for &CommandPickerView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let filtered = self.filtered_commands();

        if filtered.is_empty() {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray))
                .title(" No commands ");
            block.render(area, buf);
            return;
        }

        let inner_area = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(1),
            area.height.saturating_sub(2),
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray))
            .title(" Commands ");
        block.render(area, buf);

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        let mut y = inner_area.y;
        let text_width = inner_area.width as usize;

        for (i, cmd) in filtered.iter().enumerate() {
            let is_selected = i == self.selected_index;
            let style = if is_selected {
                ratatui::style::Style::default()
                    .fg(ratatui::style::Color::Black)
                    .bg(ratatui::style::Color::LightGreen)
            } else {
                ratatui::style::Style::default().fg(ratatui::style::Color::White)
            };

            let line = format!("{} - {}", cmd.name, cmd.description);
            let wrapped_lines: Vec<&str> = if line.is_empty() {
                vec![""]
            } else {
                line.as_bytes()
                    .chunks(text_width)
                    .map(|c| unsafe { std::str::from_utf8_unchecked(c) })
                    .collect()
            };

            for (j, line_chunk) in wrapped_lines.iter().enumerate() {
                if y >= inner_area.bottom() {
                    break;
                }
                let x = if j == 0 && is_selected {
                    inner_area.x
                } else {
                    inner_area.x + (cmd.name.len() as u16) + 3
                };
                buf.set_string(x, y, line_chunk, style);
                y += 1;
            }

            if y >= inner_area.bottom() {
                break;
            }
        }
    }
}
