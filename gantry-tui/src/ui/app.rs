use super::chat;
use super::command_picker;
use super::input;
use gantry_core::{Message, Role};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

#[derive(Clone)]
pub struct App {
    pub messages: Vec<Message>,
    pub input_buffer: String,
    pub streaming_content: Option<String>,
    pub streaming_message_idx: Option<usize>,
    pub streaming_buffer: String,
    pub show_form: bool,
    pub command_picker: Option<command_picker::CommandPicker>,
    pub status_message: Option<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            input_buffer: String::new(),
            streaming_content: None,
            streaming_message_idx: None,
            streaming_buffer: String::new(),
            show_form: false,
            command_picker: None,
            status_message: None,
        }
    }

    pub fn available_commands() -> Vec<command_picker::Command> {
        vec![command_picker::Command {
            name: "health".to_string(),
            description: "Check connection to server".to_string(),
        }]
    }

    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(Message::new(Role::User, content));
    }

    pub fn start_streaming_message(&mut self) {
        self.streaming_content = Some(String::new());
        self.streaming_message_idx = Some(self.messages.len());
        self.messages
            .push(Message::new(Role::Assistant, String::new()));
    }

    pub fn append_to_streaming(&mut self, content: &str) {
        if let Some(ref mut streaming) = self.streaming_content {
            self.streaming_buffer.push_str(content);

            while let Some(newline_idx) = self.streaming_buffer.find('\n') {
                let line = self
                    .streaming_buffer
                    .drain(..=newline_idx)
                    .collect::<String>();
                streaming.push_str(&line);
                if let Some(idx) = self.streaming_message_idx
                    && idx < self.messages.len()
                {
                    self.messages[idx].content.push_str(&line);
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn update_streaming_content(&mut self, content: String) {
        self.streaming_content = Some(content.clone());
        if let Some(idx) = self.streaming_message_idx
            && idx < self.messages.len()
        {
            self.messages[idx].content = content;
        }
    }

    pub fn finish_streaming(&mut self) {
        if !self.streaming_buffer.is_empty()
            && let Some(ref mut streaming) = self.streaming_content
        {
            streaming.push_str(&self.streaming_buffer);
            if let Some(idx) = self.streaming_message_idx
                && idx < self.messages.len()
            {
                self.messages[idx].content.push_str(&self.streaming_buffer);
            }
        }
        self.streaming_content = None;
        self.streaming_message_idx = None;
        self.streaming_buffer.clear();
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

    pub fn is_command_picker_active(&self) -> bool {
        self.input_buffer.starts_with('/') && !Self::available_commands().is_empty()
    }

    pub fn activate_command_picker(&mut self) {
        self.command_picker = Some(command_picker::CommandPicker::new(
            Self::available_commands(),
        ));
    }

    pub fn deactivate_command_picker(&mut self) {
        self.command_picker = None;
    }

    pub fn update_command_filter(&mut self, filter: &str) {
        if let Some(ref mut picker) = self.command_picker {
            picker.set_filter(filter);
        }
    }

    pub fn move_command_selection_up(&mut self) {
        if let Some(ref mut picker) = self.command_picker {
            picker.move_selection_up();
        }
    }

    pub fn move_command_selection_down(&mut self) {
        if let Some(ref mut picker) = self.command_picker {
            picker.move_selection_down();
        }
    }

    pub fn selected_command(&self) -> Option<&command_picker::Command> {
        self.command_picker
            .as_ref()
            .and_then(|p| p.selected_command())
    }

    pub fn set_status(&mut self, message: String) {
        self.status_message = Some(message);
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let input_height = if self.status_message.is_some() {
            3
        } else if self.is_command_picker_active() {
            self.command_picker
                .as_ref()
                .map(|p| p.calc_height(area.width))
                .unwrap_or(3)
        } else {
            input::Input::calc_height(&self.input_buffer, area.width)
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(input_height),
            ])
            .split(area);

        let chat_area = chunks[0];
        let input_area = chunks[2];

        frame.render_widget(
            chat::Chat::new(&self.messages, self.streaming_content.clone()),
            chat_area,
        );

        if let Some(ref status) = self.status_message {
            frame.render_widget(input::Input::new(status), input_area);
        } else if self.is_command_picker_active() {
            if let Some(ref picker) = self.command_picker {
                frame.render_widget(picker.clone(), input_area);
            }
        } else {
            frame.render_widget(input::Input::new(&self.input_buffer), input_area);
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
