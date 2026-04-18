pub mod agent_factory;
pub mod event_bus;
pub mod project_registry;
pub mod provider_config;
pub mod resource_loader;
pub mod service;
pub mod session;
pub mod state;
pub mod system_prompt;
pub mod tools;
pub mod types;

pub use agent_factory::RigAgentFactory;
pub use provider_config::{
    ConfiguredModel, ModelId, ModelSelection, OllamaProviderConfig, ProviderConfig,
    ProviderConfigCatalog, ProviderId,
};
pub use service::AppService;
pub use session::manager::SessionManager;
pub use session::store::SessionInfo;
pub use types::{
    AppEvent, Branch, BranchNode, ErrorEvent, FormHiddenEvent, FormShownEvent, FormState,
    InitEvent, Message, MessageReceivedEvent, PendingClearedEvent, PendingMessage, Role,
    SelectFormRequest, SelectFormResponse, SessionTree, StreamEndEvent, StreamMessageRequest,
    StreamStartEvent, TokenEvent,
};
