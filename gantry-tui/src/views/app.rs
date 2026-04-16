use super::chat::ChatView;
use super::command_picker::{Command, CommandPickerView};
use super::input::InputView;
use gantry_core::{Message, Role};

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::Widget,
};

pub struct AppView {
    pub chat: ChatView,
    pub input: InputView,
    pub command_picker: Option<CommandPickerView>,
    pub status_message: Option<String>,
    pub connected: bool,
    pub show_form: bool,
}

impl AppView {
    pub fn new() -> Self {
        Self {
            chat: ChatView::new(),
            input: InputView::new(),
            command_picker: None,
            status_message: None,
            connected: false,
            show_form: false,
        }
    }

    pub fn available_commands() -> Vec<Command> {
        crate::commands::all_commands()
            .iter()
            .map(|c| Command {
                name: c.name().to_string(),
                description: c.description().to_string(),
            })
            .collect()
    }

    pub fn reset_for_new_session(&mut self) {
        self.chat.reset();
        self.show_form = false;
    }

    pub fn is_streaming(&self) -> bool {
        self.chat.is_streaming()
    }

    pub fn show_form(&mut self) {
        self.show_form = true;
    }

    pub fn hide_form(&mut self) {
        self.show_form = false;
    }

    pub fn is_command_picker_active(&self) -> bool {
        self.input.value().starts_with('/') && !Self::available_commands().is_empty()
    }

    pub fn activate_command_picker(&mut self) {
        self.command_picker = Some(CommandPickerView::new(Self::available_commands()));
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

    pub fn selected_command(&self) -> Option<&Command> {
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
        frame.render_widget(self, frame.area());
    }

    // Convenience methods delegating to sub-views
    pub fn add_error_message(&mut self, message: String) {
        self.chat.messages.push(Message::new(Role::Error, message));
    }
}

impl Default for AppView {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for &AppView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let input_height = if self.status_message.is_some() {
            3
        } else if self.is_command_picker_active() {
            self.command_picker
                .as_ref()
                .map(|p| p.calc_height(area.width))
                .unwrap_or(3)
        } else {
            self.input.calc_height(area.width)
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

        self.chat.render(chat_area, buf);

        if let Some(ref status) = self.status_message {
            // Render status as a read-only input-style widget
            let status_view = InputView::new();
            let mut status_view = status_view;
            status_view.set(status.clone());
            status_view.render(input_area, buf);
        } else if self.is_command_picker_active() {
            if let Some(ref picker) = self.command_picker {
                picker.render(input_area, buf);
            }
        } else {
            self.input.render(input_area, buf);
        }
    }
}
