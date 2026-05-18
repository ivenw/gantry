/// Gantry's domain message types.
///
/// These are thin wrappers over rig's message types, adding participant identity
/// to user messages. Rig's content types (`UserContent`, `AssistantContent`, etc.)
/// are reused directly — we only own the envelope, not the content model.
///
/// Use [`Message::into_rig`] to produce a `rig::message::Message` suitable for
/// passing to the agent, applying sender name-prefixing at that boundary.
use std::path::PathBuf;

use rig::message::{AssistantContent, UserContent};
use rig::one_or_many::OneOrMany;
use serde::{Deserialize, Serialize};

/// A participant identifier attached to user messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(String);

impl UserId {
    /// Creates a new `UserId` from any string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A gantry conversation message.
///
/// Mirrors the role structure of `rig::message::Message` but omits `System` (handled
/// separately as a preamble) and adds `sender` to `User` for multi-participant support.
// TODO: I need to rethink if the system prompt shouldn't be saved. All following assistant content
// is derived from it so it is dishonest to an extend to not persist it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    /// A message from a human participant.
    User {
        /// The participant who sent this message. `None` in single-user sessions.
        sender: Option<UserId>,
        content: OneOrMany<UserContent>,
        /// Attachments loaded alongside this message. Stored here so they can be
        /// replayed to the agent on session restore without re-reading from disk,
        /// and rendered as labels in the chat UI.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<Attachment>,
    },
    /// A message produced by the assistant.
    Assistant {
        /// Correlates tool calls with their results; mirrors `rig::message::Message::Assistant`.
        id: Option<String>,
        content: OneOrMany<AssistantContent>,
    },
}

impl Message {
    // TODO: I may want to rethink the constructor api a lot of methods that maybe should be rolled
    // up with Option arguments.
    /// Creates a user message with no sender and plain text content.
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            sender: None,
            content: OneOrMany::one(UserContent::text(text)),
            attachments: Vec::new(),
        }
    }

    /// Creates a user message with no sender, plain text content, and loaded attachments.
    pub fn user_with_attachments(text: impl Into<String>, attachments: Vec<Attachment>) -> Self {
        Self::User {
            sender: None,
            content: OneOrMany::one(UserContent::text(text)),
            attachments,
        }
    }

    /// Creates a user message attributed to `sender` with plain text content.
    pub fn user_from(sender: UserId, text: impl Into<String>) -> Self {
        Self::User {
            sender: Some(sender),
            content: OneOrMany::one(UserContent::text(text)),
            attachments: Vec::new(),
        }
    }

    /// Creates an assistant message with plain text content.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::Assistant {
            id: None,
            content: OneOrMany::one(AssistantContent::text(text)),
        }
    }

    /// Creates an assistant message from structured content.
    pub fn assistant_content(content: OneOrMany<AssistantContent>) -> Self {
        Self::Assistant { id: None, content }
    }

    /// Creates a user message wrapping a tool result.
    pub fn tool_result(call_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self::User {
            sender: None,
            content: OneOrMany::one(UserContent::tool_result(
                call_id,
                OneOrMany::one(rig::message::ToolResultContent::text(output)),
            )),
            attachments: Vec::new(),
        }
    }

    /// Extracts the first text string from this message for display purposes.
    ///
    /// For user messages this is the body text only — attachments are excluded.
    pub fn text(&self) -> String {
        match self {
            Message::User { content, .. } => content
                .iter()
                .find_map(|c| match c {
                    UserContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
            Message::Assistant { content, .. } => content
                .iter()
                .find_map(|c| match c {
                    AssistantContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
        }
    }

    /// Returns the attachments on this message, if it is a user message.
    pub fn attachments(&self) -> &[Attachment] {
        match self {
            Message::User { attachments, .. } => attachments,
            Message::Assistant { .. } => &[],
        }
    }
}

impl From<Message> for rig::message::Message {
    /// Converts a [`Message`] into a `rig::message::Message` for use with the agent.
    ///
    /// Attachment XML blocks are appended to the text body so the agent sees the full
    /// content. When `sender` is set, the sender name is prepended to the first text
    /// content item so the model can distinguish participants.
    fn from(msg: Message) -> Self {
        match msg {
            Message::User {
                sender,
                content,
                attachments,
            } => {
                let content = if attachments.is_empty() {
                    content
                } else {
                    append_attachments_to_content(content, &attachments)
                };
                let content = match sender {
                    None => content,
                    Some(id) => prefix_user_content(content, id.as_str()),
                };
                rig::message::Message::User { content }
            }
            Message::Assistant { id, content } => rig::message::Message::Assistant { id, content },
        }
    }
}

/// An attachment that was loaded alongside a user message.
///
/// Carries both the display label (name or path) and the full content so the agent
/// receives the attachment on every replay without re-reading from disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Attachment {
    /// A skill loaded from a `SKILL.md` file.
    Skill { name: String, content: String },
    /// A file read from disk.
    File { path: PathBuf, content: String },
    /// A directory listing.
    Dir { path: PathBuf, content: String },
}

impl Attachment {
    /// Renders the attachment as an XML block for inclusion in the agent message.
    fn to_xml(&self) -> String {
        match self {
            Attachment::Skill { name, content } => {
                format!(
                    "<attachment type=\"skill\" name=\"{}\">\n{}\n</attachment>",
                    name, content
                )
            }
            Attachment::File { path, content } => {
                format!(
                    "<attachment type=\"file\" path=\"{}\">\n{}\n</attachment>",
                    path.display(),
                    content
                )
            }
            Attachment::Dir { path, content } => {
                format!(
                    "<attachment type=\"dir\" path=\"{}\">\n{}\n</attachment>",
                    path.display(),
                    content
                )
            }
        }
    }
}

/// Appends attachment XML blocks to the first `Text` item in `content`.
///
/// If no text item exists, a new one is prepended with only the attachment XML.
fn append_attachments_to_content(
    content: OneOrMany<UserContent>,
    attachments: &[Attachment],
) -> OneOrMany<UserContent> {
    let xml: String = attachments
        .iter()
        .map(|a| format!("\n{}", a.to_xml()))
        .collect();
    let mut items: Vec<UserContent> = content.into_iter().collect();
    let mut appended = false;
    for item in &mut items {
        if let UserContent::Text(t) = item {
            t.text.push_str(&xml);
            appended = true;
            break;
        }
    }
    if !appended {
        items.insert(0, UserContent::text(xml.trim_start().to_string()));
    }
    OneOrMany::many(items).unwrap_or_else(|_| OneOrMany::one(UserContent::text(String::new())))
}

/// Prepends `"name: "` to the first `Text` item in `content`, leaving all other items intact.
fn prefix_user_content(content: OneOrMany<UserContent>, name: &str) -> OneOrMany<UserContent> {
    let mut items: Vec<UserContent> = content.into_iter().collect();
    let mut prefixed = false;
    for item in &mut items {
        if let UserContent::Text(t) = item {
            t.text = format!("{}: {}", name, t.text);
            prefixed = true;
            break;
        }
    }
    // If there was no text item, prepend a standalone label so the model still sees the sender.
    if !prefixed {
        items.insert(0, UserContent::text(format!("{}:", name)));
    }
    OneOrMany::many(items).unwrap_or_else(|_| OneOrMany::one(UserContent::text(name)))
}
