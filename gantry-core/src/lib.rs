pub mod chat;
pub mod event_bus;
pub mod project;
pub mod provider;
pub mod service;
pub mod session;
pub mod tools;

pub use chat::events::{
    AppEvent, ErrorEvent, InitEvent, MessageReceivedEvent, PendingClearedEvent, StreamEndEvent,
    StreamMessageRequest, StreamStartEvent, TokenEvent,
};
pub use chat::{Message, PendingMessage, Role};
pub use provider::agent_factory::RigAgentFactory;
pub use provider::{
    ConfiguredModel, ModelId, ModelSelection, OllamaProviderConfig, ProviderConfig,
    ProviderConfigCatalog, ProviderId,
};
pub use service::AppService;
pub use session::manager::SessionManager;
pub use session::registry::SessionInfo;
pub use session::tree::{Branch, BranchNode, SessionTree};
