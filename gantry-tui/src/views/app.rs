use crate::model::Model;
use crate::views::ViewState;
use crate::views::chat::ChatView;
use crate::views::command_picker::CommandPickerView;
use crate::views::input::InputView;
use crate::views::status_message::StatusMessageView;
use crate::views::statusline::StatuslineView;
use crate::views::tree::TreeViewWidget;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

pub fn render(frame: &mut Frame, model: &mut Model, view_state: &mut ViewState) {
    let area = frame.area();

    if let Some(ref tv) = model.tree_view {
        frame.render_widget(TreeViewWidget::new(tv), area);
        return;
    }

    let input_height = if let Some(ref picker) = model.command_picker {
        CommandPickerView::new(picker).calc_height(area.width)
    } else {
        InputView::new(&model.input.value, model.input.cursor).calc_height(area.width)
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(input_height),
            Constraint::Length(1),
        ])
        .split(area);

    let chat_area = chunks[0];
    let input_area = chunks[2];
    let statusline_area = chunks[3];

    let chat = ChatView {
        messages: &model.chat.messages,
        scroll_offset: model.chat.scroll_offset,
    };
    frame.render_stateful_widget(chat, chat_area, &mut view_state.chat);

    if let Some(ref picker) = model.command_picker {
        frame.render_widget(CommandPickerView::new(picker), input_area);
    } else {
        frame.render_widget(
            InputView::new(&model.input.value, model.input.cursor),
            input_area,
        );
    }

    if let Some(ref msg) = model.status_message {
        frame.render_widget(StatusMessageView::new(msg), statusline_area);
    } else {
        frame.render_stateful_widget(
            StatuslineView::new(model.is_streaming()),
            statusline_area,
            &mut view_state.statusline,
        );
    }
}
