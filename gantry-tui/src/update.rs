use crossterm::event::{KeyCode, KeyModifiers};
use gantry_core::{ChatStreamItem, MultiTurnStreamItem, StreamedAssistantContent, StreamingError};

use crate::message::Msg;
use crate::model::{
    CommandEntry, CopilotAuthKind, InputMode, Model, ProviderWizard, ProvidersSubView,
    WizardProviderKind, branch_rows, prev_char_boundary,
};
use crate::views::ViewState;
use gantry_core::SessionId;

pub fn update(model: &mut Model, view_state: &ViewState, msg: Msg) -> Option<Msg> {
    match msg {
        Msg::StreamItem(item) => handle_stream_item(model, item),
        Msg::StreamDone => {
            model.chat.finish_streaming();
            if !model.chat.user_is_scrolling {
                model.chat.scroll_offset = 0;
            }
            None
        }
        Msg::ToolCallStarted { name, id } => {
            model.chat.push_tool_call(id, name);
            None
        }
        Msg::ToolCallFinished { id } => {
            model.chat.finish_tool_call(&id);
            None
        }
        Msg::StreamError(e) => {
            if let Some(text) = model.chat.cancel_streaming() {
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
            model.activate_sessions_view(sessions, active_id);
            None
        }
        // ResumeSession is handled in Runtime before update() is called.
        Msg::ResumeSession(_) => None,
        Msg::OpenTreeView(nodes) => {
            model.activate_tree_view(nodes);
            None
        }
        Msg::ReloadMessages(messages) => {
            model.chat.messages = messages;
            model.chat.scroll_offset = 0;
            model.chat.user_is_scrolling = false;
            model.deactivate_tree_view();
            None
        }
        Msg::ReloadMessagesWithInput(messages, input) => {
            model.chat.messages = messages;
            model.chat.scroll_offset = 0;
            model.chat.user_is_scrolling = false;
            model.input.set_text(input);
            model.deactivate_tree_view();
            None
        }
        Msg::ContextWindowUpdated(cw) => {
            model.context_window = Some(cw);
            None
        }
        Msg::OpenProvidersView(providers) => {
            model.activate_providers_view(providers);
            None
        }
        // AddProvider and RemoveProvider are handled in Runtime before update() is called.
        Msg::AddProvider(_, _) | Msg::RemoveProvider(_) => None,
        Msg::OpenModelPicker(selections) => {
            model.activate_model_picker_view(selections);
            None
        }
        // SelectModel is handled in Runtime before update() is called.
        Msg::SelectModel(_) => None,
        Msg::OpenUsageView(cw, history) => {
            model.activate_usage_view(cw, history);
            None
        }
        Msg::SetPathPickerResults(results) => {
            if let Some(ref mut picker) = model.attachment_picker {
                picker.kind = crate::model::AttachmentPickerKind::Path(results);
                picker.selected_idx = 0;
            }
            None
        }
        Msg::SetSkillPickerResults(results) => {
            if let Some(ref mut picker) = model.attachment_picker {
                picker.kind = crate::model::AttachmentPickerKind::Skill(results);
                picker.selected_idx = 0;
            }
            None
        }
        Msg::Quit
        | Msg::SendMessage(_)
        | Msg::InterruptStream
        | Msg::ExecuteCommand(_)
        | Msg::BranchTo(_)
        | Msg::BranchToWithInput { .. }
        | Msg::OpenPathPicker(_)
        | Msg::OpenSkillPicker(_)
        | Msg::RefineAttachmentPicker(_) => None,
    }
}

fn handle_stream_item(
    model: &mut Model,
    item: Result<ChatStreamItem, StreamingError>,
) -> Option<Msg> {
    match item {
        Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(text))) => {
            model.chat.append_to_streaming(&text.text);
            if !model.chat.user_is_scrolling {
                model.chat.scroll_offset = 0;
            }
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
    // Overlay states are handled before normal/insert mode.
    if model.is_model_picker_active() {
        return handle_key_model_picker(model, key);
    }

    if model.is_providers_view_active() {
        return handle_key_providers_view(model, key);
    }

    if model.is_sessions_view_active() {
        return handle_key_sessions_view(model, key);
    }

    if model.is_tree_view_active() {
        return handle_key_tree_view(model, key);
    }

    if model.is_usage_view_active() {
        return handle_key_usage_view(model, key);
    }

    if model.is_command_picker_active() {
        return handle_key_command_picker(model, key);
    }

    if model.is_attachment_picker_active() {
        return handle_key_attachment_picker(model, key);
    }

    match model.mode {
        InputMode::Normal => handle_key_normal(model, view_state, key),
        InputMode::Insert => handle_key_insert(model, view_state, key),
    }
}

fn handle_key_usage_view(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    if key.code == KeyCode::Esc {
        model.deactivate_usage_view();
    }
    None
}

fn handle_key_model_picker(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            model.deactivate_model_picker_view();
            None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            model.move_model_picker_selection_up();
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            model.move_model_picker_selection_down();
            None
        }
        KeyCode::Enter => {
            let msg = model
                .selected_model_in_picker()
                .cloned()
                .map(Msg::SelectModel);
            model.deactivate_model_picker_view();
            msg
        }
        _ => None,
    }
}

fn handle_key_providers_view(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    let sub_kind = model.providers_view.as_ref().map(|pv| match pv.sub {
        ProvidersSubView::List { .. } => 0u8,
        ProvidersSubView::TypePicker { .. } => 1,
        ProvidersSubView::CopilotAuthPicker { .. } => 2,
        ProvidersSubView::Wizard(_) => 3,
    })?;

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
            model.deactivate_providers_view();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let pv = model.providers_view.as_mut()?;
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
            let pv = model.providers_view.as_mut()?;
            if let ProvidersSubView::List {
                ref mut selected_idx,
            } = pv.sub
                && !pv.providers.is_empty()
            {
                *selected_idx = (*selected_idx + 1) % pv.providers.len();
            }
        }
        KeyCode::Char('a') => {
            let pv = model.providers_view.as_mut()?;
            pv.sub = ProvidersSubView::TypePicker { selected_idx: 0 };
        }
        KeyCode::Char('d') => {
            let pv = model.providers_view.as_ref()?;
            if let ProvidersSubView::List { selected_idx } = pv.sub
                && selected_idx < pv.providers.len()
            {
                let alias = pv.providers[selected_idx].alias().clone();
                return Some(Msg::RemoveProvider(alias));
            }
        }
        _ => {}
    }
    None
}

fn handle_key_providers_type_picker(
    model: &mut Model,
    key: crossterm::event::KeyEvent,
) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            let pv = model.providers_view.as_mut()?;
            pv.sub = ProvidersSubView::List { selected_idx: 0 };
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let pv = model.providers_view.as_mut()?;
            if let ProvidersSubView::TypePicker {
                ref mut selected_idx,
            } = pv.sub
            {
                let count = WizardProviderKind::ALL.len();
                *selected_idx = selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let pv = model.providers_view.as_mut()?;
            if let ProvidersSubView::TypePicker {
                ref mut selected_idx,
            } = pv.sub
            {
                *selected_idx = (*selected_idx + 1) % WizardProviderKind::ALL.len();
            }
        }
        KeyCode::Enter => {
            let pv = model.providers_view.as_mut()?;
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
    match key.code {
        KeyCode::Esc => {
            let pv = model.providers_view.as_mut()?;
            pv.sub = ProvidersSubView::TypePicker { selected_idx: 0 };
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let pv = model.providers_view.as_mut()?;
            if let ProvidersSubView::CopilotAuthPicker {
                ref mut selected_idx,
            } = pv.sub
            {
                let count = CopilotAuthKind::ALL.len();
                *selected_idx = selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let pv = model.providers_view.as_mut()?;
            if let ProvidersSubView::CopilotAuthPicker {
                ref mut selected_idx,
            } = pv.sub
            {
                *selected_idx = (*selected_idx + 1) % CopilotAuthKind::ALL.len();
            }
        }
        KeyCode::Enter => {
            let pv = model.providers_view.as_mut()?;
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
    match key.code {
        KeyCode::Esc => {
            let pv = model.providers_view.as_mut()?;
            let is_copilot = matches!(&pv.sub, ProvidersSubView::Wizard(w) if w.kind == WizardProviderKind::Copilot);
            if is_copilot {
                pv.sub = ProvidersSubView::CopilotAuthPicker { selected_idx: 0 };
            } else {
                pv.sub = ProvidersSubView::TypePicker { selected_idx: 0 };
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let pv = model.providers_view.as_mut()?;
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
            let pv = model.providers_view.as_mut()?;
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
            let pv = model.providers_view.as_mut()?;
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
                } else {
                    // Advance to the next row, skipping optional empty fields.
                    if w.focused_idx + 1 < w.row_count() {
                        w.focused_idx += 1;
                        w.cursor = w
                            .fields
                            .get(w.focused_idx)
                            .map(|f| f.value.len())
                            .unwrap_or(0);
                    }
                }
            }
        }
        KeyCode::Char(c) => {
            let pv = model.providers_view.as_mut()?;
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
            let pv = model.providers_view.as_mut()?;
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
            let pv = model.providers_view.as_mut()?;
            if let ProvidersSubView::Wizard(ref mut w) = pv.sub
                && !w.is_on_confirm()
            {
                w.cursor = prev_char_boundary(&w.fields[w.focused_idx].value, w.cursor);
            }
        }
        KeyCode::Right => {
            let pv = model.providers_view.as_mut()?;
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
            model.deactivate_sessions_view();
            None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            model.move_sessions_selection_up();
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            model.move_sessions_selection_down();
            None
        }
        KeyCode::Enter => {
            let session_id: Option<SessionId> = model.selected_session().map(|s| s.id.clone());
            model.deactivate_sessions_view();
            session_id.map(Msg::ResumeSession)
        }
        _ => None,
    }
}

fn handle_key_tree_view(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            model.deactivate_tree_view();
            None
        }
        KeyCode::Enter => handle_enter_tree_view(model),
        KeyCode::Up | KeyCode::Char('k') => {
            model.move_tree_selection_up();
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            model.move_tree_selection_down();
            None
        }
        _ => None,
    }
}

fn handle_key_command_picker(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            model.deactivate_command_picker();
            None
        }
        KeyCode::Enter => {
            let selected = model.selected_command();
            model.deactivate_command_picker();
            selected.map(|cmd| Msg::ExecuteCommand(cmd.command))
        }
        KeyCode::Up => {
            model.move_command_selection_up();
            None
        }
        KeyCode::Down => {
            model.move_command_selection_down();
            None
        }
        KeyCode::Char(c) => {
            model.command_picker_filter_push(c);
            None
        }
        KeyCode::Backspace => {
            model.command_picker_filter_pop();
            None
        }
        _ => None,
    }
}

fn handle_key_attachment_picker(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            model.deactivate_attachment_picker();
            None
        }
        KeyCode::Backspace => {
            // Pop one filter char; close the picker if the filter is already empty.
            let had_chars = model.attachment_picker_filter_pop();
            if !had_chars {
                model.deactivate_attachment_picker();
                return None;
            }
            let query = model.attachment_picker_filter().unwrap_or("").to_string();
            Some(Msg::RefineAttachmentPicker(query))
        }
        KeyCode::Enter => {
            let token = model.selected_attachment();
            let filter_len = model
                .attachment_picker_filter()
                .map(|f| f.len())
                .unwrap_or(0);
            model.deactivate_attachment_picker();
            if let Some(token) = token {
                // +1 for the sigil character that was inserted into the input on activation.
                model
                    .input
                    .replace_filter_with_attachment(1 + filter_len, token);
            }
            None
        }
        KeyCode::Up => {
            model.move_attachment_selection_up();
            None
        }
        KeyCode::Down => {
            model.move_attachment_selection_down();
            None
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
                // Remove the sigil from the input and close the picker cleanly.
                model.input.delete_before_cursor();
                model.deactivate_attachment_picker();
            } else {
                model.attachment_picker_filter_clear();
                return Some(Msg::RefineAttachmentPicker(String::new()));
            }
            None
        }
        KeyCode::Char(c) => {
            model.attachment_picker_filter_push(c);
            let query = model.attachment_picker_filter().unwrap_or("").to_string();
            Some(Msg::RefineAttachmentPicker(query))
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
            model.mode = InputMode::Insert;
            None
        }
        KeyCode::Char(' ') => {
            model.activate_command_picker(available_command_entries());
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
            model.mode = InputMode::Normal;
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
    let node = model.selected_tree_node()?;
    let msg = if matches!(node.node.message, gantry_core::Message::User { .. }) {
        let input = node.node.message.text();
        let tv = model.tree_view.as_ref()?;
        let rows = branch_rows(&tv.tree.stem, 0);
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

    let display = model.input.raw_display();
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
    model.chat.start_streaming_message();
    model.chat.scroll_offset = 0;
    model.chat.user_is_scrolling = false;
    Some(Msg::SendMessage(tokens))
}

pub fn available_command_entries() -> Vec<CommandEntry> {
    crate::commands::KnownCommand::ALL
        .iter()
        .map(|k| CommandEntry {
            name: k.name().to_string(),
            description: k.description().to_string(),
            command: k.into_command().into(),
            indices: vec![],
        })
        .collect()
}
