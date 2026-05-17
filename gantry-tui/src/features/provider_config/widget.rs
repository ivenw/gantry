use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

use super::{
    CopilotAuthKind, ProviderWizard, ProvidersConfigState, ProvidersSubView, WizardProviderKind,
};

pub struct ProviderConfigWidget<'a> {
    state: &'a ProvidersConfigState,
}

impl<'a> ProviderConfigWidget<'a> {
    /// Creates a new widget bound to the given providers view state.
    pub fn new(state: &'a ProvidersConfigState) -> Self {
        Self { state }
    }

    /// Returns the height required to render the current sub-view, capped at 10 content rows.
    ///
    /// Layout: 2 borders + content rows (capped) + 1 footer.
    pub fn height(&self) -> u16 {
        let content_rows = match &self.state.sub {
            ProvidersSubView::List { .. } => (self.state.providers.len() as u16).clamp(1, 10),
            ProvidersSubView::TypePicker { .. } => WizardProviderKind::ALL.len() as u16,
            ProvidersSubView::CopilotAuthPicker { .. } => CopilotAuthKind::ALL.len() as u16,
            ProvidersSubView::Wizard(w) => w.row_count() as u16,
        };
        2 + content_rows + 1
    }
}

impl Widget for ProviderConfigWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match &self.state.sub {
            ProvidersSubView::List { selected_idx } => {
                render_list(buf, area, &self.state.providers, *selected_idx);
            }
            ProvidersSubView::TypePicker { selected_idx } => {
                render_type_picker(buf, area, *selected_idx);
            }
            ProvidersSubView::CopilotAuthPicker { selected_idx } => {
                render_copilot_auth_picker(buf, area, *selected_idx);
            }
            ProvidersSubView::Wizard(wizard) => {
                render_wizard(buf, area, wizard);
            }
        }
    }
}

fn render_list(
    buf: &mut Buffer,
    area: Rect,
    providers: &[gantry_core::ProviderConfig],
    selected_idx: usize,
) {
    let block = Block::default()
        .title(" Providers ")
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(Color::DarkGray));
    block.render(area, buf);

    let inner = inner_area(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let footer_y = inner.bottom().saturating_sub(1);
    let list_area = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(1),
    );

    if providers.is_empty() {
        let msg = "No providers configured";
        buf.set_string(
            list_area.x,
            list_area.y,
            msg,
            Style::default().fg(Color::DarkGray),
        );
    } else {
        for (i, provider) in providers.iter().enumerate() {
            if list_area.y + i as u16 >= list_area.bottom() {
                break;
            }
            let y = list_area.y + i as u16;
            let is_selected = i == selected_idx;
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            let type_label = provider_type_label(provider);
            let line = format!("{:<20} {}", provider.alias().as_str(), type_label);
            let padded = format!("{:<width$}", line, width = inner.width as usize);
            buf.set_string(list_area.x, y, &padded, style);
        }
    }

    let footer = " a add   d delete   Esc close ";
    buf.set_string(
        inner.x,
        footer_y,
        footer,
        Style::default().fg(Color::DarkGray),
    );
}

fn render_type_picker(buf: &mut Buffer, area: Rect, selected_idx: usize) {
    let block = Block::default()
        .title(" Add Provider — Choose Type ")
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(Color::DarkGray));
    block.render(area, buf);

    let inner = inner_area(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let footer_y = inner.bottom().saturating_sub(1);
    let list_area = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(1),
    );

    for (i, kind) in WizardProviderKind::ALL.iter().enumerate() {
        if list_area.y + i as u16 >= list_area.bottom() {
            break;
        }
        let y = list_area.y + i as u16;
        let is_selected = i == selected_idx;
        let style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        };
        let padded = format!("{:<width$}", kind.label(), width = inner.width as usize);
        buf.set_string(list_area.x, y, &padded, style);
    }

    let footer = " ↑↓ navigate   Enter select   Esc back ";
    buf.set_string(
        inner.x,
        footer_y,
        footer,
        Style::default().fg(Color::DarkGray),
    );
}

fn render_copilot_auth_picker(buf: &mut Buffer, area: Rect, selected_idx: usize) {
    let block = Block::default()
        .title(" Add Provider — GitHub Copilot — Choose Auth ")
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(Color::DarkGray));
    block.render(area, buf);

    let inner = inner_area(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let footer_y = inner.bottom().saturating_sub(1);
    let list_area = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(1),
    );

    for (i, kind) in CopilotAuthKind::ALL.iter().enumerate() {
        if list_area.y + i as u16 >= list_area.bottom() {
            break;
        }
        let y = list_area.y + i as u16;
        let is_selected = i == selected_idx;
        let style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        };
        let padded = format!("{:<width$}", kind.label(), width = inner.width as usize);
        buf.set_string(list_area.x, y, &padded, style);
    }

    let footer = " ↑↓ navigate   Enter select   Esc back ";
    buf.set_string(
        inner.x,
        footer_y,
        footer,
        Style::default().fg(Color::DarkGray),
    );
}

fn render_wizard(buf: &mut Buffer, area: Rect, wizard: &ProviderWizard) {
    let title = format!(" Add Provider — {} ", wizard.kind.label());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(Color::DarkGray));
    block.render(area, buf);

    let inner = inner_area(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // Reserve footer (1), optional error line (1), and optional provider note (1).
    let has_error = wizard.error.is_some();
    let has_note = wizard.kind == WizardProviderKind::Copilot
        && wizard.copilot_auth != Some(CopilotAuthKind::ApiKey);
    let reserved = 1 + if has_error { 1 } else { 0 } + if has_note { 1 } else { 0 };
    let footer_y = inner.bottom().saturating_sub(1);
    let error_y = footer_y.saturating_sub(if has_error { 1 } else { 0 });
    let note_y = error_y.saturating_sub(if has_note { 1 } else { 0 });
    let content_height = inner.height.saturating_sub(reserved as u16);
    let content_area = Rect::new(inner.x, inner.y, inner.width, content_height);

    let label_width = wizard
        .fields
        .iter()
        .map(|f| f.label.len())
        .max()
        .unwrap_or(0)
        + 2; // ": " suffix

    for (i, field) in wizard.fields.iter().enumerate() {
        if content_area.y + i as u16 >= content_area.bottom() {
            break;
        }
        let y = content_area.y + i as u16;
        let is_focused = i == wizard.focused_idx;

        let req_marker = if field.required { "*" } else { " " };
        let label = format!(
            "{}{:<width$}",
            req_marker,
            format!("{}:", field.label),
            width = label_width
        );

        let label_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        buf.set_string(content_area.x, y, &label, label_style);

        let value_x = content_area.x + 1 + label_width as u16;
        let value_width = inner.width.saturating_sub(1 + label_width as u16) as usize;
        let value_style = if is_focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let display = format!("{:<width$}", field.value, width = value_width);
        buf.set_string(value_x, y, &display, value_style);

        // Draw cursor block on the focused field.
        if is_focused {
            let cursor_col = field
                .value
                .chars()
                .count()
                .min(value_width.saturating_sub(1));
            let cursor_x = value_x + cursor_col as u16;
            if cursor_x < inner.right()
                && let Some(cell) = buf.cell_mut((cursor_x, y))
            {
                cell.set_char('█')
                    .set_style(Style::default().fg(Color::White));
            }
        }
    }

    // Confirm row.
    let confirm_y = content_area.y + wizard.fields.len() as u16;
    if confirm_y < content_area.bottom() {
        let is_focused = wizard.is_on_confirm();
        let style = if is_focused {
            Style::default().fg(Color::Black).bg(Color::LightGreen)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let padded = format!("{:<width$}", "[ Confirm ]", width = inner.width as usize);
        buf.set_string(content_area.x, confirm_y, &padded, style);
    }

    if has_note {
        let note = " Requires GitHub CLI (`gh`). Run `gh auth login` if not authenticated. ";
        buf.set_string(inner.x, note_y, note, Style::default().fg(Color::Yellow));
    }

    if has_error && let Some(ref msg) = wizard.error {
        buf.set_string(inner.x, error_y, msg, Style::default().fg(Color::Red));
    }

    let footer = " ↑↓ navigate   Enter edit/confirm   Esc back ";
    buf.set_string(
        inner.x,
        footer_y,
        footer,
        Style::default().fg(Color::DarkGray),
    );
}

fn inner_area(area: Rect) -> Rect {
    Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    )
}

fn provider_type_label(config: &gantry_core::ProviderConfig) -> &'static str {
    match config {
        gantry_core::ProviderConfig::Ollama(_) => "ollama",
        gantry_core::ProviderConfig::Copilot(_) => "copilot",
        gantry_core::ProviderConfig::OpenAiCompletions(_) => "openai-completions",
        gantry_core::ProviderConfig::OpenAiResponses(_) => "openai-responses",
        gantry_core::ProviderConfig::Cortecs(_) => "cortecs",
    }
}
