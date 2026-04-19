pub mod agent_factory;
pub mod chat;
pub mod event_bus;
pub mod project_registry;
pub mod provider_config;
pub mod resource_loader;
pub mod service;
pub mod session;
pub mod system_prompt;
pub mod tools;

pub use agent_factory::RigAgentFactory;
pub use chat::events::{
    AppEvent, ErrorEvent, InitEvent, MessageReceivedEvent, PendingClearedEvent, StreamEndEvent,
    StreamMessageRequest, StreamStartEvent, TokenEvent,
};
pub use chat::{Message, PendingMessage, Role};
pub use provider_config::{
    ConfiguredModel, ModelId, ModelSelection, OllamaProviderConfig, ProviderConfig,
    ProviderConfigCatalog, ProviderId,
};
pub use service::AppService;
pub use session::manager::SessionManager;
pub use session::store::SessionInfo;
pub use session::tree::{Branch, BranchNode, SessionTree};
