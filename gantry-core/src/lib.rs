pub mod app;
pub mod chat;
pub mod dirs;
pub mod fs;
pub mod project;
pub mod provider;
pub mod session;
pub mod tools;

pub use app::App;
pub use chat::events::{
    AppEvent, ErrorEvent, InitEvent, MessageReceivedEvent, PendingClearedEvent, StreamEndEvent,
    StreamMessageRequest, StreamStartEvent, TokenEvent, ToolCallStartedEvent,
    ToolResultReceivedEvent,
};
pub use chat::stream::StreamEvent;
pub use provider::agent_factory::RigAgentFactory;
pub use provider::{
    ConfiguredModel, ModelId, ModelSelection, OllamaProviderConfig, ProviderConfig,
    ProviderConfigCatalog, ProviderId,
};
pub use rig::message::Message;

/// Extracts the first text string from a rig [`Message`] for display purposes.
pub fn message_text(message: &Message) -> String {
    use rig::message::{AssistantContent, UserContent};
    match message {
        Message::User { content } => content
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
        Message::System { content } => content.clone(),
    }
}

pub use fs::FsSessionRegistry;
pub use session::{Branch, NodeId, Session, SessionId, SessionInfo, SessionRegistry, SessionTree};
