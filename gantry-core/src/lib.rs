pub mod chat;
pub mod project;
pub mod provider;
pub mod service;
pub mod session;
pub mod tools;

pub use chat::events::{
    AppEvent, ErrorEvent, InitEvent, MessageReceivedEvent, PendingClearedEvent, StreamEndEvent,
    StreamMessageRequest, StreamStartEvent, TokenEvent,
};
pub use chat::stream::StreamEvent;
pub use chat::{Message, PendingMessage, Role};
pub use provider::agent_factory::RigAgentFactory;
pub use provider::{
    ConfiguredModel, ModelId, ModelSelection, OllamaProviderConfig, ProviderConfig,
    ProviderConfigCatalog, ProviderId,
};
pub use service::{AppService, SessionHandle};
pub use session::{Branch, BranchNode, Session, SessionInfo, SessionTree};
