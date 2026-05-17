use crossterm::event::{KeyCode, KeyModifiers};
use gantry_core::{
    AppEvent, ChatStreamItem, MultiTurnStreamItem, ReasoningContent, StreamedAssistantContent,
    StreamedUserContent, ToolResultContent,
};

use super::{InputOverlay, Mode, Model};
use crate::features::command_picker::CommandPickerState;
use crate::features::input::prev_char_boundary;
use crate::features::provider_config::{
    CopilotAuthKind, ProviderWizard, ProvidersSubView, WizardProviderKind,
};
use crate::features::tree::branch_rows;
use crate::message::{Cmd, Msg};
use crate::view::WidgetState;
use gantry_core::SessionId;

/// Applies a `Msg` to the model, returning an optional `Cmd` to be executed by `Runtime`.
///
/// This function is pure: it only reads and mutates `Model`. All side effects are carried
/// out by `Runtime` after inspecting the returned `Cmd`.
pub fn update(model: &mut Model, view_state: &WidgetState, msg: Msg) -> Option<Cmd> {
    match msg {
        Msg::KeyEvent(key) => handle_key(model, view_state, key),
        Msg::ScrollChat(delta) => {
            model.chat.scroll_by(delta, view_state.chat.max_scroll);
            None
        }
        Msg::StartStream => {
            model.start_stream();
            None
        }
        Msg::StreamInterrupted => {
            model.interrupt_stream();
            None
        }
        Msg::AppEvent(AppEvent::EditDiff { path, hunks }) => {
            model.chat.attach_edit_diff(&path, hunks);
            None
        }
        Msg::AppEvent(AppEvent::MetricsUpdated {
            context_window,
            total_consumption,
        }) => {
            model.update_metrics(crate::model::SessionStats {
                context_window,
                usage: total_consumption,
            });
            None
        }
        Msg::StreamItem(item) => handle_stream_item(model, item),
        Msg::StreamDone => {
            model.complete_stream();
            None
        }
        Msg::StreamError(e) => {
            model.fail_stream(e);
            None
        }
        Msg::SetStatus(s) => {
            model.status_message = Some(s);
            None
        }
        Msg::SessionCreated => {
            model.reset_session();
            None
        }
        Msg::OpenSessionsPicker(sessions, active_id) => {
            model.open_sessions_picker(sessions, active_id);
            None
        }
        Msg::OpenSessionTree(nodes) => {
            model.open_session_tree(nodes);
            None
        }
        Msg::SessionLoaded {
            session_id,
            messages,
            session_stats,
        } => {
            model.load_session(session_id, messages, session_stats);
            None
        }
        Msg::ReloadMessages(messages) => {
            model.reload_messages(messages);
            None
        }
        Msg::ReloadMessagesWithInput(messages, input) => {
            model.reload_messages_with_input(messages, input);
            None
        }
        Msg::OpenProviderConfig(providers) => {
            model.open_provider_config(providers);
            None
        }
        Msg::OpenModelPicker(selections) => {
            model.open_model_picker(selections);
            None
        }
        Msg::ModelsFetched(models) => {
            model.cached_models = Some(models.clone());
            model.open_model_picker(models);
            None
        }
        Msg::OpenUsageState(cw) => {
            model.overlay = InputOverlay::Usage(cw);
            None
        }
        Msg::SetPathPickerResults(results) => {
            if let InputOverlay::AttachmentPicker(ref mut picker) = model.overlay {
                picker.set_path_results(results);
            }
            None
        }
        Msg::SetSkillPickerResults(results) => {
            if let InputOverlay::AttachmentPicker(ref mut picker) = model.overlay {
                picker.set_skill_results(results);
            }
            None
        }
        Msg::ActivatePathPicker(results) => {
            model.activate_path_picker(results);
            None
        }
        Msg::ActivateSkillPicker(results) => {
            model.activate_skill_picker(results);
            None
        }
        Msg::ProviderAdded(providers) => {
            model.open_provider_config(providers);
            None
        }
        Msg::ProviderRemoved(providers) => {
            model.cached_models = None;
            if let InputOverlay::ProviderConfig(ref mut pv) = model.overlay
                && let ProvidersSubView::List {
                    ref mut selected_idx,
                } = pv.sub
            {
                pv.providers = providers;
                if !pv.providers.is_empty() {
                    *selected_idx = (*selected_idx).min(pv.providers.len() - 1);
                } else {
                    *selected_idx = 0;
                }
            }
            None
        }
        Msg::ProviderAddFailed(error) => {
            if let InputOverlay::ProviderConfig(ref mut pv) = model.overlay
                && let ProvidersSubView::Wizard(ref mut w) = pv.sub
            {
                w.error = Some(error);
            } else {
                model.status_message = Some(error);
            }
            None
        }
        Msg::ModelSelected(selection) => {
            model.selection = Some(selection);
            None
        }
    }
}

fn handle_stream_item(model: &mut Model, item: ChatStreamItem) -> Option<Cmd> {
    match item {
        MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Reasoning(r)) => {
            let text: String = r
                .content
                .iter()
                .filter_map(|c| {
                    if let ReasoningContent::Text { text, .. } = c {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if !text.is_empty() {
                model.chat.append_to_reasoning(&text);
                model.chat.scroll_to_bottom();
            }
        }
        MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(text)) => {
            model.chat.append_to_streaming(&text.text);
            model.chat.scroll_to_bottom();
        }
        MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::ToolCall {
            tool_call,
            internal_call_id,
        }) => {
            model.chat.push_tool_call(
                internal_call_id,
                tool_call.function.name,
                tool_call.function.arguments,
            );
        }
        // A tool result closes the pending tool call and opens a fresh streaming slot so the
        // next assistant text turn renders as a separate message.
        MultiTurnStreamItem::StreamUserItem(StreamedUserContent::ToolResult {
            internal_call_id,
            tool_result,
        }) => {
            let result_text = tool_result.content.iter().find_map(|c| {
                if let ToolResultContent::Text(t) = c {
                    Some(t.text.as_str())
                } else {
                    None
                }
            });
            let is_error = result_text
                .map(|t| t.starts_with(gantry_core::tools::TOOL_ERROR_PREFIX))
                .unwrap_or(false);
            model.chat.finish_tool_call(&internal_call_id, is_error);
            model.chat.start_streaming_message();
        }
        _ => {}
    }
    None
}

fn handle_key(
    model: &mut Model,
    view_state: &WidgetState,
    key: crossterm::event::KeyEvent,
) -> Option<Cmd> {
    match &model.overlay {
        InputOverlay::ModelPicker(_) => handle_key_model_picker(model, key),
        InputOverlay::ProviderConfig(_) => handle_key_providers_view(model, key),
        InputOverlay::SessionPicker(_) => handle_key_sessions_view(model, key),
        InputOverlay::Tree(_) => handle_key_tree_view(model, key),
        InputOverlay::Usage(_) => handle_key_usage_view(model, key),
        InputOverlay::CommandPicker(_) => handle_key_command_picker(model, key),
        InputOverlay::AttachmentPicker(_) => handle_key_attachment_picker(model, key),
        InputOverlay::Input(Mode::Normal) => handle_key_normal(model, view_state, key),
        InputOverlay::Input(Mode::Insert) => handle_key_insert(model, view_state, key),
    }
}

fn handle_key_usage_view(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Cmd> {
    if key.code == KeyCode::Esc {
        model.overlay = InputOverlay::Input(Mode::Normal);
    }
    None
}

fn handle_key_model_picker(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Cmd> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Input(Mode::Normal);
            return None;
        }
        KeyCode::Enter => {
            let msg = model.selected_model_in_picker().map(Cmd::SelectModel);
            model.overlay = InputOverlay::Input(Mode::Normal);
            return msg;
        }
        _ => {}
    }
    let InputOverlay::ModelPicker(ref mut mv) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Up => mv.picker.move_up(),
        KeyCode::Down => mv.picker.move_down(),
        KeyCode::Backspace => {
            let mut f = mv.picker.filter().to_owned();
            f.pop();
            mv.picker.set_filter(&f);
        }
        KeyCode::Char(c) => {
            let mut f = mv.picker.filter().to_owned();
            f.push(c);
            mv.picker.set_filter(&f);
        }
        _ => {}
    }
    None
}

fn handle_key_providers_view(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Cmd> {
    let InputOverlay::ProviderConfig(ref pv) = model.overlay else {
        return None;
    };
    match pv.sub {
        ProvidersSubView::List { .. } => handle_key_providers_list(model, key),
        ProvidersSubView::TypePicker { .. } => handle_key_providers_type_picker(model, key),
        ProvidersSubView::CopilotAuthPicker { .. } => handle_key_copilot_auth_picker(model, key),
        ProvidersSubView::Wizard(_) => handle_key_wizard(model, key),
    }
}

fn handle_key_providers_list(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Cmd> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Input(Mode::Normal);
            return None;
        }
        KeyCode::Char('d') => {
            return selected_provider_alias(&model.overlay).map(Cmd::RemoveProvider);
        }
        _ => {}
    }
    let InputOverlay::ProviderConfig(ref mut pv) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if let ProvidersSubView::List {
                ref mut selected_idx,
            } = pv.sub
                && !pv.providers.is_empty()
            {
                *selected_idx = selected_idx
                    .checked_sub(1)
                    .unwrap_or(pv.providers.len() - 1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let ProvidersSubView::List {
                ref mut selected_idx,
            } = pv.sub
                && !pv.providers.is_empty()
            {
                *selected_idx = (*selected_idx + 1) % pv.providers.len();
            }
        }
        KeyCode::Char('a') => {
            pv.sub = ProvidersSubView::TypePicker { selected_idx: 0 };
        }
        _ => {}
    }
    None
}

fn selected_provider_alias(overlay: &InputOverlay) -> Option<gantry_core::ProviderAlias> {
    let InputOverlay::ProviderConfig(pv) = overlay else {
        return None;
    };
    let ProvidersSubView::List { selected_idx } = pv.sub else {
        return None;
    };
    pv.providers.get(selected_idx).map(|p| p.alias().clone())
}

fn handle_key_providers_type_picker(
    model: &mut Model,
    key: crossterm::event::KeyEvent,
) -> Option<Cmd> {
    let InputOverlay::ProviderConfig(ref mut pv) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Esc => {
            pv.sub = ProvidersSubView::List { selected_idx: 0 };
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let ProvidersSubView::TypePicker {
                ref mut selected_idx,
            } = pv.sub
            {
                let count = WizardProviderKind::ALL.len();
                *selected_idx = selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let ProvidersSubView::TypePicker {
                ref mut selected_idx,
            } = pv.sub
            {
                *selected_idx = (*selected_idx + 1) % WizardProviderKind::ALL.len();
            }
        }
        KeyCode::Enter => {
            if let ProvidersSubView::TypePicker { selected_idx } = pv.sub {
                let kind = WizardProviderKind::ALL[selected_idx];
                if kind == WizardProviderKind::Copilot {
                    pv.sub = ProvidersSubView::CopilotAuthPicker { selected_idx: 0 };
                } else {
                    pv.sub = ProvidersSubView::Wizard(ProviderWizard::new(kind, None));
                }
            }
        }
        _ => {}
    }
    None
}

fn handle_key_copilot_auth_picker(
    model: &mut Model,
    key: crossterm::event::KeyEvent,
) -> Option<Cmd> {
    let InputOverlay::ProviderConfig(ref mut pv) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Esc => {
            pv.sub = ProvidersSubView::TypePicker { selected_idx: 0 };
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let ProvidersSubView::CopilotAuthPicker {
                ref mut selected_idx,
            } = pv.sub
            {
                let count = CopilotAuthKind::ALL.len();
                *selected_idx = selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let ProvidersSubView::CopilotAuthPicker {
                ref mut selected_idx,
            } = pv.sub
            {
                *selected_idx = (*selected_idx + 1) % CopilotAuthKind::ALL.len();
            }
        }
        KeyCode::Enter => {
            if let ProvidersSubView::CopilotAuthPicker { selected_idx } = pv.sub {
                let auth = CopilotAuthKind::ALL[selected_idx];
                pv.sub = ProvidersSubView::Wizard(ProviderWizard::new(
                    WizardProviderKind::Copilot,
                    Some(auth),
                ));
            }
        }
        _ => {}
    }
    None
}

fn handle_key_wizard(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Cmd> {
    let InputOverlay::ProviderConfig(ref mut pv) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Esc => {
            let is_copilot = matches!(&pv.sub, ProvidersSubView::Wizard(w) if w.kind == WizardProviderKind::Copilot);
            if is_copilot {
                pv.sub = ProvidersSubView::CopilotAuthPicker { selected_idx: 0 };
            } else {
                pv.sub = ProvidersSubView::TypePicker { selected_idx: 0 };
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let ProvidersSubView::Wizard(ref mut w) = pv.sub
                && w.focused_idx > 0
            {
                w.focused_idx -= 1;
                w.cursor = w
                    .fields
                    .get(w.focused_idx)
                    .map(|f| f.value.len())
                    .unwrap_or(0);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let ProvidersSubView::Wizard(ref mut w) = pv.sub
                && w.focused_idx + 1 < w.row_count()
            {
                w.focused_idx += 1;
                w.cursor = w
                    .fields
                    .get(w.focused_idx)
                    .map(|f| f.value.len())
                    .unwrap_or(0);
            }
        }
        KeyCode::Enter => {
            if let ProvidersSubView::Wizard(ref mut w) = pv.sub {
                if w.is_on_confirm() {
                    match w.build() {
                        Ok((config, credential)) => {
                            return Some(Cmd::AddProvider(config, credential));
                        }
                        Err(msg) => {
                            w.error = Some(msg);
                        }
                    }
                } else if w.focused_idx + 1 < w.row_count() {
                    w.focused_idx += 1;
                    w.cursor = w
                        .fields
                        .get(w.focused_idx)
                        .map(|f| f.value.len())
                        .unwrap_or(0);
                }
            }
        }
        KeyCode::Char(c) => {
            if let ProvidersSubView::Wizard(ref mut w) = pv.sub
                && !w.is_on_confirm()
            {
                let field = &mut w.fields[w.focused_idx];
                field.value.insert(w.cursor, c);
                w.cursor += c.len_utf8();
                w.error = None;
            }
        }
        KeyCode::Backspace => {
            if let ProvidersSubView::Wizard(ref mut w) = pv.sub
                && !w.is_on_confirm()
                && w.cursor > 0
            {
                let field = &mut w.fields[w.focused_idx];
                let prev = prev_char_boundary(&field.value, w.cursor);
                field.value.drain(prev..w.cursor);
                w.cursor = prev;
                w.error = None;
            }
        }
        KeyCode::Left => {
            if let ProvidersSubView::Wizard(ref mut w) = pv.sub
                && !w.is_on_confirm()
            {
                w.cursor = prev_char_boundary(&w.fields[w.focused_idx].value, w.cursor);
            }
        }
        KeyCode::Right => {
            if let ProvidersSubView::Wizard(ref mut w) = pv.sub
                && !w.is_on_confirm()
            {
                let val = &w.fields[w.focused_idx].value;
                if w.cursor < val.len() {
                    let c = val[w.cursor..].chars().next().unwrap();
                    w.cursor += c.len_utf8();
                }
            }
        }
        _ => {}
    }
    None
}

fn handle_key_sessions_view(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Cmd> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Input(Mode::Normal);
            return None;
        }
        KeyCode::Enter => {
            let session_id: Option<SessionId> = model.selected_session().map(|s| s.id.clone());
            model.overlay = InputOverlay::Input(Mode::Normal);
            return session_id.map(Cmd::ResumeSession);
        }
        _ => {}
    }
    let InputOverlay::SessionPicker(ref mut sv) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => sv.picker.move_up(),
        KeyCode::Down | KeyCode::Char('j') => sv.picker.move_down(),
        KeyCode::Backspace => {
            let mut f = sv.picker.filter().to_owned();
            f.pop();
            sv.picker.set_filter(&f);
        }
        KeyCode::Char(c) => {
            let mut f = sv.picker.filter().to_owned();
            f.push(c);
            sv.picker.set_filter(&f);
        }
        _ => {}
    }
    None
}

fn handle_key_tree_view(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Cmd> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Input(Mode::Normal);
            return None;
        }
        KeyCode::Enter => return handle_enter_tree_view(model),
        _ => {}
    }
    let InputOverlay::Tree(ref mut tv) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            tv.selected_idx = tv.selected_idx.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let count = branch_rows(&tv.tree.stem, 0).len();
            if count > 0 {
                tv.selected_idx = (tv.selected_idx + 1).min(count - 1);
            }
        }
        _ => {}
    }
    None
}

fn handle_key_command_picker(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Cmd> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Input(Mode::Normal);
            return None;
        }
        KeyCode::Enter => {
            let selected = if let InputOverlay::CommandPicker(ref p) = model.overlay {
                p.picker.selected().cloned()
            } else {
                None
            };
            model.overlay = InputOverlay::Input(Mode::Normal);
            return selected.map(Cmd::RunCommand);
        }
        _ => {}
    }
    let InputOverlay::CommandPicker(ref mut picker) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Up => picker.picker.move_up(),
        KeyCode::Down => picker.picker.move_down(),
        KeyCode::Char(c) => {
            let mut f = picker.picker.filter().to_owned();
            f.push(c);
            picker.picker.set_filter(&f);
        }
        KeyCode::Backspace => {
            let mut f = picker.picker.filter().to_owned();
            f.pop();
            picker.picker.set_filter(&f);
        }
        _ => {}
    }
    None
}

fn handle_key_attachment_picker(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Cmd> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Input(Mode::Insert);
            return None;
        }
        KeyCode::Backspace => {
            let had_chars = model.attachment_picker_filter_pop();
            if !had_chars {
                model.overlay = InputOverlay::Input(Mode::Insert);
                return None;
            }
            let query = model.attachment_picker_filter().unwrap_or("").to_string();
            return Some(Cmd::RefineAttachmentPicker(query));
        }
        KeyCode::Enter => {
            let token = model.selected_attachment();
            let filter_len = model
                .attachment_picker_filter()
                .map(|f| f.len())
                .unwrap_or(0);
            model.overlay = InputOverlay::Input(Mode::Insert);
            if let Some(token) = token {
                // +1 for the sigil character that was inserted into the input on activation.
                model
                    .input
                    .replace_filter_with_attachment(1 + filter_len, token);
            }
            return None;
        }
        KeyCode::Char('c')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            let is_empty = model
                .attachment_picker_filter()
                .map(|f| f.is_empty())
                .unwrap_or(true);
            if is_empty {
                model.input.delete_before_cursor();
                model.overlay = InputOverlay::Input(Mode::Insert);
                return None;
            } else {
                model.attachment_picker_filter_clear();
                return Some(Cmd::RefineAttachmentPicker(String::new()));
            }
        }
        KeyCode::Char(c) => {
            model.attachment_picker_filter_push(c);
            let query = model.attachment_picker_filter().unwrap_or("").to_string();
            return Some(Cmd::RefineAttachmentPicker(query));
        }
        // Navigation keys fall through to the picker below.
        _ => {}
    }
    let InputOverlay::AttachmentPicker(ref mut picker) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Up => {
            let count = picker.len();
            if count > 0 {
                picker.selected_idx = picker.selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
            None
        }
        KeyCode::Down => {
            let count = picker.len();
            if count > 0 {
                picker.selected_idx = (picker.selected_idx + 1) % count;
            }
            None
        }
        _ => None,
    }
}

fn handle_key_normal(
    model: &mut Model,
    view_state: &WidgetState,
    key: crossterm::event::KeyEvent,
) -> Option<Cmd> {
    match key.code {
        KeyCode::Char('i') => {
            model.overlay = InputOverlay::Input(Mode::Insert);
            None
        }
        KeyCode::Char(' ') => {
            model.overlay = InputOverlay::CommandPicker(CommandPickerState::new());
            None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            model.chat.scroll_by(-1, view_state.chat.max_scroll);
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            model.chat.scroll_by(1, view_state.chat.max_scroll);
            None
        }
        KeyCode::PageDown => {
            model.chat.scroll_by(-10, view_state.chat.max_scroll);
            None
        }
        KeyCode::PageUp => {
            model.chat.scroll_by(10, view_state.chat.max_scroll);
            None
        }
        _ => None,
    }
}

fn handle_key_insert(
    model: &mut Model,
    view_state: &WidgetState,
    key: crossterm::event::KeyEvent,
) -> Option<Cmd> {
    if let KeyCode::Char('c') = key.code
        && key.modifiers.contains(KeyModifiers::CONTROL)
    {
        model.input.clear();
        return None;
    }

    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Input(Mode::Normal);
            if model.is_streaming() {
                return Some(Cmd::StopStream);
            }
            None
        }
        KeyCode::Enter => handle_enter_insert(model, key.modifiers),
        KeyCode::Char(c) => {
            if model.status_message.is_some() {
                model.status_message = None;
            }
            if c == '+' {
                return Some(Cmd::OpenPathPicker(String::new()));
            }
            if c == '/' {
                return Some(Cmd::OpenSkillPicker(String::new()));
            }
            model.input.insert(c);
            None
        }
        KeyCode::Backspace => {
            if model.status_message.is_some() {
                model.status_message = None;
            } else {
                model.input.delete_before_cursor();
            }
            None
        }
        KeyCode::Left => {
            model.input.move_left();
            None
        }
        KeyCode::Right => {
            model.input.move_right();
            None
        }
        KeyCode::Up => {
            model.chat.scroll_by(1, view_state.chat.max_scroll);
            None
        }
        KeyCode::Down => {
            model.chat.scroll_by(-1, view_state.chat.max_scroll);
            None
        }
        KeyCode::PageUp => {
            model.chat.scroll_by(10, view_state.chat.max_scroll);
            None
        }
        KeyCode::PageDown => {
            model.chat.scroll_by(-10, view_state.chat.max_scroll);
            None
        }
        _ => None,
    }
}

fn handle_enter_tree_view(model: &mut Model) -> Option<Cmd> {
    let InputOverlay::Tree(ref tv) = model.overlay else {
        return None;
    };
    let rows = branch_rows(&tv.tree.stem, 0);
    let (node, _) = rows.get(tv.selected_idx)?;
    let msg = if matches!(node.message, gantry_core::Message::User { .. }) {
        let input = node.message.text();
        let preceding = rows[..tv.selected_idx]
            .iter()
            .rfind(|(n, _)| !matches!(n.message, gantry_core::Message::User { .. }))
            .map(|(n, _)| n.id.to_string())?;
        Cmd::BranchToWithInput {
            branch_id: preceding,
            input,
        }
    } else {
        Cmd::BranchTo(node.id.to_string())
    };
    Some(msg)
}

fn handle_enter_insert(model: &mut Model, modifiers: KeyModifiers) -> Option<Cmd> {
    if modifiers.contains(KeyModifiers::SHIFT) {
        model.input.insert('\n');
        return None;
    }
    model.submit_message().map(Cmd::SendMessage)
}
