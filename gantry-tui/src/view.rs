use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

use crate::agent_statusline::{AgentStatuslineWidget, AgentStatuslineWidgetState};
use crate::app_statusline::AppStatuslineWidget;
use crate::attachment_picker::AttachmentPickerWidget;
use crate::chat::ChatWidgetState;
use crate::chat::widget::ChatWidget;
use crate::command_picker::CommandPickerWidget;
use crate::input::InputWidget;
use crate::model::{InputOverlay, Mode, Model};
use crate::model_picker::ModelPickerWidget;
use crate::providers::ProvidersWidget;
use crate::sessions::SessionsWidget;
use crate::tree::TreeWidget;
use crate::usage::UsageWidget;

#[derive(Default)]
pub struct WidgetState {
    pub chat: ChatWidgetState,
    pub agent_statusline: AgentStatuslineWidgetState,
}

/// Renders the full application UI for the current frame.
pub fn render(frame: &mut Frame, model: &mut Model, view_state: &mut WidgetState) {
    let area = frame.area();

    let input_height = match &model.overlay {
        InputOverlay::Usage(uv) => UsageWidget::new(uv).height(),
        InputOverlay::CommandPicker(picker) => CommandPickerWidget::new(picker).height(),
        InputOverlay::ModelPicker(mv) => ModelPickerWidget::new(mv).height(),
        InputOverlay::Sessions(sv) => SessionsWidget::new(sv).height(),
        InputOverlay::Tree(tv) => TreeWidget::new(tv).height(),
        InputOverlay::Providers(pv) => ProvidersWidget::new(pv).height(),
        InputOverlay::AttachmentPicker(_) | InputOverlay::Input(_) => {
            InputWidget::new(&model.input, &model.cwd).height(area.width)
        }
    };

    let app_statusline_height = match &model.overlay {
        InputOverlay::AttachmentPicker(picker) => {
            AttachmentPickerWidget::new(picker, &model.cwd).height()
        }
        _ => 1,
    };

    let agent_statusline =
        AgentStatuslineWidget::new(&model.stream, model.status_message.as_deref());
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

    let chat = ChatWidget {
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
        InputOverlay::Usage(uv) => {
            frame.render_widget(UsageWidget::new(uv), input_area);
        }
        InputOverlay::CommandPicker(picker) => {
            frame.render_widget(CommandPickerWidget::new(picker), input_area);
        }
        InputOverlay::ModelPicker(mv) => {
            frame.render_widget(ModelPickerWidget::new(mv), input_area);
        }
        InputOverlay::Sessions(sv) => {
            frame.render_widget(SessionsWidget::new(sv), input_area);
        }
        InputOverlay::Tree(tv) => {
            frame.render_widget(TreeWidget::new(tv), input_area);
        }
        InputOverlay::Providers(pv) => {
            frame.render_widget(ProvidersWidget::new(pv), input_area);
        }
        InputOverlay::AttachmentPicker(_) | InputOverlay::Input(_) => {
            let picker_filter_len =
                if let InputOverlay::AttachmentPicker(ref picker) = model.overlay {
                    1 + picker.filter.len() // sigil + filter chars
                } else {
                    0
                };
            let mode = match &model.overlay {
                InputOverlay::Input(m) => *m,
                _ => Mode::Insert,
            };
            frame.render_widget(
                InputWidget::new(&model.input, &model.cwd)
                    .with_mode(mode)
                    .with_picker_filter_len(picker_filter_len),
                input_area,
            );
        }
    }

    match &model.overlay {
        InputOverlay::AttachmentPicker(picker) => {
            frame.render_widget(
                AttachmentPickerWidget::new(picker, &model.cwd),
                app_statusline_area,
            );
        }
        _ => {
            let mode = match &model.overlay {
                InputOverlay::Input(m) => *m,
                _ => Mode::Normal,
            };
            frame.render_widget(
                AppStatuslineWidget::new(mode, model.context_window.clone()),
                app_statusline_area,
            );
        }
    }
}
