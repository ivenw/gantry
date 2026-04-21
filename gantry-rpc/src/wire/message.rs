use rig::message::{AssistantContent, Message, Text, ToolResult, ToolResultContent, UserContent};
use rig::one_or_many::OneOrMany;
use serde::{Deserialize, Serialize};

/// Wire representation of a rig [`Message`] for RPC transport.
///
/// The `ToolCall` variant (assistant messages carrying only a tool call) is omitted — it is
/// in-memory only on the server and never sent to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireMessage {
    User {
        content: String,
    },
    Assistant {
        content: String,
    },
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        content: String,
    },
}

/// Converts a rig [`Message`] to a [`WireMessage`], returning `None` for tool-call-only assistant
/// messages which are in-memory only and never leave the server.
pub fn to_wire(msg: &Message) -> Option<WireMessage> {
    match msg {
        Message::User { content } => {
            // Extract plain text turns; skip tool result turns (not rendered on client).
            let text = content.iter().find_map(|c| match c {
                UserContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })?;
            Some(WireMessage::User { content: text })
        }
        Message::Assistant { content, .. } => {
            let text = content.iter().find_map(|c| match c {
                AssistantContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })?;
            Some(WireMessage::Assistant { content: text })
        }
        Message::System { .. } => None,
    }
}

impl From<Message> for WireMessage {
    /// Converts a rig [`Message`] to a [`WireMessage`], using a default empty `User` message for
    /// variants that have no wire representation (tool-call-only assistant turns, system messages).
    fn from(msg: Message) -> Self {
        to_wire(&msg).unwrap_or(WireMessage::User {
            content: String::new(),
        })
    }
}

impl From<WireMessage> for Message {
    fn from(msg: WireMessage) -> Self {
        match msg {
            WireMessage::User { content } => Message::User {
                content: OneOrMany::one(UserContent::Text(Text { text: content })),
            },
            WireMessage::Assistant { content } => Message::Assistant {
                id: None,
                content: OneOrMany::one(AssistantContent::Text(Text { text: content })),
            },
            WireMessage::ToolResult {
                tool_call_id,
                tool_name,
                content,
            } => {
                let tr = ToolResult {
                    id: tool_name,
                    call_id: Some(tool_call_id),
                    content: OneOrMany::one(ToolResultContent::Text(Text { text: content })),
                };
                Message::User {
                    content: OneOrMany::one(UserContent::ToolResult(tr)),
                }
            }
        }
    }
}
