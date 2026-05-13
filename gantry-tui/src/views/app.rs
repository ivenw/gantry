use crate::chat::view::ChatView;
use crate::command_picker::CommandPickerView;
use crate::input::{AttachmentPickerView, InputView};
use crate::model::Model;
use crate::model_picker::ModelPickerWidget;
use crate::providers::ProvidersViewWidget;
use crate::sessions::SessionsViewWidget;
use crate::tree::TreeViewWidget;
use crate::views::ViewState;
use crate::views::status_message::StatusMessageView;
use crate::views::statusline::StatuslineView;
use crate::usage::UsageViewWidget;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

pub fn render(frame: &mut Frame, model: &mut Model, view_state: &mut ViewState) {
    let area = frame.area();

    if let Some(ref sv) = model.sessions_view {
        frame.render_widget(SessionsViewWidget::new(sv), area);
        return;
    }

    if let Some(ref tv) = model.tree_view {
        frame.render_widget(TreeViewWidget::new(tv), area);
        return;
    }

    if let Some(ref pv) = model.providers_view {
        frame.render_widget(ProvidersViewWidget::new(pv), area);
        return;
    }

    let input_height = if let Some(ref uv) = model.usage_view {
        UsageViewWidget::new(uv).height()
    } else if let Some(ref picker) = model.command_picker {
        CommandPickerView::new(picker).height()
    } else if let Some(ref mv) = model.model_picker_view {
        ModelPickerWidget::new(mv).height()
    } else {
        InputView::new(&model.input, &model.cwd).height(area.width)
    };

    let statusline_height = if let Some(ref picker) = model.attachment_picker {
        AttachmentPickerView::new(picker, &model.cwd).height()
    } else {
        1
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(input_height),
            Constraint::Length(statusline_height),
        ])
        .split(area);

    let chat_area = chunks[0];
    let input_area = chunks[2];
    let statusline_area = chunks[3];

    let chat = ChatView {
        messages: &model.chat.messages,
        scroll_offset: model.chat.scroll_offset,
        spinner: view_state.statusline.spinner(),
    };
    frame.render_stateful_widget(chat, chat_area, &mut view_state.chat);

    if let Some(ref uv) = model.usage_view {
        frame.render_widget(UsageViewWidget::new(uv), input_area);
    } else if let Some(ref picker) = model.command_picker {
        frame.render_widget(CommandPickerView::new(picker), input_area);
    } else if let Some(ref mv) = model.model_picker_view {
        frame.render_widget(ModelPickerWidget::new(mv), input_area);
    } else {
        // Input is always visible; compute picker_filter_len for highlight when picker is active.
        let picker_filter_len = model
            .attachment_picker
            .as_ref()
            .map(|p| 1 + p.filter.len()) // sigil + filter chars
            .unwrap_or(0);
        frame.render_widget(
            InputView::new(&model.input, &model.cwd)
                .with_mode(model.mode)
                .with_picker_filter_len(picker_filter_len),
            input_area,
        );
    }

    if let Some(ref picker) = model.attachment_picker {
        frame.render_widget(
            AttachmentPickerView::new(picker, &model.cwd),
            statusline_area,
        );
    } else if let Some(ref msg) = model.status_message {
        frame.render_widget(StatusMessageView::new(msg), statusline_area);
    } else {
        frame.render_stateful_widget(
            StatuslineView::new(
                model.mode,
                model.is_streaming(),
                model.context_window.clone(),
            ),
            statusline_area,
            &mut view_state.statusline,
        );
    }
}
