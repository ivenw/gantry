use super::chat;
use super::input;
use gantry_contract::Message;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

#[derive(Clone)]
pub struct App {
    pub messages: Vec<Message>,
    pub input_buffer: String,
    pub streaming_content: Option<String>,
    pub streaming_message_idx: Option<usize>,
    pub show_form: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            input_buffer: String::new(),
            streaming_content: None,
            streaming_message_idx: None,
            show_form: false,
        }
    }

    pub fn add_user_message(&mut self, content: String) {
            self.messages
            .push(Message::new(gantry_contract::Role::User, content));
    }

    pub fn start_streaming_message(&mut self) {
        self.streaming_content = Some(String::new());
        self.streaming_message_idx = Some(self.messages.len());
        self.messages
            .push(Message::new(gantry_contract::Role::Assistant, String::new()));
    }

    pub fn append_to_streaming(&mut self, content: &str) {
        if let Some(ref mut streaming) = self.streaming_content {
            streaming.push_str(content);
            if let Some(idx) = self.streaming_message_idx {
                if idx < self.messages.len() {
                    self.messages[idx].content.push_str(content);
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn update_streaming_content(&mut self, content: String) {
        self.streaming_content = Some(content.clone());
        if let Some(idx) = self.streaming_message_idx {
            if idx < self.messages.len() {
                self.messages[idx].content = content;
            }
        }
    }

    pub fn finish_streaming(&mut self) {
        self.streaming_content = None;
        self.streaming_message_idx = None;
    }

    #[allow(dead_code)]
    pub fn is_streaming(&self) -> bool {
        self.streaming_content.is_some()
    }

    pub fn show_form(&mut self) {
        self.show_form = true;
    }

    pub fn hide_form(&mut self) {
        self.show_form = false;
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

        frame.render_widget(
            chat::Chat::new(&self.messages, self.streaming_content.clone()),
            chat_area,
        );
        frame.render_widget(input::Input::new(&self.input_buffer), input_area);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
