use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

use crate::chat::ChatViewState;
use crate::chat::view::ChatView;
use crate::command_picker::CommandPickerView;
use crate::input::{AttachmentPickerView, InputView};
use crate::model::{InputOverlay, Mode, Model};
use crate::model_picker::ModelPickerWidget;
use crate::providers::ProvidersViewWidget;
use crate::sessions::SessionsViewWidget;
use crate::statusline::{AgentStatusline, AgentStatuslineState, AppStatusline};
use crate::tree::TreeViewWidget;
use crate::usage::UsageViewWidget;

#[derive(Default)]
pub struct ViewState {
    pub chat: ChatViewState,
    pub agent_statusline: AgentStatuslineState,
}

/// Renders the full application UI for the current frame.
pub fn render(frame: &mut Frame, model: &mut Model, view_state: &mut ViewState) {
    let area = frame.area();

    let input_height = match &model.overlay {
        InputOverlay::UsageView(uv) => UsageViewWidget::new(uv).height(),
        InputOverlay::CommandPicker(picker) => CommandPickerView::new(picker).height(),
        InputOverlay::ModelPicker(mv) => ModelPickerWidget::new(mv).height(),
        InputOverlay::SessionsView(sv) => SessionsViewWidget::new(sv).height(),
        InputOverlay::TreeView(tv) => TreeViewWidget::new(tv).height(),
        InputOverlay::Providers(pv) => ProvidersViewWidget::new(pv).height(),
        InputOverlay::AttachmentPicker(_) | InputOverlay::Chat(_) => {
            InputView::new(&model.input, &model.cwd).height(area.width)
        }
    };

    let app_statusline_height = match &model.overlay {
        InputOverlay::AttachmentPicker(picker) => {
            AttachmentPickerView::new(picker, &model.cwd).height()
        }
        _ => 1,
    };

    let agent_statusline = AgentStatusline::new(&model.stream, model.status_message.as_deref());
    let agent_statusline_height = agent_statusline.height();

    let agent_statusline_bottom_pad = if agent_statusline_height > 0 { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(agent_statusline_height),
            Constraint::Length(agent_statusline_bottom_pad),
            Constraint::Length(input_height),
            Constraint::Length(app_statusline_height),
        ])
        .split(area);

    let chat_area = chunks[0];
    let agent_statusline_area = chunks[2];
    let input_area = chunks[4];
    let app_statusline_area = chunks[5];

    let chat = ChatView {
        messages: &model.chat.messages,
        scroll_offset: model.chat.scroll_offset,
        spinner: view_state.agent_statusline.spinner(),
    };
    frame.render_stateful_widget(chat, chat_area, &mut view_state.chat);

    frame.render_stateful_widget(
        agent_statusline,
        agent_statusline_area,
        &mut view_state.agent_statusline,
    );

    match &model.overlay {
        InputOverlay::UsageView(uv) => {
            frame.render_widget(UsageViewWidget::new(uv), input_area);
        }
        InputOverlay::CommandPicker(picker) => {
            frame.render_widget(CommandPickerView::new(picker), input_area);
        }
        InputOverlay::ModelPicker(mv) => {
            frame.render_widget(ModelPickerWidget::new(mv), input_area);
        }
        InputOverlay::SessionsView(sv) => {
            frame.render_widget(SessionsViewWidget::new(sv), input_area);
        }
        InputOverlay::TreeView(tv) => {
            frame.render_widget(TreeViewWidget::new(tv), input_area);
        }
        InputOverlay::Providers(pv) => {
            frame.render_widget(ProvidersViewWidget::new(pv), input_area);
        }
        InputOverlay::AttachmentPicker(_) | InputOverlay::Chat(_) => {
            let picker_filter_len =
                if let InputOverlay::AttachmentPicker(ref picker) = model.overlay {
                    1 + picker.filter.len() // sigil + filter chars
                } else {
                    0
                };
            let mode = match &model.overlay {
                InputOverlay::Chat(m) => *m,
                _ => Mode::Insert,
            };
            frame.render_widget(
                InputView::new(&model.input, &model.cwd)
                    .with_mode(mode)
                    .with_picker_filter_len(picker_filter_len),
                input_area,
            );
        }
    }

    match &model.overlay {
        InputOverlay::AttachmentPicker(picker) => {
            frame.render_widget(
                AttachmentPickerView::new(picker, &model.cwd),
                app_statusline_area,
            );
        }
        _ => {
            let mode = match &model.overlay {
                InputOverlay::Chat(m) => *m,
                _ => Mode::Normal,
            };
            frame.render_widget(
                AppStatusline::new(mode, model.context_window.clone()),
                app_statusline_area,
            );
        }
    }
}
