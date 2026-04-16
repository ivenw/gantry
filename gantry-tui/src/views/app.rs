use crate::model::Model;
use crate::views::chat::ChatViewState;
use crate::views::command_picker::CommandPickerView;
use crate::views::input::InputView;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

pub fn render(frame: &mut Frame, model: &Model) {
    let area = frame.area();

    let input_height = if model.status_message.is_some() {
        3
    } else if let Some(ref picker) = model.command_picker {
        CommandPickerView::new(picker).calc_height(area.width)
    } else {
        InputView::new(&model.input.value).calc_height(area.width)
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

    let chat = ChatViewState {
        messages: &model.chat.messages,
        streaming_content: model.chat.streaming_content.as_deref(),
    };
    frame.render_widget(chat, chat_area);

    if let Some(ref status) = model.status_message {
        frame.render_widget(InputView::new(status), input_area);
    } else if let Some(ref picker) = model.command_picker {
        frame.render_widget(CommandPickerView::new(picker), input_area);
    } else {
        frame.render_widget(InputView::new(&model.input.value), input_area);
    }
}
