pub mod app;
pub mod config;
pub mod dirs;
pub mod metrics;
pub mod fs;
pub mod message;
pub mod resource_loader;
pub mod provider;
pub mod session;
pub mod system_prompt;
pub mod tools;

pub use app::App;
pub use config::{
    Credential, CredentialsCatalog, CredentialsRepository, CopilotProviderConfig,
    OllamaProviderConfig, OpenAiCompletionsProviderConfig, OpenAiResponsesProviderConfig,
    ProjectConfig, ProviderConfig, ProviderConfigCatalog, ProviderConfigRepository, StoredCredential,
};
pub use dirs::{GlobalConfigDir, ProjectRootDir};
pub use message::{Message, UserId};
pub use provider::agent::{ChatStream, ChatStreamItem};
pub use provider::ToolCallEvent;
pub use provider::registry::ProviderClientRegistry;
pub use provider::{ModelAlias, ModelSelection, ProviderAlias};
pub use rig::agent::{MultiTurnStreamItem, StreamingError};
pub use metrics::{CharCounts, ContextWindow, TokenBreakdown};
pub use rig::completion::Usage;
pub use rig::streaming::StreamedAssistantContent;

pub use fs::FsSessionRegistry;
pub use session::{Branch, NodeId, Session, SessionId, SessionInfo, SessionRegistry, SessionTree};
