/// Gantry's domain message types.
///
/// These are thin wrappers over rig's message types, adding participant identity
/// to user messages. Rig's content types (`UserContent`, `AssistantContent`, etc.)
/// are reused directly — we only own the envelope, not the content model.
///
/// Use [`Message::into_rig`] to produce a `rig::message::Message` suitable for
/// passing to the agent, applying sender name-prefixing at that boundary.
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    /// A message from a human participant.
    User {
        /// The participant who sent this message. `None` in single-user sessions.
        sender: Option<UserId>,
        content: OneOrMany<UserContent>,
    },
    /// A message produced by the assistant.
    Assistant {
        /// Correlates tool calls with their results; mirrors `rig::message::Message::Assistant`.
        id: Option<String>,
        content: OneOrMany<AssistantContent>,
    },
}

impl Message {
    /// Creates a user message with no sender and plain text content.
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            sender: None,
            content: OneOrMany::one(UserContent::text(text)),
        }
    }

    /// Creates a user message attributed to `sender` with plain text content.
    pub fn user_from(sender: UserId, text: impl Into<String>) -> Self {
        Self::User {
            sender: Some(sender),
            content: OneOrMany::one(UserContent::text(text)),
        }
    }

    /// Creates an assistant message with plain text content.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::Assistant {
            id: None,
            content: OneOrMany::one(AssistantContent::text(text)),
        }
    }

    /// Creates a user message wrapping a tool result.
    pub fn tool_result(call_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self::User {
            sender: None,
            content: OneOrMany::one(UserContent::tool_result(
                call_id,
                OneOrMany::one(rig::message::ToolResultContent::text(output)),
            )),
        }
    }

    /// Extracts the first text string from this message for display purposes.
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
}

impl From<Message> for rig::message::Message {
    /// Converts a [`Message`] into a `rig::message::Message` for use with the agent.
    ///
    /// When `sender` is set, the sender name is prepended to the first text content item so the
    /// model can distinguish participants. All other content is passed through unchanged.
    fn from(msg: Message) -> Self {
        match msg {
            Message::User { sender, content } => {
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
