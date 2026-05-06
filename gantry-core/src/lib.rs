pub mod app;
pub mod dirs;
pub mod fs;
pub mod message;
pub mod project;
pub mod provider;
pub mod session;
pub mod system_prompt;
pub mod tools;

pub use app::App;
pub use message::{Message, UserId};
pub use provider::agent::{ChatStream, ChatStreamItem};
pub use provider::agent_factory::AgentFactory;
pub use provider::{
    ConfiguredModel, ModelId, ModelSelection, OllamaProviderConfig, ProviderConfig,
    ProviderConfigCatalog, ProviderId,
};
pub use rig::agent::{MultiTurnStreamItem, StreamingError};
pub use rig::streaming::StreamedAssistantContent;

pub use fs::FsSessionRegistry;
pub use session::{Branch, NodeId, Session, SessionId, SessionInfo, SessionRegistry, SessionTree};
