pub mod agent_factory;
pub mod event_bus;
pub mod provider_config;
pub mod service;
pub mod state;
pub mod tools;
pub mod types;

pub use agent_factory::RigAgentFactory;
pub use provider_config::{
    ConfiguredModel, ModelId, ModelSelection, OllamaProviderConfig, ProviderConfig,
    ProviderConfigCatalog, ProviderId,
};
pub use service::AppService;
pub use types::{
    AppEvent, ErrorEvent, FormHiddenEvent, FormShownEvent, FormState, InitEvent, Message,
    MessageReceivedEvent, PendingClearedEvent, PendingMessage, Role, SelectFormRequest,
    SelectFormResponse, StreamEndEvent, StreamMessageRequest, StreamStartEvent, TokenEvent,
};
