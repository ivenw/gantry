pub mod app;
pub mod config;
pub mod dirs;
pub mod fs;
pub mod message;
pub mod resource_loader;
pub mod provider;
pub mod session;
pub mod system_prompt;
pub mod tools;

pub use app::App;
pub use config::{
    ConfigLoader, Credential, CredentialsCatalog, OllamaProviderConfig, Project, ProviderConfig,
    ProviderConfigCatalog,
};
pub use message::{Message, UserId};
pub use provider::agent::{ChatStream, ChatStreamItem};
pub use provider::registry::ProviderClientRegistry;
pub use provider::{ModelAlias, ModelSelection, ProviderAlias};
pub use rig::agent::{MultiTurnStreamItem, StreamingError};
pub use rig::streaming::StreamedAssistantContent;

pub use fs::FsSessionRegistry;
pub use session::{Branch, NodeId, Session, SessionId, SessionInfo, SessionRegistry, SessionTree};
