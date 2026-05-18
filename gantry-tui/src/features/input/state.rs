use std::path::Path;

use gantry_core::InputToken;

pub struct InputState {
    pub tokens: Vec<InputToken>,
    pub cursor: InputCursor,
}

impl InputState {
    /// Creates an empty input model with a single empty text token.
    pub fn new() -> Self {
        Self {
            tokens: vec![InputToken::Text(String::new())],
            cursor: InputCursor::InText {
                token_idx: 0,
                byte_offset: 0,
            },
        }
    }

    /// Inserts a character at the current cursor position within the active text token.
    ///
    /// If the cursor is on an attachment, the character is inserted into the text token
    /// immediately before the attachment.
    pub fn insert(&mut self, c: char) {
        match self.cursor.clone() {
            InputCursor::InText {
                token_idx,
                byte_offset,
            } => {
                if let InputToken::Text(ref mut text) = self.tokens[token_idx] {
                    text.insert(byte_offset, c);
                    self.cursor = InputCursor::InText {
                        token_idx,
                        byte_offset: byte_offset + c.len_utf8(),
                    };
                }
            }
            InputCursor::AtAttachment { token_idx } => {
                // Insert a new Text token before the attachment and place cursor in it.
                let new_text = c.to_string();
                let new_offset = c.len_utf8();
                self.tokens.insert(token_idx, InputToken::Text(new_text));
                self.cursor = InputCursor::InText {
                    token_idx,
                    byte_offset: new_offset,
                };
            }
        }
        self.normalize();
    }

    /// Deletes the character before the cursor, or the whole attachment token if the cursor is on one.
    pub fn delete_before_cursor(&mut self) {
        match self.cursor.clone() {
            InputCursor::InText {
                token_idx,
                byte_offset,
            } => {
                if byte_offset == 0 {
                    if token_idx == 0 {
                        return;
                    }
                    let prev_idx = token_idx - 1;
                    match &self.tokens[prev_idx] {
                        InputToken::Text(_) => {
                            // Should have been normalized; move into it.
                            let len = if let InputToken::Text(t) = &self.tokens[prev_idx] {
                                t.len()
                            } else {
                                unreachable!()
                            };
                            self.cursor = InputCursor::InText {
                                token_idx: prev_idx,
                                byte_offset: len,
                            };
                        }
                        _ => {
                            // Delete the attachment immediately rather than landing on it first.
                            self.tokens.remove(prev_idx);
                            let new_idx = prev_idx.saturating_sub(1);
                            let byte_offset = match self.tokens.get(new_idx) {
                                Some(InputToken::Text(t)) => t.len(),
                                _ => 0,
                            };
                            self.cursor = InputCursor::InText {
                                token_idx: new_idx,
                                byte_offset,
                            };
                            self.normalize();
                        }
                    }
                } else {
                    if let InputToken::Text(ref mut text) = self.tokens[token_idx] {
                        let prev = prev_char_boundary(text, byte_offset);
                        text.drain(prev..byte_offset);
                        self.cursor = InputCursor::InText {
                            token_idx,
                            byte_offset: prev,
                        };
                    }
                }
            }
            InputCursor::AtAttachment { token_idx } => {
                self.tokens.remove(token_idx);
                // Land in the text token that is now at token_idx (the one before the removed token
                // was merged into it by normalize, or the successor text token shifted down).
                let new_idx = token_idx.saturating_sub(1);
                let byte_offset = match self.tokens.get(new_idx) {
                    Some(InputToken::Text(t)) => t.len(),
                    _ => 0,
                };
                self.cursor = InputCursor::InText {
                    token_idx: new_idx,
                    byte_offset,
                };
                self.normalize();
            }
        }
    }

    /// Moves the cursor one position to the left.
    ///
    /// Attachment tokens are skipped transparently — the cursor only ever rests in Text tokens.
    pub fn move_left(&mut self) {
        let InputCursor::InText {
            token_idx,
            byte_offset,
        } = self.cursor.clone()
        else {
            return;
        };
        if byte_offset > 0 {
            if let InputToken::Text(ref text) = self.tokens[token_idx] {
                let prev = prev_char_boundary(text, byte_offset);
                self.cursor = InputCursor::InText {
                    token_idx,
                    byte_offset: prev,
                };
            }
            return;
        }
        // At the start of a text token — scan left for the previous text token, skipping attachments.
        let mut idx = token_idx;
        loop {
            if idx == 0 {
                return;
            }
            idx -= 1;
            if let InputToken::Text(t) = &self.tokens[idx] {
                self.cursor = InputCursor::InText {
                    token_idx: idx,
                    byte_offset: t.len(),
                };
                return;
            }
            // Non-text (attachment) token — keep scanning left.
        }
    }

    /// Moves the cursor one position to the right.
    ///
    /// Attachment tokens are skipped transparently — the cursor only ever rests in Text tokens.
    pub fn move_right(&mut self) {
        let InputCursor::InText {
            token_idx,
            byte_offset,
        } = self.cursor.clone()
        else {
            return;
        };
        if let InputToken::Text(ref text) = self.tokens[token_idx]
            && byte_offset < text.len()
        {
            let c = text[byte_offset..].chars().next().unwrap();
            self.cursor = InputCursor::InText {
                token_idx,
                byte_offset: byte_offset + c.len_utf8(),
            };
            return;
        }
        // At the end of a text token — scan right for the next text token, skipping attachments.
        let mut idx = token_idx;
        loop {
            idx += 1;
            if idx >= self.tokens.len() {
                return;
            }
            if matches!(self.tokens[idx], InputToken::Text(_)) {
                self.cursor = InputCursor::InText {
                    token_idx: idx,
                    byte_offset: 0,
                };
                return;
            }
            // Non-text (attachment) token — keep scanning right.
        }
    }

    /// Resets the input to an empty state.
    pub fn clear(&mut self) {
        self.tokens = vec![InputToken::Text(String::new())];
        self.cursor = InputCursor::InText {
            token_idx: 0,
            byte_offset: 0,
        };
    }

    /// Inserts an attachment token at the current cursor position.
    ///
    /// If the cursor is inside a text token, it is split at the cursor position. A new empty
    /// text token is appended after the attachment so the cursor always lands in a text token.
    pub fn insert_attachment(&mut self, token: InputToken) {
        match self.cursor.clone() {
            InputCursor::InText {
                token_idx,
                byte_offset,
            } => {
                let tail = if let InputToken::Text(ref mut text) = self.tokens[token_idx] {
                    text.split_off(byte_offset)
                } else {
                    String::new()
                };
                let attach_idx = token_idx + 1;
                self.tokens.insert(attach_idx, token);
                self.tokens
                    .insert(attach_idx + 1, InputToken::Text(format!(" {tail}")));
                self.cursor = InputCursor::InText {
                    token_idx: attach_idx + 1,
                    byte_offset: 1,
                };
            }
            InputCursor::AtAttachment { token_idx } => {
                self.tokens.insert(token_idx, token);
                let new_text_idx = token_idx + 1;
                self.tokens
                    .insert(new_text_idx, InputToken::Text(" ".to_string()));
                self.cursor = InputCursor::InText {
                    token_idx: new_text_idx,
                    byte_offset: 1,
                };
            }
        }
        self.normalize();
    }

    /// Replaces the trailing `sigil_and_filter_len` bytes before the cursor with an attachment token.
    ///
    /// Used when the user confirms a picker selection: the sigil + filter text already in the
    /// input is stripped and the chosen token is inserted in its place.
    pub fn replace_filter_with_attachment(
        &mut self,
        sigil_and_filter_len: usize,
        token: InputToken,
    ) {
        if let InputCursor::InText {
            token_idx,
            byte_offset,
        } = self.cursor.clone()
            && let InputToken::Text(ref mut text) = self.tokens[token_idx]
        {
            let strip_start = byte_offset.saturating_sub(sigil_and_filter_len);
            let tail = text.split_off(byte_offset);
            text.truncate(strip_start);
            let attach_idx = token_idx + 1;
            self.tokens.insert(attach_idx, token);
            self.tokens
                .insert(attach_idx + 1, InputToken::Text(format!(" {tail}")));
            self.cursor = InputCursor::InText {
                token_idx: attach_idx + 1,
                byte_offset: 1,
            };
            self.normalize();
        }
    }

    /// Returns a display string with attachment sigils (`+path`, `/skill`) inlined.
    ///
    /// Path tokens are shown relative to `project_root`.
    pub fn raw_display(&self, project_root: &Path) -> String {
        let mut out = String::new();
        for token in &self.tokens {
            match token {
                InputToken::Text(t) => out.push_str(t),
                InputToken::Path(p) => {
                    let rel = p.strip_prefix(project_root).unwrap_or(p);
                    out.push('+');
                    out.push_str(&rel.display().to_string());
                }
                InputToken::Skill { name, .. } => {
                    out.push('/');
                    out.push_str(name);
                }
            }
        }
        out
    }

    /// Returns whether the effective content (raw display) is blank.
    pub fn is_blank(&self) -> bool {
        // Path tokens are non-empty regardless of prefix stripping, so project_root is irrelevant here.
        self.raw_display(Path::new("")).trim().is_empty()
    }

    /// Returns the display string and the cursor's byte offset within it, suitable for rendering.
    ///
    /// Path tokens are shown relative to `project_root`.
    pub fn display_with_cursor(&self, project_root: &Path) -> (String, usize) {
        let mut out = String::new();
        let mut cursor_byte = 0usize;
        let mut found_cursor = false;

        for (idx, token) in self.tokens.iter().enumerate() {
            let token_start = out.len();
            match token {
                InputToken::Text(t) => {
                    if !found_cursor
                        && let InputCursor::InText {
                            token_idx,
                            byte_offset,
                        } = self.cursor
                        && token_idx == idx
                    {
                        cursor_byte = token_start + byte_offset;
                        found_cursor = true;
                    }
                    out.push_str(t);
                }
                InputToken::Path(p) => {
                    let rel = p.strip_prefix(project_root).unwrap_or(p);
                    let sigil = format!("+{}", rel.display());
                    if !found_cursor
                        && let InputCursor::AtAttachment { token_idx } = self.cursor
                        && token_idx == idx
                    {
                        cursor_byte = token_start;
                        found_cursor = true;
                    }
                    out.push_str(&sigil);
                }
                InputToken::Skill { name, .. } => {
                    let sigil = format!("/{}", name);
                    if !found_cursor
                        && let InputCursor::AtAttachment { token_idx } = self.cursor
                        && token_idx == idx
                    {
                        cursor_byte = token_start;
                        found_cursor = true;
                    }
                    out.push_str(&sigil);
                }
            }
        }

        if !found_cursor {
            cursor_byte = out.len();
        }

        (out, cursor_byte)
    }

    /// Replaces all tokens with a single text token containing `text`, placing the cursor at the end.
    pub fn set_text(&mut self, text: String) {
        let len = text.len();
        self.tokens = vec![InputToken::Text(text)];
        self.cursor = InputCursor::InText {
            token_idx: 0,
            byte_offset: len,
        };
    }

    /// Restores the input buffer to a previously saved token list, placing the cursor at the end.
    pub fn restore_tokens(&mut self, tokens: Vec<InputToken>) {
        // Find the last text token to place the cursor in.
        let last_text_idx = tokens
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, t)| {
                if matches!(t, InputToken::Text(_)) {
                    Some(i)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let byte_offset = if let Some(InputToken::Text(t)) = tokens.get(last_text_idx) {
            t.len()
        } else {
            0
        };
        self.tokens = tokens;
        self.cursor = InputCursor::InText {
            token_idx: last_text_idx,
            byte_offset,
        };
    }

    /// Merges adjacent `Text` tokens and ensures the sequence starts and ends with a `Text` token.
    fn normalize(&mut self) {
        // Merge adjacent text tokens.
        let mut i = 0;
        while i + 1 < self.tokens.len() {
            if matches!(
                (&self.tokens[i], &self.tokens[i + 1]),
                (InputToken::Text(_), InputToken::Text(_))
            ) {
                let next = if let InputToken::Text(t) = self.tokens.remove(i + 1) {
                    t
                } else {
                    unreachable!()
                };
                if let InputToken::Text(ref mut cur) = self.tokens[i] {
                    // Update cursor byte_offset if it pointed into the merged token.
                    if let InputCursor::InText {
                        token_idx,
                        ref mut byte_offset,
                    } = self.cursor
                        && token_idx == i + 1
                    {
                        self.cursor = InputCursor::InText {
                            token_idx: i,
                            byte_offset: cur.len() + *byte_offset,
                        };
                    }
                    cur.push_str(&next);
                }
            } else {
                i += 1;
            }
        }

        // Ensure sequence starts with a Text token.
        if !matches!(self.tokens.first(), Some(InputToken::Text(_))) {
            self.tokens.insert(0, InputToken::Text(String::new()));
            // Shift cursor indices.
            match &mut self.cursor {
                InputCursor::InText { token_idx, .. } => *token_idx += 1,
                InputCursor::AtAttachment { token_idx } => *token_idx += 1,
            }
        }

        // Ensure sequence ends with a Text token.
        if !matches!(self.tokens.last(), Some(InputToken::Text(_))) {
            self.tokens.push(InputToken::Text(String::new()));
        }
    }
}

/// Cursor position within the token sequence.
#[derive(Debug, Clone, PartialEq)]
pub enum InputCursor {
    /// Cursor is inside a `Text` token at the given byte offset.
    InText {
        token_idx: usize,
        byte_offset: usize,
    },
    /// Cursor is positioned on an attachment token (next backspace deletes it).
    AtAttachment { token_idx: usize },
}

/// Returns the byte offset of the character boundary immediately before `cursor` in `s`.
pub fn prev_char_boundary(s: &str, cursor: usize) -> usize {
    let mut pos = cursor;
    while pos > 0 {
        pos -= 1;
        if s.is_char_boundary(pos) {
            return pos;
        }
    }
    0
}
