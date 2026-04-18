use crossterm::event::{KeyCode, KeyModifiers};
use gantry_core::AppEvent;

use crate::message::Msg;
use crate::model::{CommandEntry, ConnectionState, Model};

pub fn update(model: &mut Model, msg: Msg) -> Option<Msg> {
    match msg {
        Msg::AppEvent(ev) => handle_app_event(model, ev),
        Msg::WsDisconnected => {
            model.connection_state = ConnectionState::Disconnected;
            model.status_message = Some("Disconnected \u{2014} reconnecting...".into());
            model.chat.finish_streaming();
            None
        }
        Msg::WsError(e) => {
            model.chat.add_error_message(e);
            None
        }
        Msg::StreamResult(Ok(())) => None,
        Msg::StreamResult(Err(e)) => {
            model.chat.finish_streaming();
            if is_connection_error(&e) {
                model.connection_state = ConnectionState::Disconnected;
                model.status_message = Some("Disconnected \u{2014} reconnecting...".into());
            } else {
                model.chat.add_error_message(e);
            }
            None
        }
        Msg::SetStatus(s) => {
            model.status_message = Some(s);
            None
        }
        Msg::ReconnectSuccess {
            session_id,
            clear_messages,
            ..
        } => {
            model.connection_state = ConnectionState::Connected;
            model.session_id = session_id;
            model.status_message = None;
            if clear_messages {
                model.chat.reset();
            }
            None
        }
        Msg::NewSession { session_id, .. } => {
            model.session_id = session_id;
            model.chat.reset();
            model.connection_state = ConnectionState::Connected;
            model.status_message = None;
            None
        }
        Msg::Key(key) => handle_key(model, key),
        Msg::Quit | Msg::SendMessage(_) | Msg::InterruptStream | Msg::ExecuteCommand(_) => None,
    }
}

fn handle_app_event(model: &mut Model, event: AppEvent) -> Option<Msg> {
    match event {
        AppEvent::Init(ev) => {
            model.chat.messages = ev.messages;
            if let Some(pending) = ev.pending_message {
                model.chat.add_user_message(pending.content.clone());
                model.chat.start_streaming_message();
                model.chat.pending_message_id = Some(pending.id);
            }
            if ev.form.is_some() {
                model.chat.show_form = true;
            }
        }
        AppEvent::MessageReceived(ev) => {
            model.chat.pending_message_id = Some(ev.id);
        }
        AppEvent::StreamStart(_) => {}
        AppEvent::Token(ev) => {
            model.chat.append_to_streaming(&ev.delta);
        }
        AppEvent::StreamEnd(_) => {
            model.chat.finish_streaming();
        }
        AppEvent::PendingCleared(_) => {
            model.chat.pending_message_id = None;
        }
        AppEvent::FormShown(_) => {
            model.chat.show_form = true;
        }
        AppEvent::FormHidden(_) => {
            model.chat.show_form = false;
        }
        AppEvent::Error(ev) => {
            model.chat.add_error_message(ev.message);
        }
    }
    None
}

fn handle_key(model: &mut Model, key: crossterm::event::KeyEvent) -> Option<Msg> {
    if let KeyCode::Char('c') = key.code
        && key.modifiers.contains(KeyModifiers::CONTROL)
    {
        return Some(Msg::Quit);
    }

    match key.code {
        KeyCode::Char('q') if !model.is_command_picker_active() && model.input.value.is_empty() => {
            Some(Msg::Quit)
        }
        KeyCode::Esc => handle_esc(model),
        KeyCode::Enter => handle_enter(model, key.modifiers),
        KeyCode::Char(c) => {
            handle_char(model, c);
            None
        }
        KeyCode::Backspace => {
            handle_backspace(model);
            None
        }
        KeyCode::Up => {
            if model.is_command_picker_active() {
                model.move_command_selection_up();
                update_input_from_selection(model);
            }
            None
        }
        KeyCode::Down => {
            if model.is_command_picker_active() {
                model.move_command_selection_down();
                update_input_from_selection(model);
            }
            None
        }
        _ => None,
    }
}

fn handle_esc(model: &mut Model) -> Option<Msg> {
    if model.status_message.is_some() {
        model.status_message = None;
        None
    } else if model.is_command_picker_active() {
        model.input.value.clear();
        model.deactivate_command_picker();
        None
    } else if model.chat.pending_message_id.is_some() {
        Some(Msg::InterruptStream)
    } else {
        None
    }
}

fn handle_enter(model: &mut Model, modifiers: KeyModifiers) -> Option<Msg> {
    if model.status_message.is_some() {
        model.status_message = None;
        return None;
    }

    if model.is_command_picker_active() {
        let selected = model.selected_command();
        model.input.value.clear();
        model.deactivate_command_picker();
        return selected.map(|cmd| Msg::ExecuteCommand(cmd.command));
    }

    if modifiers.contains(KeyModifiers::SHIFT) {
        model.input.value.push('\n');
        return None;
    }

    let input = model.input.value.clone();
    if input.trim().is_empty() || model.is_streaming() {
        return None;
    }

    if input.starts_with('/') {
        let filter = input.strip_prefix('/').unwrap_or("");
        let available = available_command_entries();
        let has_match = available.iter().any(|c| c.name.starts_with(filter));
        if !has_match {
            model.input.value.clear();
            return None;
        }
    }

    if !model.is_connected() {
        model.status_message = Some("Not connected to server \u{2014} reconnecting...".to_string());
        return None;
    }

    model.input.value.clear();
    model.chat.add_user_message(input.clone());
    model.chat.start_streaming_message();
    Some(Msg::SendMessage(input))
}

fn handle_char(model: &mut Model, c: char) {
    if model.status_message.is_some() {
        model.status_message = None;
    }
    if c == '/' && !available_command_entries().is_empty() && model.input.value.is_empty() {
        model.input.value.push(c);
        model.activate_command_picker(available_command_entries());
    } else if model.is_command_picker_active() {
        model.input.value.push(c);
        let filter = input_filter(&model.input.value);
        model.update_command_filter(&filter);
    } else {
        model.input.value.push(c);
    }
}

fn handle_backspace(model: &mut Model) {
    if model.status_message.is_some() {
        model.status_message = None;
        return;
    }
    if model.is_command_picker_active() {
        model.input.value.pop();
        let filter = input_filter(&model.input.value);
        if filter.is_empty() && !model.input.value.starts_with('/') {
            model.deactivate_command_picker();
        } else {
            model.update_command_filter(&filter);
        }
    } else {
        model.input.value.pop();
    }
}

fn update_input_from_selection(model: &mut Model) {
    if let Some(cmd) = model.selected_command() {
        model.input.value = format!("/{}", cmd.name);
    }
}

fn input_filter(input: &str) -> String {
    if input.len() > 1 {
        input[1..].to_string()
    } else {
        String::new()
    }
}

pub fn available_command_entries() -> Vec<CommandEntry> {
    crate::commands::all_commands()
        .into_iter()
        .map(|c| {
            let c: std::sync::Arc<dyn crate::commands::Command> = c.into();
            CommandEntry {
                name: c.name().to_string(),
                description: c.description().to_string(),
                command: c,
            }
        })
        .collect()
}

pub fn is_connection_error(err: &str) -> bool {
    err.contains("connection refused")
        || err.contains("failed to connect")
        || err.contains("broken pipe")
        || err.contains("WebSocket")
        || err.contains("os error")
}
