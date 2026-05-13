use crossterm::event::{KeyCode, KeyModifiers};
use gantry_core::{
    AppEvent, ChatStreamItem, MultiTurnStreamItem, ReasoningContent, StreamedAssistantContent,
    StreamedUserContent, StreamingError, ToolResultContent,
};

use crate::command_picker::CommandEntry;
use crate::commands::KnownCommand;
use crate::input::prev_char_boundary;
use crate::message::Msg;
use crate::model::{InputOverlay, Mode, Model, StreamState};
use crate::providers::{CopilotAuthKind, ProviderWizard, ProvidersSubView, WizardProviderKind};
use crate::tree::branch_rows;
use crate::view::ViewState;
use gantry_core::SessionId;

pub fn update(model: &mut Model, view_state: &ViewState, msg: Msg) -> Option<Msg> {
    match msg {
        Msg::StreamItem(item) => handle_stream_item(model, item),
        Msg::StreamDone => {
            if !matches!(model.stream, StreamState::Interrupted { .. }) {
                model.finish_stream();
                if !model.chat.user_is_scrolling {
                    model.chat.scroll_offset = 0;
                }
            }
            None
        }
        Msg::StreamError(e) => {
            if let Some(text) = model.cancel_stream() {
                model.input.set_text(text);
            }
            model.status_message = Some(e);
            None
        }
        Msg::SetStatus(s) => {
            model.status_message = Some(s);
            None
        }
        Msg::NewSession => {
            model.chat.reset();
            model.status_message = None;
            model.reset_stream();
            None
        }
        Msg::Key(key) => handle_key(model, view_state, key),
        Msg::ScrollChat(delta) => {
            let max = view_state.chat.max_scroll;
            let offset = model.chat.scroll_offset as i32 + delta;
            model.chat.scroll_offset = offset.clamp(0, max as i32) as u16;
            model.chat.user_is_scrolling = model.chat.scroll_offset > 0;
            None
        }
        Msg::OpenSessionsView(sessions, active_id) => {
            model.open_sessions_view(sessions, active_id);
            None
        }
        // ResumeSession is handled in Runtime before update() is called.
        Msg::ResumeSession(_) => None,
        Msg::OpenTreeView(nodes) => {
            model.open_tree_view(nodes);
            None
        }
        Msg::ReloadMessages(messages) => {
            model.chat.messages = messages;
            model.chat.scroll_offset = 0;
            model.chat.user_is_scrolling = false;
            model.overlay = InputOverlay::Chat(Mode::Normal);
            None
        }
        Msg::ReloadMessagesWithInput(messages, input) => {
            model.chat.messages = messages;
            model.chat.scroll_offset = 0;
            model.chat.user_is_scrolling = false;
            model.input.set_text(input);
            model.overlay = InputOverlay::Chat(Mode::Normal);
            None
        }
        Msg::ContextWindowUpdated(cw) => {
            model.context_window = Some(cw);
            None
        }
        Msg::OpenProvidersView(providers) => {
            use crate::providers::{ProvidersSubView, ProvidersView};
            model.overlay = InputOverlay::Providers(ProvidersView {
                providers,
                sub: ProvidersSubView::List { selected_idx: 0 },
            });
            None
        }
        // AddProvider and RemoveProvider are handled in Runtime before update() is called.
        Msg::AddProvider(_, _) | Msg::RemoveProvider(_) => None,
        Msg::OpenModelPicker(selections) => {
            model.open_model_picker(selections);
            None
        }
        // SelectModel is handled in Runtime before update() is called.
        Msg::SelectModel(_) => None,
        Msg::OpenUsageView(cw, consumption) => {
            use crate::usage::UsageView;
            model.overlay = InputOverlay::UsageView(UsageView {
                context_window: cw,
                consumption,
            });
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
        Msg::Quit
        | Msg::SendMessage(_)
        | Msg::InterruptStream
        | Msg::RunCommand(_)
        | Msg::BranchTo(_)
        | Msg::BranchToWithInput { .. }
        | Msg::OpenPathPicker(_)
        | Msg::OpenSkillPicker(_)
        | Msg::RefineAttachmentPicker(_) => None,
        Msg::AppEvent(AppEvent::EditDiff { path, hunks }) => {
            model.chat.attach_edit_diff(&path, hunks);
            None
        }
    }
}

fn handle_stream_item(
    model: &mut Model,
    item: Result<ChatStreamItem, StreamingError>,
) -> Option<Msg> {
    match item {
        Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Reasoning(r))) => {
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
                if !model.chat.user_is_scrolling {
                    model.chat.scroll_offset = 0;
                }
            }
        }
        Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(text))) => {
            model.chat.append_to_streaming(&text.text);
            if !model.chat.user_is_scrolling {
                model.chat.scroll_offset = 0;
            }
        }
        Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::ToolCall {
            tool_call,
            internal_call_id,
        })) => {
            model.chat.push_tool_call(
                internal_call_id,
                tool_call.function.name,
                tool_call.function.arguments,
            );
        }
        // A tool result closes the pending tool call and opens a fresh streaming slot so the
        // next assistant text turn renders as a separate message.
        Ok(MultiTurnStreamItem::StreamUserItem(StreamedUserContent::ToolResult {
            internal_call_id,
            tool_result,
        })) => {
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
        Ok(_) => {}
        Err(e) => {
            model.status_message = Some(e.to_string());
        }
    }
    None
}

fn handle_key(
    model: &mut Model,
    view_state: &ViewState,
    key: crossterm::event::KeyEvent,
) -> Option<Msg> {
    match &model.overlay {
        InputOverlay::ModelPicker(_) => handle_key_model_picker(model, key),
        InputOverlay::Providers(_) => handle_key_providers_view(model, key),
        InputOverlay::SessionsView(_) => handle_key_sessions_view(model, key),
        InputOverlay::TreeView(_) => handle_key_tree_view(model, key),
        InputOverlay::UsageView(_) => handle_key_usage_view(model, key),
        InputOverlay::CommandPicker(_) => handle_key_command_picker(model, key),
        InputOverlay::AttachmentPicker(_) => handle_key_attachment_picker(model, key),
        InputOverlay::Chat(Mode::Normal) => handle_key_normal(model, view_state, key),
        InputOverlay::Chat(Mode::Insert) => handle_key_insert(model, view_state, key),
    }
}

fn handle_key_usage_view(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    if key.code == KeyCode::Esc {
        model.overlay = InputOverlay::Chat(Mode::Normal);
    }
    None
}

fn handle_key_model_picker(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Chat(Mode::Normal);
            return None;
        }
        KeyCode::Enter => {
            let msg = model.selected_model_in_picker().map(Msg::SelectModel);
            model.overlay = InputOverlay::Chat(Mode::Normal);
            return msg;
        }
        _ => {}
    }
    let InputOverlay::ModelPicker(ref mut mv) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Up => {
            let count = mv.filtered.len();
            if count > 0 {
                mv.selected_idx = mv.selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
        KeyCode::Down => {
            let count = mv.filtered.len();
            if count > 0 {
                mv.selected_idx = (mv.selected_idx + 1) % count;
            }
        }
        KeyCode::Backspace => {
            mv.filter.pop();
            mv.selected_idx = 0;
            mv.refilter();
        }
        KeyCode::Char(c) => {
            mv.filter.push(c);
            mv.selected_idx = 0;
            mv.refilter();
        }
        _ => {}
    }
    None
}

fn handle_key_providers_view(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    let sub_kind = if let InputOverlay::Providers(ref pv) = model.overlay {
        match pv.sub {
            ProvidersSubView::List { .. } => 0u8,
            ProvidersSubView::TypePicker { .. } => 1,
            ProvidersSubView::CopilotAuthPicker { .. } => 2,
            ProvidersSubView::Wizard(_) => 3,
        }
    } else {
        return None;
    };

    match sub_kind {
        0 => handle_key_providers_list(model, key),
        1 => handle_key_providers_type_picker(model, key),
        2 => handle_key_copilot_auth_picker(model, key),
        _ => handle_key_wizard(model, key),
    }
}

fn handle_key_providers_list(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Chat(Mode::Normal);
            return None;
        }
        KeyCode::Char('d') => {
            let InputOverlay::Providers(ref pv) = model.overlay else {
                return None;
            };
            if let ProvidersSubView::List { selected_idx } = pv.sub
                && selected_idx < pv.providers.len()
            {
                let alias = pv.providers[selected_idx].alias().clone();
                return Some(Msg::RemoveProvider(alias));
            }
            return None;
        }
        _ => {}
    }
    let InputOverlay::Providers(ref mut pv) = model.overlay else {
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

fn handle_key_providers_type_picker(
    model: &mut Model,
    key: crossterm::event::KeyEvent,
) -> Option<Msg> {
    let InputOverlay::Providers(ref mut pv) = model.overlay else {
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
) -> Option<Msg> {
    let InputOverlay::Providers(ref mut pv) = model.overlay else {
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

fn handle_key_wizard(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    let InputOverlay::Providers(ref mut pv) = model.overlay else {
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
                            return Some(Msg::AddProvider(config, credential));
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

fn handle_key_sessions_view(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Chat(Mode::Normal);
            return None;
        }
        KeyCode::Enter => {
            let session_id: Option<SessionId> = model.selected_session().map(|s| s.id.clone());
            model.overlay = InputOverlay::Chat(Mode::Normal);
            return session_id.map(Msg::ResumeSession);
        }
        _ => {}
    }
    let InputOverlay::SessionsView(ref mut sv) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if !sv.sessions.is_empty() {
                sv.selected_idx = sv
                    .selected_idx
                    .checked_sub(1)
                    .unwrap_or(sv.sessions.len() - 1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !sv.sessions.is_empty() {
                sv.selected_idx = (sv.selected_idx + 1) % sv.sessions.len();
            }
        }
        _ => {}
    }
    None
}

fn handle_key_tree_view(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Chat(Mode::Normal);
            return None;
        }
        KeyCode::Enter => return handle_enter_tree_view(model),
        _ => {}
    }
    let InputOverlay::TreeView(ref mut tv) = model.overlay else {
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

fn handle_key_command_picker(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Chat(Mode::Normal);
            return None;
        }
        KeyCode::Enter => {
            let selected = if let InputOverlay::CommandPicker(ref p) = model.overlay {
                p.filtered.get(p.selected_idx).cloned()
            } else {
                None
            };
            model.overlay = InputOverlay::Chat(Mode::Normal);
            return selected.map(|cmd| Msg::RunCommand(cmd.command));
        }
        _ => {}
    }
    let InputOverlay::CommandPicker(ref mut picker) = model.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Up => {
            let count = picker.filtered.len();
            if count > 0 {
                picker.selected_idx = picker.selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
        KeyCode::Down => {
            let count = picker.filtered.len();
            if count > 0 {
                picker.selected_idx = (picker.selected_idx + 1) % count;
            }
        }
        KeyCode::Char(c) => {
            picker.filter.push(c);
            picker.selected_idx = 0;
            picker.refilter();
        }
        KeyCode::Backspace => {
            picker.filter.pop();
            picker.selected_idx = 0;
            picker.refilter();
        }
        _ => {}
    }
    None
}

fn handle_key_attachment_picker(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Chat(Mode::Insert);
            return None;
        }
        KeyCode::Backspace => {
            let had_chars = model.attachment_picker_filter_pop();
            if !had_chars {
                model.overlay = InputOverlay::Chat(Mode::Insert);
                return None;
            }
            let query = model.attachment_picker_filter().unwrap_or("").to_string();
            return Some(Msg::RefineAttachmentPicker(query));
        }
        KeyCode::Enter => {
            let token = model.selected_attachment();
            let filter_len = model
                .attachment_picker_filter()
                .map(|f| f.len())
                .unwrap_or(0);
            model.overlay = InputOverlay::Chat(Mode::Insert);
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
                model.overlay = InputOverlay::Chat(Mode::Insert);
            } else {
                model.attachment_picker_filter_clear();
                return Some(Msg::RefineAttachmentPicker(String::new()));
            }
            return None;
        }
        KeyCode::Char(c) => {
            model.attachment_picker_filter_push(c);
            let query = model.attachment_picker_filter().unwrap_or("").to_string();
            return Some(Msg::RefineAttachmentPicker(query));
        }
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
    view_state: &ViewState,
    key: crossterm::event::KeyEvent,
) -> Option<Msg> {
    match key.code {
        KeyCode::Char('i') => {
            model.overlay = InputOverlay::Chat(Mode::Insert);
            None
        }
        KeyCode::Char(' ') => {
            let commands = available_command_entries();
            let cmd_col_width = commands
                .iter()
                .map(|c| c.name.chars().count() as u16)
                .max()
                .unwrap_or(0);
            let mut picker = crate::command_picker::CommandPicker {
                commands,
                filter: String::new(),
                selected_idx: 0,
                filtered: Vec::new(),
                cmd_col_width,
            };
            picker.refilter();
            model.overlay = InputOverlay::CommandPicker(picker);
            None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            model.chat.scroll_offset = model.chat.scroll_offset.saturating_sub(1);
            model.chat.user_is_scrolling = model.chat.scroll_offset > 0;
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let max = view_state.chat.max_scroll;
            model.chat.scroll_offset = model.chat.scroll_offset.saturating_add(1).min(max);
            model.chat.user_is_scrolling = model.chat.scroll_offset > 0;
            None
        }
        KeyCode::PageDown => {
            model.chat.scroll_offset = model.chat.scroll_offset.saturating_sub(10);
            model.chat.user_is_scrolling = model.chat.scroll_offset > 0;
            None
        }
        KeyCode::PageUp => {
            let max = view_state.chat.max_scroll;
            model.chat.scroll_offset = model.chat.scroll_offset.saturating_add(10).min(max);
            model.chat.user_is_scrolling = model.chat.scroll_offset > 0;
            None
        }
        _ => None,
    }
}

fn handle_key_insert(
    model: &mut Model,
    view_state: &ViewState,
    key: crossterm::event::KeyEvent,
) -> Option<Msg> {
    if let KeyCode::Char('c') = key.code
        && key.modifiers.contains(KeyModifiers::CONTROL)
    {
        model.input.clear();
        return None;
    }

    match key.code {
        KeyCode::Esc => {
            model.overlay = InputOverlay::Chat(Mode::Normal);
            if model.is_streaming() {
                return Some(Msg::InterruptStream);
            }
            None
        }
        KeyCode::Enter => handle_enter_insert(model, key.modifiers),
        KeyCode::Char(c) => {
            if model.status_message.is_some() {
                model.status_message = None;
            }
            if c == '+' {
                return Some(Msg::OpenPathPicker(String::new()));
            }
            if c == '/' {
                return Some(Msg::OpenSkillPicker(String::new()));
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
            let max = view_state.chat.max_scroll;
            model.chat.scroll_offset = model.chat.scroll_offset.saturating_add(1).min(max);
            model.chat.user_is_scrolling = model.chat.scroll_offset > 0;
            None
        }
        KeyCode::Down => {
            model.chat.scroll_offset = model.chat.scroll_offset.saturating_sub(1);
            model.chat.user_is_scrolling = model.chat.scroll_offset > 0;
            None
        }
        KeyCode::PageUp => {
            let max = view_state.chat.max_scroll;
            model.chat.scroll_offset = model.chat.scroll_offset.saturating_add(10).min(max);
            model.chat.user_is_scrolling = model.chat.scroll_offset > 0;
            None
        }
        KeyCode::PageDown => {
            model.chat.scroll_offset = model.chat.scroll_offset.saturating_sub(10);
            model.chat.user_is_scrolling = model.chat.scroll_offset > 0;
            None
        }
        _ => None,
    }
}

fn handle_enter_tree_view(model: &mut Model) -> Option<Msg> {
    let InputOverlay::TreeView(ref tv) = model.overlay else {
        return None;
    };
    let rows = branch_rows(&tv.tree.stem, 0);
    let (node, _) = rows.get(tv.selected_idx)?;
    let msg = if matches!(node.node.message, gantry_core::Message::User { .. }) {
        let input = node.node.message.text();
        let preceding = rows[..tv.selected_idx]
            .iter()
            .rfind(|(n, _)| !matches!(n.node.message, gantry_core::Message::User { .. }))
            .map(|(n, _)| n.node.id.to_string())?;
        Msg::BranchToWithInput {
            branch_id: preceding,
            input,
        }
    } else {
        Msg::BranchTo(node.node.id.to_string())
    };
    Some(msg)
}

fn handle_enter_insert(model: &mut Model, modifiers: KeyModifiers) -> Option<Msg> {
    if model.status_message.is_some() {
        model.status_message = None;
        return None;
    }

    if modifiers.contains(KeyModifiers::SHIFT) {
        model.input.insert('\n');
        return None;
    }

    if model.input.is_blank() || model.is_streaming() {
        return None;
    }

    if model.selection.is_none() {
        model.status_message = Some("No model selected".to_string());
        return None;
    }

    let display = model.input.raw_display(&model.project_path);
    if display.starts_with('/') {
        let filter = display.strip_prefix('/').unwrap_or("");
        let available = available_command_entries();
        let has_match = available.iter().any(|c| c.name.starts_with(filter));
        if !has_match {
            model.input.clear();
            return None;
        }
    }

    let tokens = model.input.tokens.clone();
    model.input.clear();
    model.chat.add_user_message(display);
    model.start_stream();
    model.chat.scroll_offset = 0;
    model.chat.user_is_scrolling = false;
    Some(Msg::SendMessage(tokens))
}

/// Builds the full list of command entries for the command picker.
pub fn available_command_entries() -> Vec<CommandEntry> {
    KnownCommand::ALL
        .iter()
        .map(|k| CommandEntry {
            name: k.name().to_string(),
            description: k.description().to_string(),
            command: *k,
            indices: vec![],
        })
        .collect()
}
