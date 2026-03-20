use super::chat;
use super::input;
use gantry_types::Message;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

pub struct App {
    pub messages: Vec<Message>,
    pub input_buffer: String,
}

impl App {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            input_buffer: String::new(),
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = frame.size();
        let input_height = input::Input::calc_height(&self.input_buffer, area.width);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(input_height)])
            .split(area);

        let chat_area = chunks[0];
        let input_area = chunks[1];

        frame.render_widget(chat::Chat::new(&self.messages), chat_area);
        frame.render_widget(input::Input::new(&self.input_buffer), input_area);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
